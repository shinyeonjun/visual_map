use super::{CanonicalFactBundleArtifact, BUNDLE_ARTIFACT_SCHEMA};
use codebase_fact_model::coverage::{
    AnalysisCapability, AnalysisErrorCode, AnalysisGap, AnalysisIssue, AnalysisScope,
    AnalysisStage, AnalysisUnitReceipt, CapabilityReceipt, FileCoverageRecord, GapCode,
    SourceScopeCoverageRecord,
};
use codebase_fact_model::evidence::{EvidenceLocation, FactEvidence};
use codebase_fact_model::fact_graph::{
    DispatchKind, FactBundleManifest, FactEdge, FactNode, FactRoleAssignment, FactTruth,
    ResolutionMethod, Visibility,
};
use codebase_fact_model::identity::{
    AnalysisUnitId, EvidenceId, FactNodeId, ProviderSymbolId, Sha256Digest, SnapshotId,
};
use codebase_fact_model::language_ir::IrDefinition;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::validation::Validate;
use codebase_fact_model::ContractSchema;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SEMANTIC_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.canonical-bundle.semantic.v1\0";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct BundleStore {
    connection: Option<Connection>,
    temporary_path: PathBuf,
    output_root: PathBuf,
    snapshot_id: SnapshotId,
    merged_node_count: u64,
    merged_edge_count: u64,
    committed: bool,
}

pub(super) struct BundleFinalizationInput {
    pub(super) workspace_id: codebase_fact_model::identity::WorkspaceId,
    pub(super) source_manifest_digest: Sha256Digest,
    pub(super) config_digest: Sha256Digest,
    pub(super) analysis_plan_digest: Sha256Digest,
    pub(super) provider_set_digest: Sha256Digest,
    pub(super) execution_context_set_digest: Sha256Digest,
}

pub(super) struct BundleFinalization {
    pub(super) manifest: FactBundleManifest,
    pub(super) artifact: CanonicalFactBundleArtifact,
}

impl BundleStore {
    pub(super) fn create(
        project_root: &Path,
        output_root: &Path,
        snapshot_id: SnapshotId,
    ) -> Result<Self, String> {
        fs::create_dir_all(output_root).map_err(|error| {
            format!(
                "cannot create canonical bundle root {}: {error}",
                output_root.display()
            )
        })?;
        let canonical_output_root = output_root.canonicalize().map_err(|error| {
            format!(
                "cannot resolve canonical bundle root {}: {error}",
                output_root.display()
            )
        })?;
        let canonical_project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        if canonical_output_root.starts_with(&canonical_project_root) {
            return Err(
                "canonical import bundles must be written outside the selected repository"
                    .to_string(),
            );
        }
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_path = canonical_output_root.join(format!(
            ".canonical-{}-{}-{sequence}.sqlite.tmp",
            std::process::id(),
            unix_nanos()
        ));
        let connection = Connection::open(&temporary_path).map_err(|error| {
            format!(
                "cannot create canonical staging database {}: {error}",
                temporary_path.display()
            )
        })?;
        connection
            .execute_batch(
                // This database is an unpublished staging payload. Every
                // crash leaves only a `.tmp` file, so per-row durability would
                // add unbounded latency without protecting product state. The
                // completed database is closed, fsynced, hashed, and only then
                // renamed before its manifest is published.
                "PRAGMA journal_mode=OFF;
                 PRAGMA synchronous=OFF;
                 PRAGMA locking_mode=EXCLUSIVE;
                 PRAGMA foreign_keys=OFF;
                 PRAGMA temp_store=FILE;
                 PRAGMA trusted_schema=OFF;
                 PRAGMA user_version=1;

                 CREATE TABLE bundle_metadata (
                   key TEXT PRIMARY KEY NOT NULL,
                   value TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE analysis_unit_receipts (
                   unit_id TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE evidence (
                   id TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE source_evidence_identity (
                   id TEXT PRIMARY KEY NOT NULL,
                   path TEXT NOT NULL
                 ) WITHOUT ROWID;
                 CREATE TABLE nodes (
                   id TEXT PRIMARY KEY NOT NULL,
                   snapshot_id TEXT NOT NULL,
                   kind TEXT NOT NULL,
                   qualified_name TEXT NOT NULL,
                   display_name TEXT NOT NULL,
                   language TEXT NOT NULL DEFAULT '',
                   parent_id TEXT,
                   definition_evidence_id TEXT,
                   relevant INTEGER NOT NULL CHECK (relevant IN (0, 1)),
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE node_evidence (
                   node_id TEXT NOT NULL,
                   evidence_id TEXT NOT NULL,
                   PRIMARY KEY(node_id, evidence_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE edges (
                   id TEXT PRIMARY KEY NOT NULL,
                   snapshot_id TEXT NOT NULL,
                   source_id TEXT NOT NULL,
                   target_id TEXT NOT NULL,
                   kind TEXT NOT NULL,
                   semantic_context_id TEXT NOT NULL DEFAULT '',
                   qualifier TEXT NOT NULL DEFAULT '',
                   execution_site_id TEXT NOT NULL DEFAULT '',
                   truth TEXT NOT NULL,
                   payload_json TEXT NOT NULL,
                   UNIQUE(source_id, target_id, kind, semantic_context_id, qualifier, execution_site_id)
                 ) STRICT;
                 CREATE TABLE edge_evidence (
                   edge_id TEXT NOT NULL,
                   evidence_id TEXT NOT NULL,
                   PRIMARY KEY(edge_id, evidence_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE file_coverage (
                   record_key TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE source_scope_coverage (
                   path TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE capability_receipts (
                   record_key TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE gaps (
                   record_key TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE gap_evidence (
                   gap_key TEXT NOT NULL,
                   evidence_id TEXT NOT NULL,
                   PRIMARY KEY(gap_key, evidence_id)
                 ) WITHOUT ROWID;
                 CREATE TABLE issues (
                   record_key TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;

                 CREATE TABLE stream_headers (
                   unit_id TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE stream_completions (
                   unit_id TEXT PRIMARY KEY NOT NULL,
                   payload_json TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE file_identity (
                   unit_id TEXT NOT NULL,
                   language TEXT NOT NULL,
                   path TEXT NOT NULL,
                   node_id TEXT NOT NULL,
                   PRIMARY KEY(unit_id, language, path)
                 ) WITHOUT ROWID;
                 CREATE INDEX file_identity_path_idx ON file_identity(path, language, unit_id);
                 CREATE TABLE structure_identity (
                   unit_id TEXT NOT NULL,
                   kind TEXT NOT NULL,
                   qualified_name TEXT NOT NULL,
                   node_id TEXT NOT NULL,
                   PRIMARY KEY(unit_id, kind, qualified_name)
                 ) WITHOUT ROWID;
                 CREATE TABLE definition_identity (
                   unit_id TEXT NOT NULL,
                   symbol_id TEXT NOT NULL,
                   node_id TEXT NOT NULL,
                   definition_json TEXT NOT NULL,
                   PRIMARY KEY(unit_id, symbol_id)
                 ) WITHOUT ROWID;
                 CREATE INDEX definition_identity_node_idx ON definition_identity(node_id, unit_id, symbol_id);
                 CREATE INDEX definition_identity_symbol_idx ON definition_identity(symbol_id, unit_id);
                 CREATE INDEX nodes_parent_idx ON nodes(parent_id);
                 CREATE INDEX nodes_display_name_idx ON nodes(display_name COLLATE NOCASE, kind, id);
                 CREATE INDEX nodes_qualified_name_idx ON nodes(qualified_name COLLATE NOCASE, kind, id);
                 CREATE INDEX edges_source_idx ON edges(source_id, kind);
                 CREATE INDEX edges_target_idx ON edges(target_id, kind);",
            )
            .map_err(|error| format!("cannot initialize canonical bundle schema: {error}"))?;
        connection
            .execute(
                "INSERT INTO bundle_metadata(key, value) VALUES ('schema', ?1)",
                ["codebase-workspace.canonical-fact-bundle.sqlite.v1"],
            )
            .map_err(|error| format!("cannot seal canonical bundle schema: {error}"))?;
        connection
            .execute(
                "INSERT INTO bundle_metadata(key, value) VALUES ('snapshot_id', ?1)",
                [snapshot_id.as_str()],
            )
            .map_err(|error| format!("cannot seal canonical bundle snapshot: {error}"))?;
        connection
            .execute_batch("BEGIN IMMEDIATE TRANSACTION;")
            .map_err(|error| format!("cannot begin canonical bundle build transaction: {error}"))?;
        Ok(Self {
            connection: Some(connection),
            temporary_path,
            output_root: canonical_output_root,
            snapshot_id,
            merged_node_count: 0,
            merged_edge_count: 0,
            committed: false,
        })
    }

    fn connection(&self) -> Result<&Connection, String> {
        self.connection
            .as_ref()
            .ok_or_else(|| "canonical bundle database is already closed".to_string())
    }

    pub(super) fn insert_header<T: Serialize>(
        &self,
        unit_id: &AnalysisUnitId,
        header: &T,
    ) -> Result<(), String> {
        insert_exact(
            self.connection()?,
            "stream_headers",
            "unit_id",
            unit_id.as_str(),
            &to_json(header)?,
        )
    }

    pub(super) fn insert_completion<T: Serialize>(
        &self,
        unit_id: &AnalysisUnitId,
        completion: &T,
    ) -> Result<(), String> {
        insert_exact(
            self.connection()?,
            "stream_completions",
            "unit_id",
            unit_id.as_str(),
            &to_json(completion)?,
        )
    }

    pub(super) fn insert_analysis_unit_receipt(
        &self,
        receipt: &AnalysisUnitReceipt,
    ) -> Result<(), String> {
        receipt
            .validate()
            .map_err(|error| format!("invalid analysis-unit receipt: {error}"))?;
        insert_exact(
            self.connection()?,
            "analysis_unit_receipts",
            "unit_id",
            receipt.unit.id.as_str(),
            &to_json(receipt)?,
        )
    }

    pub(super) fn insert_evidence(&self, evidence: &FactEvidence) -> Result<(), String> {
        evidence
            .validate()
            .map_err(|error| format!("invalid canonical evidence: {error}"))?;
        let connection = self.connection()?;
        let payload = to_json(evidence)?;
        let inserted = connection
            .execute(
                "INSERT INTO evidence(id, payload_json) VALUES (?1, ?2)
                 ON CONFLICT(id) DO NOTHING",
                params![evidence.id.as_str(), payload],
            )
            .map_err(|error| format!("cannot write canonical evidence: {error}"))?;
        if inserted == 1 {
            self.register_source_evidence_identity(evidence)?;
            return Ok(());
        }

        // Identity collisions are exceptional. Keep the expensive JSON read
        // and merge on that path instead of paying it for every unique source
        // evidence record in a large repository.
        let existing = select_payload(connection, "evidence", "id", evidence.id.as_str())?
            .ok_or_else(|| format!("evidence {} disappeared after conflict", evidence.id))?;
        let mut merged: FactEvidence = from_json(&existing)?;
        if merged.id != evidence.id
            || merged.kind != evidence.kind
            || merged.producer != evidence.producer
            || merged.location != evidence.location
        {
            return Err(format!("evidence identity collision for {}", evidence.id));
        }
        merged.summary = merge_optional_text(merged.summary, evidence.summary.clone());
        let merged_payload = to_json(&merged)?;
        if merged_payload != existing {
            connection
                .execute(
                    "UPDATE evidence SET payload_json=?2 WHERE id=?1",
                    params![merged.id.as_str(), merged_payload],
                )
                .map_err(|error| format!("cannot merge canonical evidence: {error}"))?;
        }
        Ok(())
    }

    fn register_source_evidence_identity(&self, evidence: &FactEvidence) -> Result<(), String> {
        let EvidenceLocation::Source { span } = &evidence.location else {
            return Ok(());
        };
        self.connection()?
            .execute(
                "INSERT INTO source_evidence_identity(id, path) VALUES (?1, ?2)",
                params![evidence.id.as_str(), span.path.as_str()],
            )
            .map_err(|error| format!("cannot register source evidence identity: {error}"))?;
        Ok(())
    }

    pub(super) fn has_evidence(&self, id: &str) -> Result<bool, String> {
        self.connection()?
            .prepare_cached("SELECT 1 FROM evidence WHERE id=?1")
            .map_err(|error| format!("cannot prepare canonical evidence lookup: {error}"))?
            .query_row([id], |_| Ok(()))
            .optional()
            .map(|row| row.is_some())
            .map_err(|error| format!("cannot check canonical evidence identity: {error}"))
    }

    pub(super) fn source_evidence_path(&self, id: &str) -> Result<Option<RepositoryPath>, String> {
        self.connection()?
            .prepare_cached("SELECT path FROM source_evidence_identity WHERE id=?1")
            .map_err(|error| format!("cannot prepare source evidence lookup: {error}"))?
            .query_row([id], |row| row.get::<_, String>(0))
            .optional()
            .map_err(|error| format!("cannot read source evidence identity: {error}"))?
            .map(|path| {
                RepositoryPath::parse(path)
                    .map_err(|error| format!("invalid stored source evidence path: {error}"))
            })
            .transpose()
    }

    pub(super) fn insert_file_coverage(&self, coverage: &FileCoverageRecord) -> Result<(), String> {
        coverage
            .validate()
            .map_err(|error| format!("invalid file coverage: {error}"))?;
        let unit = coverage
            .unit_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "-".to_string());
        let language = coverage.language.map(|value| value.as_str()).unwrap_or("-");
        let key = format!("{unit}\0{language}\0{}", coverage.path.as_str());
        insert_exact(
            self.connection()?,
            "file_coverage",
            "record_key",
            &key,
            &to_json(coverage)?,
        )
    }

    pub(super) fn insert_source_scope_coverage(
        &self,
        coverage: &SourceScopeCoverageRecord,
    ) -> Result<(), String> {
        coverage
            .validate()
            .map_err(|error| format!("invalid source-scope coverage: {error}"))?;
        insert_exact(
            self.connection()?,
            "source_scope_coverage",
            "path",
            coverage.path.as_str(),
            &to_json(coverage)?,
        )
    }

    pub(super) fn insert_capability_receipt(
        &self,
        receipt: &CapabilityReceipt,
    ) -> Result<(), String> {
        receipt
            .validate()
            .map_err(|error| format!("invalid capability receipt: {error}"))?;
        let key = format!(
            "{}\0{}",
            receipt.unit_id.as_str(),
            receipt.capability.as_str()
        );
        insert_exact(
            self.connection()?,
            "capability_receipts",
            "record_key",
            &key,
            &to_json(receipt)?,
        )
    }

    pub(super) fn insert_gap(&self, gap: &AnalysisGap) -> Result<(), String> {
        gap.validate()
            .map_err(|error| format!("invalid analysis gap: {error}"))?;
        let key = Sha256Digest::of_bytes(semantic_gap_payload(gap)?.as_bytes()).to_hex();
        let connection = self.connection()?;
        let merged = match select_payload(connection, "gaps", "record_key", &key)? {
            None => gap.clone(),
            Some(payload) => {
                let mut existing: AnalysisGap = from_json(&payload)?;
                if semantic_gap_payload(&existing)? != semantic_gap_payload(gap)? {
                    return Err(format!("analysis-gap identity collision for {key}"));
                }
                existing.message = existing.message.min(gap.message.clone());
                existing
            }
        };
        connection
            .execute(
                "INSERT INTO gaps(record_key, payload_json) VALUES (?1, ?2)
                 ON CONFLICT(record_key) DO UPDATE SET payload_json=excluded.payload_json",
                params![key, to_json(&merged)?],
            )
            .map_err(|error| format!("cannot write canonical analysis gap: {error}"))?;
        for evidence_id in &gap.evidence_ids {
            connection
                .execute(
                    "INSERT OR IGNORE INTO gap_evidence(gap_key, evidence_id) VALUES (?1, ?2)",
                    params![key, evidence_id.as_str()],
                )
                .map_err(|error| format!("cannot link gap evidence: {error}"))?;
        }
        Ok(())
    }

    pub(super) fn insert_issue(&self, issue: &AnalysisIssue) -> Result<(), String> {
        issue
            .validate()
            .map_err(|error| format!("invalid analysis issue: {error}"))?;
        let key = Sha256Digest::of_bytes(semantic_issue_payload(issue)?.as_bytes()).to_hex();
        let connection = self.connection()?;
        let merged = match select_payload(connection, "issues", "record_key", &key)? {
            None => issue.clone(),
            Some(payload) => {
                let mut existing: AnalysisIssue = from_json(&payload)?;
                if semantic_issue_payload(&existing)? != semantic_issue_payload(issue)? {
                    return Err(format!("analysis-issue identity collision for {key}"));
                }
                existing.message = existing.message.min(issue.message.clone());
                existing.remediation =
                    merge_optional_text(existing.remediation, issue.remediation.clone());
                existing
            }
        };
        connection
            .execute(
                "INSERT INTO issues(record_key, payload_json) VALUES (?1, ?2)
                 ON CONFLICT(record_key) DO UPDATE SET payload_json=excluded.payload_json",
                params![key, to_json(&merged)?],
            )
            .map_err(|error| format!("cannot write canonical analysis issue: {error}"))?;
        Ok(())
    }

    pub(super) fn register_file_identity(
        &self,
        unit_id: &AnalysisUnitId,
        language: codebase_fact_model::analysis::ProgrammingLanguage,
        path: &RepositoryPath,
        node_id: &FactNodeId,
    ) -> Result<(), String> {
        let changed = self
            .connection()?
            .execute(
                "INSERT INTO file_identity(unit_id, language, path, node_id) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(unit_id, language, path) DO UPDATE SET node_id=excluded.node_id
                 WHERE file_identity.node_id=excluded.node_id",
                params![unit_id.as_str(), language.as_str(), path.as_str(), node_id.as_str()],
            )
            .map_err(|error| format!("cannot register canonical file identity: {error}"))?;
        if changed == 0 {
            return Err(format!(
                "file identity collision for {}/{}:{}",
                unit_id,
                language.as_str(),
                path
            ));
        }
        Ok(())
    }

    pub(super) fn register_definition(
        &self,
        definition: &IrDefinition,
        node_id: &FactNodeId,
    ) -> Result<bool, String> {
        definition
            .validate()
            .map_err(|error| format!("invalid IR definition before linking: {error}"))?;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "INSERT INTO definition_identity(unit_id, symbol_id, node_id, definition_json)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(unit_id, symbol_id) DO NOTHING",
                params![
                    definition.unit_id.as_str(),
                    definition.symbol_id.as_str(),
                    node_id.as_str(),
                    to_json(definition)?
                ],
            )
            .map_err(|error| format!("cannot register definition identity: {error}"))?;
        if changed == 0 {
            let existing: Option<(String, String)> = connection
                .query_row(
                    "SELECT node_id, definition_json FROM definition_identity
                     WHERE unit_id=?1 AND symbol_id=?2",
                    params![definition.unit_id.as_str(), definition.symbol_id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| format!("cannot inspect definition identity: {error}"))?;
            let Some((existing_node, existing_definition)) = existing else {
                return Err("definition identity disappeared during registration".to_string());
            };
            if existing_node != node_id.as_str() || existing_definition != to_json(definition)? {
                return Err(format!(
                    "provider symbol {} maps to conflicting canonical definitions",
                    definition.symbol_id
                ));
            }
        }
        Ok(changed > 0)
    }

    pub(super) fn definition_node_ids_page(
        &self,
        after: Option<&FactNodeId>,
        limit: usize,
    ) -> Result<Vec<FactNodeId>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT node_id FROM definition_identity
                 WHERE (?1 = '' OR node_id > ?1)
                 ORDER BY node_id LIMIT ?2",
            )
            .map_err(|error| format!("cannot read definition identities: {error}"))?;
        let rows = statement
            .query_map(
                params![
                    after.map(FactNodeId::as_str).unwrap_or_default(),
                    limit as u64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| format!("cannot iterate definition identities: {error}"))?;
        let mut result = Vec::new();
        for row in rows {
            let node_id =
                row.map_err(|error| format!("cannot decode definition identity row: {error}"))?;
            result.push(
                FactNodeId::parse(node_id)
                    .map_err(|error| format!("invalid staged canonical node ID: {error}"))?,
            );
        }
        Ok(result)
    }

    pub(super) fn definitions_for_node(
        &self,
        node_id: &FactNodeId,
    ) -> Result<Vec<IrDefinition>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT definition_json FROM definition_identity WHERE node_id=?1
                 ORDER BY unit_id, symbol_id",
            )
            .map_err(|error| format!("cannot prepare canonical definition group: {error}"))?;
        let rows = statement
            .query_map([node_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(|error| format!("cannot query canonical definition group: {error}"))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(from_json(&row.map_err(|error| {
                format!("cannot decode canonical definition group: {error}")
            })?)?);
        }
        Ok(result)
    }

    pub(super) fn resolve_local_symbol(
        &self,
        unit_id: &AnalysisUnitId,
        symbol_id: &ProviderSymbolId,
    ) -> Result<Option<FactNodeId>, String> {
        let value = self
            .connection()?
            .query_row(
                "SELECT node_id FROM definition_identity WHERE unit_id=?1 AND symbol_id=?2",
                params![unit_id.as_str(), symbol_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("cannot resolve local provider symbol: {error}"))?;
        value
            .map(|value| {
                FactNodeId::parse(value)
                    .map_err(|error| format!("invalid canonical symbol identity: {error}"))
            })
            .transpose()
    }

    pub(super) fn resolve_symbol_exact(
        &self,
        unit_id: &AnalysisUnitId,
        symbol_id: &ProviderSymbolId,
    ) -> Result<Option<FactNodeId>, String> {
        if let Some(local) = self.resolve_local_symbol(unit_id, symbol_id)? {
            return Ok(Some(local));
        }
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT node_id FROM definition_identity WHERE symbol_id=?1 ORDER BY node_id LIMIT 2",
            )
            .map_err(|error| format!("cannot prepare global provider-symbol lookup: {error}"))?;
        let rows = statement
            .query_map([symbol_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(|error| format!("cannot query global provider-symbol lookup: {error}"))?;
        let values = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot decode global provider-symbol lookup: {error}"))?;
        match values.as_slice() {
            [value] => FactNodeId::parse(value.clone())
                .map(Some)
                .map_err(|error| format!("invalid canonical symbol identity: {error}")),
            [] | [_, _, ..] => Ok(None),
        }
    }

    pub(super) fn resolve_file_exact(
        &self,
        unit_id: &AnalysisUnitId,
        language: codebase_fact_model::analysis::ProgrammingLanguage,
        path: &RepositoryPath,
    ) -> Result<Option<FactNodeId>, String> {
        let connection = self.connection()?;
        let local = connection
            .query_row(
                "SELECT node_id FROM file_identity WHERE unit_id=?1 AND language=?2 AND path=?3",
                params![unit_id.as_str(), language.as_str(), path.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("cannot resolve local file identity: {error}"))?;
        if let Some(value) = local {
            return FactNodeId::parse(value)
                .map(Some)
                .map_err(|error| format!("invalid canonical file identity: {error}"));
        }
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT node_id FROM file_identity
                 WHERE path=?1 AND language=?2 ORDER BY node_id LIMIT 2",
            )
            .map_err(|error| format!("cannot prepare same-language file lookup: {error}"))?;
        let rows = statement
            .query_map(params![path.as_str(), language.as_str()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| format!("cannot query same-language file lookup: {error}"))?;
        let same_language = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot decode same-language file lookup: {error}"))?;
        match same_language.as_slice() {
            [value] => {
                return FactNodeId::parse(value.clone())
                    .map(Some)
                    .map_err(|error| format!("invalid canonical file identity: {error}"));
            }
            [_, _, ..] => return Ok(None),
            [] => {}
        }
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT node_id FROM file_identity WHERE path=?1 ORDER BY node_id LIMIT 2",
            )
            .map_err(|error| format!("cannot prepare exact file lookup: {error}"))?;
        let rows = statement
            .query_map([path.as_str()], |row| row.get::<_, String>(0))
            .map_err(|error| format!("cannot query exact file lookup: {error}"))?;
        let values = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("cannot decode exact file lookup: {error}"))?;
        match values.as_slice() {
            [value] => FactNodeId::parse(value.clone())
                .map(Some)
                .map_err(|error| format!("invalid canonical file identity: {error}")),
            [] | [_, _, ..] => Ok(None),
        }
    }

    pub(super) fn resolve_structure_exact(
        &self,
        unit_id: &AnalysisUnitId,
        kind: codebase_fact_model::fact_graph::FactNodeKind,
        qualified_name: &str,
    ) -> Result<Option<FactNodeId>, String> {
        let value = self
            .connection()?
            .query_row(
                "SELECT node_id FROM structure_identity
                 WHERE unit_id=?1 AND kind=?2 AND qualified_name=?3",
                params![unit_id.as_str(), kind.as_str(), qualified_name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("cannot resolve canonical structure identity: {error}"))?;
        value
            .map(|value| {
                FactNodeId::parse(value)
                    .map_err(|error| format!("invalid canonical structure identity: {error}"))
            })
            .transpose()
    }

    pub(super) fn register_structure_identity(
        &self,
        unit_id: &AnalysisUnitId,
        kind: codebase_fact_model::fact_graph::FactNodeKind,
        qualified_name: &str,
        node_id: &FactNodeId,
    ) -> Result<(), String> {
        let changed = self
            .connection()?
            .execute(
                "INSERT INTO structure_identity(unit_id, kind, qualified_name, node_id)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(unit_id, kind, qualified_name) DO UPDATE SET node_id=excluded.node_id
                 WHERE structure_identity.node_id=excluded.node_id",
                params![
                    unit_id.as_str(),
                    kind.as_str(),
                    qualified_name,
                    node_id.as_str()
                ],
            )
            .map_err(|error| format!("cannot register canonical structure identity: {error}"))?;
        if changed == 0 {
            return Err(format!(
                "structure identity collision for {}/{}/{}",
                unit_id,
                kind.as_str(),
                qualified_name
            ));
        }
        Ok(())
    }

    pub(super) fn insert_node(&mut self, node: &FactNode, relevant: bool) -> Result<(), String> {
        node.validate()
            .map_err(|error| format!("invalid canonical node: {error}"))?;
        let existing = select_payload(self.connection()?, "nodes", "id", node.id.as_str())?;
        let (merged, was_merged) = match existing {
            None => (node.clone(), false),
            Some(payload) => (merge_nodes(from_json(&payload)?, node.clone())?, true),
        };
        if was_merged {
            self.merged_node_count += 1;
        }
        let connection = self.connection()?;
        let previous_relevant = connection
            .query_row(
                "SELECT relevant FROM nodes WHERE id=?1",
                [node.id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("cannot read node relevance: {error}"))?
            .unwrap_or(0);
        connection
            .execute(
                "INSERT INTO nodes(id, snapshot_id, kind, qualified_name, display_name, language, parent_id, definition_evidence_id, relevant, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                   qualified_name=excluded.qualified_name,
                   display_name=excluded.display_name,
                   language=excluded.language,
                   parent_id=excluded.parent_id,
                   definition_evidence_id=excluded.definition_evidence_id,
                   relevant=MAX(nodes.relevant, excluded.relevant),
                   payload_json=excluded.payload_json",
                params![
                    merged.id.as_str(),
                    merged.snapshot_id.as_str(),
                    merged.kind.as_str(),
                    merged.qualified_name.as_str(),
                    merged.display_name.as_str(),
                    merged
                        .language
                        .map(|language| language.as_str())
                        .unwrap_or(""),
                    merged.parent_id.as_ref().map(FactNodeId::as_str),
                    merged.definition_evidence_id.as_ref().map(ToString::to_string),
                    i64::from(relevant || previous_relevant != 0),
                    to_json(&merged)?
                ],
            )
            .map_err(|error| format!("cannot write canonical node: {error}"))?;
        connection
            .execute(
                "DELETE FROM node_evidence WHERE node_id=?1",
                [merged.id.as_str()],
            )
            .map_err(|error| format!("cannot reset node evidence: {error}"))?;
        for evidence_id in &merged.evidence_ids {
            connection
                .execute(
                    "INSERT INTO node_evidence(node_id, evidence_id) VALUES (?1, ?2)",
                    params![merged.id.as_str(), evidence_id.as_str()],
                )
                .map_err(|error| format!("cannot link node evidence: {error}"))?;
        }
        Ok(())
    }

    pub(super) fn mark_node_relevant(&self, id: &FactNodeId) -> Result<(), String> {
        self.connection()?
            .execute("UPDATE nodes SET relevant=1 WHERE id=?1", [id.as_str()])
            .map_err(|error| format!("cannot mark relation endpoint relevant: {error}"))?;
        Ok(())
    }

    pub(super) fn node(&self, id: &FactNodeId) -> Result<Option<FactNode>, String> {
        select_payload(self.connection()?, "nodes", "id", id.as_str())?
            .map(|payload| from_json(&payload))
            .transpose()
    }

    pub(super) fn insert_edge(&mut self, edge: &FactEdge) -> Result<(), String> {
        edge.validate()
            .map_err(|error| format!("invalid canonical edge: {error}"))?;
        let existing = select_payload(self.connection()?, "edges", "id", edge.id.as_str())?;
        let (merged, was_merged) = match existing {
            None => (edge.clone(), false),
            Some(payload) => (merge_edges(from_json(&payload)?, edge.clone())?, true),
        };
        if was_merged {
            self.merged_edge_count += 1;
        }
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO edges(id, snapshot_id, source_id, target_id, kind, semantic_context_id, qualifier, execution_site_id, truth, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET truth=excluded.truth, payload_json=excluded.payload_json",
                params![
                    merged.id.as_str(),
                    merged.snapshot_id.as_str(),
                    merged.source_id.as_str(),
                    merged.target_id.as_str(),
                    merged.kind.as_str(),
                    merged
                        .semantic_context_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    merged.qualifier.as_deref().unwrap_or_default(),
                    merged
                        .execution
                        .as_ref()
                        .map(|execution| execution.call_site_evidence_id.as_str())
                        .unwrap_or_default(),
                    truth_name(merged.truth),
                    to_json(&merged)?
                ],
            )
            .map_err(|error| format!("cannot write canonical edge: {error}"))?;
        connection
            .execute(
                "DELETE FROM edge_evidence WHERE edge_id=?1",
                [merged.id.as_str()],
            )
            .map_err(|error| format!("cannot reset edge evidence: {error}"))?;
        for evidence_id in &merged.evidence_ids {
            connection
                .execute(
                    "INSERT INTO edge_evidence(edge_id, evidence_id) VALUES (?1, ?2)",
                    params![merged.id.as_str(), evidence_id.as_str()],
                )
                .map_err(|error| format!("cannot link edge evidence: {error}"))?;
        }
        Ok(())
    }

    pub(super) fn retain_relevant_nodes_and_evidence(&self) -> Result<u64, String> {
        let connection = self.connection()?;
        connection
            .execute_batch(
                "WITH RECURSIVE ancestors(id) AS (
                   SELECT parent_id FROM nodes WHERE relevant=1 AND parent_id IS NOT NULL
                   UNION
                   SELECT nodes.parent_id FROM nodes JOIN ancestors ON nodes.id=ancestors.id
                   WHERE nodes.parent_id IS NOT NULL
                 )
                 UPDATE nodes SET relevant=1 WHERE id IN (SELECT id FROM ancestors);
                 DELETE FROM node_evidence WHERE node_id IN (SELECT id FROM nodes WHERE relevant=0);
                 DELETE FROM nodes WHERE relevant=0;",
            )
            .map_err(|error| format!("cannot apply canonical relevance gate: {error}"))?;
        connection
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get::<_, u64>(0))
            .map_err(|error| format!("cannot count retained canonical nodes: {error}"))
    }

    pub(super) fn retained_nodes_page(
        &self,
        after: Option<&FactNodeId>,
        limit: usize,
    ) -> Result<Vec<FactNode>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT payload_json FROM nodes
                 WHERE (?1 = '' OR id > ?1)
                 ORDER BY id LIMIT ?2",
            )
            .map_err(|error| format!("cannot prepare retained-node page: {error}"))?;
        let rows = statement
            .query_map(
                params![
                    after.map(FactNodeId::as_str).unwrap_or_default(),
                    limit as u64
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| format!("cannot query retained-node page: {error}"))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(from_json(&row.map_err(|error| {
                format!("cannot decode retained node: {error}")
            })?)?);
        }
        Ok(result)
    }

    pub(super) fn retained_definition_count(&self) -> Result<u64, String> {
        count_query(
            self.connection()?,
            "SELECT COUNT(DISTINCT d.node_id)
             FROM definition_identity d JOIN nodes n ON n.id=d.node_id",
        )
    }

    pub(super) fn canonical_definition_node_count(&self) -> Result<u64, String> {
        count_query(
            self.connection()?,
            "SELECT COUNT(DISTINCT node_id) FROM definition_identity",
        )
    }

    pub(super) fn prune_unreferenced_evidence(&self) -> Result<(), String> {
        self.connection()?
            .execute(
                "DELETE FROM evidence
                 WHERE id NOT IN (SELECT evidence_id FROM node_evidence)
                   AND id NOT IN (SELECT evidence_id FROM edge_evidence)
                   AND id NOT IN (SELECT evidence_id FROM gap_evidence)",
                [],
            )
            .map_err(|error| format!("cannot prune unreferenced evidence: {error}"))?;
        Ok(())
    }

    pub(super) fn validate_invariants(&self) -> Result<BundleInvariantCounts, String> {
        let connection = self.connection()?;
        let dangling_edge_endpoints = count_query(
            connection,
            "SELECT COUNT(*) FROM edges e
             LEFT JOIN nodes s ON s.id=e.source_id
             LEFT JOIN nodes t ON t.id=e.target_id
             WHERE s.id IS NULL OR t.id IS NULL",
        )?;
        let dangling_parents = count_query(
            connection,
            "SELECT COUNT(*) FROM nodes n LEFT JOIN nodes p ON p.id=n.parent_id
             WHERE n.parent_id IS NOT NULL AND p.id IS NULL",
        )?;
        let dangling_node_evidence = count_query(
            connection,
            "SELECT COUNT(*) FROM node_evidence ne LEFT JOIN evidence e ON e.id=ne.evidence_id
             WHERE e.id IS NULL",
        )?;
        let dangling_edge_evidence = count_query(
            connection,
            "SELECT COUNT(*) FROM edge_evidence ee LEFT JOIN evidence e ON e.id=ee.evidence_id
             WHERE e.id IS NULL",
        )?;
        let dangling_gap_evidence = count_query(
            connection,
            "SELECT COUNT(*) FROM gap_evidence ge LEFT JOIN evidence e ON e.id=ge.evidence_id
             WHERE e.id IS NULL",
        )?;
        let confirmed_without_evidence = count_query(
            connection,
            "SELECT COUNT(*) FROM edges e
             WHERE e.truth='confirmed'
               AND NOT EXISTS (SELECT 1 FROM edge_evidence ee WHERE ee.edge_id=e.id)",
        )?;
        let duplicate_logical_edges = count_query(
            connection,
            "SELECT COUNT(*) FROM (
               SELECT source_id, target_id, kind, semantic_context_id, qualifier, execution_site_id, COUNT(*) AS n
               FROM edges GROUP BY source_id, target_id, kind, semantic_context_id, qualifier, execution_site_id
               HAVING n > 1
             )",
        )?;
        let counts = BundleInvariantCounts {
            dangling_endpoint_count: dangling_edge_endpoints
                + dangling_parents
                + dangling_node_evidence
                + dangling_edge_evidence
                + dangling_gap_evidence,
            confirmed_without_evidence_count: confirmed_without_evidence,
            duplicate_logical_edge_count: duplicate_logical_edges,
        };
        if counts.dangling_endpoint_count > 0
            || counts.confirmed_without_evidence_count > 0
            || counts.duplicate_logical_edge_count > 0
        {
            return Err(format!(
                "canonical bundle invariant failure: dangling={} confirmed_without_evidence={} duplicate_edges={}",
                counts.dangling_endpoint_count,
                counts.confirmed_without_evidence_count,
                counts.duplicate_logical_edge_count
            ));
        }
        Ok(counts)
    }

    pub(super) fn merged_node_count(&self) -> u64 {
        self.merged_node_count
    }

    pub(super) fn merged_edge_count(&self) -> u64 {
        self.merged_edge_count
    }

    pub(super) fn semantic_digest(&self) -> Result<Sha256Digest, String> {
        semantic_digest(self.connection()?)
    }

    pub(super) fn count(&self, table: &str) -> Result<u64, String> {
        if !FINAL_TABLES.contains(&table) {
            return Err(format!("unknown canonical bundle table: {table}"));
        }
        count_query(self.connection()?, &format!("SELECT COUNT(*) FROM {table}"))
    }

    pub(super) fn finish(
        mut self,
        input: BundleFinalizationInput,
        semantic_digest: Sha256Digest,
    ) -> Result<BundleFinalization, String> {
        let counts = BundleCounts {
            analysis_units: self.count("analysis_unit_receipts")?,
            nodes: self.count("nodes")?,
            edges: self.count("edges")?,
            evidence: self.count("evidence")?,
            file_coverage: self.count("file_coverage")?,
            source_scope_coverage: self.count("source_scope_coverage")?,
            capabilities: self.count("capability_receipts")?,
            gaps: self.count("gaps")?,
            issues: self.count("issues")?,
        };
        let connection = self
            .connection
            .take()
            .ok_or_else(|| "canonical bundle database is already closed".to_string())?;
        connection
            .execute_batch(
                "COMMIT;
                 DROP TABLE stream_headers;
                 DROP TABLE stream_completions;
                 DROP TABLE file_identity;
                 DROP TABLE structure_identity;
                 DROP TABLE definition_identity;
                 DROP TABLE source_evidence_identity;
                 VACUUM;",
            )
            .map_err(|error| format!("cannot finalize canonical bundle schema: {error}"))?;
        connection
            .close()
            .map_err(|(_, error)| format!("cannot close canonical bundle database: {error}"))?;
        sync_file(&self.temporary_path)?;
        let bundle_digest = sha256_file(&self.temporary_path)?;
        // The payload digest is the immutable artifact identity. Snapshot and
        // semantic identity live in the manifest; keeping them out of the file
        // name also avoids Windows path-length failures on deep cache roots.
        let final_stem = format!("canonical-{bundle_digest}");
        let final_path = self.output_root.join(format!("{final_stem}.sqlite"));
        publish_immutable_payload(&self.temporary_path, &final_path, bundle_digest)?;

        let manifest = FactBundleManifest {
            schema: ContractSchema::CanonicalFactV1,
            snapshot_id: self.snapshot_id.clone(),
            workspace_id: input.workspace_id,
            source_manifest_digest: input.source_manifest_digest,
            config_digest: input.config_digest,
            analysis_plan_digest: input.analysis_plan_digest,
            provider_set_digest: input.provider_set_digest,
            execution_context_set_digest: input.execution_context_set_digest,
            semantic_digest,
            bundle_digest,
            analysis_unit_receipt_count: counts.analysis_units,
            node_count: counts.nodes,
            edge_count: counts.edges,
            evidence_count: counts.evidence,
            file_coverage_count: counts.file_coverage,
            source_scope_coverage_count: counts.source_scope_coverage,
            capability_receipt_count: counts.capabilities,
            gap_count: counts.gaps,
            issue_count: counts.issues,
            completed_at_unix_ms: unix_millis(),
        };
        manifest
            .validate()
            .map_err(|error| format!("invalid canonical bundle manifest: {error}"))?;
        let manifest_path = self.output_root.join(format!("{final_stem}.manifest.json"));
        let manifest = publish_manifest(&manifest_path, &manifest)?;
        self.committed = true;
        Ok(BundleFinalization {
            artifact: CanonicalFactBundleArtifact {
                schema: BUNDLE_ARTIFACT_SCHEMA,
                snapshot_id: self.snapshot_id.clone(),
                semantic_digest,
                bundle_digest,
                bundle_path: final_path,
                manifest_path,
            },
            manifest,
        })
    }
}

impl Drop for BundleStore {
    fn drop(&mut self) {
        if !self.committed {
            self.connection.take();
            let _ = fs::remove_file(&self.temporary_path);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BundleInvariantCounts {
    pub(super) dangling_endpoint_count: u64,
    pub(super) confirmed_without_evidence_count: u64,
    pub(super) duplicate_logical_edge_count: u64,
}

struct BundleCounts {
    analysis_units: u64,
    nodes: u64,
    edges: u64,
    evidence: u64,
    file_coverage: u64,
    source_scope_coverage: u64,
    capabilities: u64,
    gaps: u64,
    issues: u64,
}

const FINAL_TABLES: &[&str] = &[
    "analysis_unit_receipts",
    "evidence",
    "nodes",
    "edges",
    "file_coverage",
    "source_scope_coverage",
    "capability_receipts",
    "gaps",
    "issues",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticGap<'a> {
    code: &'a GapCode,
    scope: &'a AnalysisScope,
    capability: &'a Option<AnalysisCapability>,
    evidence_ids: &'a [EvidenceId],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticIssue<'a> {
    code: &'a AnalysisErrorCode,
    stage: &'a AnalysisStage,
    scope: &'a AnalysisScope,
    retryable: bool,
}

fn merge_nodes(mut left: FactNode, right: FactNode) -> Result<FactNode, String> {
    if left.id != right.id
        || left.snapshot_id != right.snapshot_id
        || left.family != right.family
        || left.kind != right.kind
        || left.qualified_name != right.qualified_name
        || left.display_name != right.display_name
        || left.signature != right.signature
        || left.details != right.details
        || left.language != right.language
        || left.analysis_unit_id != right.analysis_unit_id
    {
        return Err(format!("canonical node identity collision for {}", left.id));
    }
    left.native_kind = match (&left.native_kind, &right.native_kind) {
        (Some(left), Some(right)) if left == right => Some(left.clone()),
        (None, value) | (value, None) => value.clone(),
        (Some(_), Some(_)) => None,
    };
    left.visibility = if left.visibility == right.visibility {
        left.visibility
    } else {
        Visibility::Unknown
    };
    left.parent_id = merge_optional_exact(left.parent_id, right.parent_id, "node parent")?;
    left.definition_evidence_id = match (left.definition_evidence_id, right.definition_evidence_id)
    {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    };
    left.evidence_ids.extend(right.evidence_ids);
    left.evidence_ids.sort();
    left.evidence_ids.dedup();
    left.roles = merge_roles(left.roles, right.roles);
    left.flags.test |= right.flags.test;
    left.flags.generated |= right.flags.generated;
    left.flags.vendor |= right.flags.vendor;
    left.flags.external |= right.flags.external;
    left.validate()
        .map_err(|error| format!("invalid merged canonical node: {error}"))?;
    Ok(left)
}

fn merge_roles(
    left: Vec<FactRoleAssignment>,
    right: Vec<FactRoleAssignment>,
) -> Vec<FactRoleAssignment> {
    let mut roles = BTreeMap::new();
    for assignment in left.into_iter().chain(right) {
        roles
            .entry(assignment.role)
            .or_insert_with(Vec::new)
            .extend(assignment.evidence_ids);
    }
    roles
        .into_iter()
        .map(|(role, mut evidence_ids)| {
            evidence_ids.sort();
            evidence_ids.dedup();
            FactRoleAssignment { role, evidence_ids }
        })
        .collect()
}

fn merge_edges(mut left: FactEdge, right: FactEdge) -> Result<FactEdge, String> {
    if left.id != right.id
        || left.snapshot_id != right.snapshot_id
        || left.source_id != right.source_id
        || left.target_id != right.target_id
        || left.family != right.family
        || left.kind != right.kind
        || left.semantic_context_id != right.semantic_context_id
        || left.qualifier != right.qualifier
        || left.execution != right.execution
    {
        return Err(format!("canonical edge identity collision for {}", left.id));
    }
    left.truth = stronger_truth(left.truth, right.truth);
    left.resolution = stronger_resolution(left.resolution, right.resolution);
    left.dispatch = merge_dispatch(left.dispatch, right.dispatch);
    left.evidence_ids.extend(right.evidence_ids);
    left.evidence_ids.sort();
    left.evidence_ids.dedup();
    left.validate()
        .map_err(|error| format!("invalid merged canonical edge: {error}"))?;
    Ok(left)
}

fn stronger_truth(left: FactTruth, right: FactTruth) -> FactTruth {
    if truth_rank(left) >= truth_rank(right) {
        left
    } else {
        right
    }
}

fn truth_rank(value: FactTruth) -> u8 {
    match value {
        FactTruth::StaticCandidate => 0,
        FactTruth::Structural => 1,
        FactTruth::Confirmed => 2,
    }
}

fn truth_name(value: FactTruth) -> &'static str {
    match value {
        FactTruth::Confirmed => "confirmed",
        FactTruth::Structural => "structural",
        FactTruth::StaticCandidate => "static_candidate",
    }
}

fn stronger_resolution(left: ResolutionMethod, right: ResolutionMethod) -> ResolutionMethod {
    if resolution_rank(left) >= resolution_rank(right) {
        left
    } else {
        right
    }
}

fn resolution_rank(value: ResolutionMethod) -> u8 {
    match value {
        ResolutionMethod::Manifest => 0,
        ResolutionMethod::SyntaxExact => 1,
        ResolutionMethod::ProjectModel => 2,
        ResolutionMethod::FrameworkAdapter => 3,
        ResolutionMethod::DatabaseReconciliation => 4,
        ResolutionMethod::Provider => 5,
        ResolutionMethod::Compiler => 6,
    }
}

fn merge_dispatch(left: DispatchKind, right: DispatchKind) -> DispatchKind {
    if left == right {
        left
    } else if left == DispatchKind::Unknown {
        right
    } else if right == DispatchKind::Unknown {
        left
    } else {
        DispatchKind::Unknown
    }
}

fn merge_optional_exact<T: Eq + std::fmt::Display>(
    left: Option<T>,
    right: Option<T>,
    label: &str,
) -> Result<Option<T>, String> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => {
            Err(format!("conflicting {label}: {left} vs {right}"))
        }
        (left, right) => Ok(left.or(right)),
    }
}

fn merge_optional_text(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}

fn insert_exact(
    connection: &Connection,
    table: &str,
    key_column: &str,
    key: &str,
    payload: &str,
) -> Result<(), String> {
    let existing = select_payload(connection, table, key_column, key)?;
    if let Some(existing) = existing {
        if existing != payload {
            return Err(format!("conflicting duplicate row in {table}: {key}"));
        }
        return Ok(());
    }
    connection
        .prepare_cached(&format!(
            "INSERT INTO {table}({key_column}, payload_json) VALUES (?1, ?2)"
        ))
        .and_then(|mut statement| statement.execute(params![key, payload]))
        .map_err(|error| format!("cannot write {table} row: {error}"))?;
    Ok(())
}

fn select_payload(
    connection: &Connection,
    table: &str,
    key_column: &str,
    key: &str,
) -> Result<Option<String>, String> {
    connection
        .prepare_cached(&format!(
            "SELECT payload_json FROM {table} WHERE {key_column}=?1"
        ))
        .and_then(|mut statement| statement.query_row([key], |row| row.get(0)))
        .optional()
        .map_err(|error| format!("cannot read {table} row: {error}"))
}

fn semantic_digest(connection: &Connection) -> Result<Sha256Digest, String> {
    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_DIGEST_DOMAIN);
    for table in FINAL_TABLES {
        hash_component(&mut hasher, table.as_bytes());
        let order_column = semantic_order_column(table)?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT payload_json FROM {table} ORDER BY {order_column}"
            ))
            .map_err(|error| format!("cannot prepare semantic digest for {table}: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("cannot read semantic digest rows for {table}: {error}"))?;
        for row in rows {
            let payload =
                row.map_err(|error| format!("cannot decode semantic row for {table}: {error}"))?;
            let semantic_payload = semantic_payload(table, &payload)?;
            hash_component(&mut hasher, semantic_payload.as_bytes());
        }
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("cannot encode canonical semantic digest: {error}"))
}

fn semantic_order_column(table: &str) -> Result<&'static str, String> {
    match table {
        "analysis_unit_receipts" => Ok("unit_id"),
        "evidence" | "nodes" | "edges" => Ok("id"),
        "file_coverage" | "capability_receipts" | "gaps" | "issues" => Ok("record_key"),
        "source_scope_coverage" => Ok("path"),
        _ => Err(format!("unknown semantic digest table: {table}")),
    }
}

fn semantic_payload(table: &str, payload: &str) -> Result<String, String> {
    match table {
        "evidence" => {
            let mut evidence: FactEvidence = from_json(payload)?;
            evidence.summary = None;
            to_json(&evidence)
        }
        "gaps" => semantic_gap_payload(&from_json(payload)?),
        "issues" => semantic_issue_payload(&from_json(payload)?),
        _ => Ok(payload.to_string()),
    }
}

fn semantic_gap_payload(gap: &AnalysisGap) -> Result<String, String> {
    to_json(&SemanticGap {
        code: &gap.code,
        scope: &gap.scope,
        capability: &gap.capability,
        evidence_ids: &gap.evidence_ids,
    })
}

fn semantic_issue_payload(issue: &AnalysisIssue) -> Result<String, String> {
    to_json(&SemanticIssue {
        code: &issue.code,
        stage: &issue.stage,
        scope: &issue.scope,
        retryable: issue.retryable,
    })
}

fn count_query(connection: &Connection, sql: &str) -> Result<u64, String> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(|error| format!("cannot execute canonical count query: {error}"))
}

fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("cannot serialize canonical row: {error}"))
}

fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, String> {
    serde_json::from_str(value).map_err(|error| format!("cannot decode canonical row: {error}"))
}

fn hash_component(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn sha256_file(path: &Path) -> Result<Sha256Digest, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open {} for SHA-256: {error}", path.display()))?;
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("cannot encode SHA-256 for {}: {error}", path.display()))
}

fn sync_file(path: &Path) -> Result<(), String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("cannot sync {}: {error}", path.display()))
}

fn publish_immutable_payload(
    temporary_path: &Path,
    final_path: &Path,
    expected_digest: Sha256Digest,
) -> Result<(), String> {
    if final_path.exists() {
        let existing = sha256_file(final_path)?;
        if existing != expected_digest {
            return Err(format!(
                "immutable canonical bundle collision at {}",
                final_path.display()
            ));
        }
        fs::remove_file(temporary_path).map_err(|error| {
            format!(
                "cannot discard duplicate canonical staging payload {}: {error}",
                temporary_path.display()
            )
        })?;
        return Ok(());
    }
    fs::rename(temporary_path, final_path).map_err(|error| {
        format!(
            "cannot publish canonical bundle {}: {error}",
            final_path.display()
        )
    })?;
    sync_file(final_path)
}

fn publish_manifest(
    path: &Path,
    manifest: &FactBundleManifest,
) -> Result<FactBundleManifest, String> {
    if path.exists() {
        let existing: FactBundleManifest = serde_json::from_reader(
            File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("cannot decode {}: {error}", path.display()))?;
        existing
            .validate()
            .map_err(|error| format!("invalid existing bundle manifest: {error}"))?;
        if existing.snapshot_id != manifest.snapshot_id
            || existing.semantic_digest != manifest.semantic_digest
            || existing.bundle_digest != manifest.bundle_digest
        {
            return Err(format!(
                "immutable canonical manifest collision at {}",
                path.display()
            ));
        }
        return Ok(existing);
    }
    let temporary = path.with_extension(format!(
        "manifest.json.{}.tmp",
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let file = File::create(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, manifest)
        .map_err(|error| format!("cannot encode bundle manifest: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|error| format!("cannot flush {}: {error}", temporary.display()))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("cannot sync {}: {error}", temporary.display()))?;
    drop(writer);
    fs::rename(&temporary, path)
        .map_err(|error| format!("cannot publish {}: {error}", path.display()))?;
    Ok(manifest.clone())
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
