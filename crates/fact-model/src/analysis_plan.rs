//! Deterministic mapping from census files to semantic analysis units.

use crate::analysis::{AnalysisUnit, ProgrammingLanguage};
use crate::coverage::{AnalysisGap, AnalysisScope};
use crate::identity::{AnalysisUnitId, Sha256Digest, WorkspaceId};
use crate::source::RepositoryPath;
use crate::source_manifest::{SourceEntryState, SourceManifest};
use crate::validation::{ensure_unique, ContractError, ContractErrorCode, Validate};
use crate::ContractSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const ANALYSIS_PLAN_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.analysis-plan.digest.v1\0";
const CONFIG_SET_DIGEST_DOMAIN: &[u8] = b"codebase-workspace.config-set.digest.v1\0";

/// One recognized language view of a file and the contexts that own it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileAnalysisAssignment {
    pub path: RepositoryPath,
    pub language: ProgrammingLanguage,
    pub unit_ids: Vec<AnalysisUnitId>,
}

impl Validate for FileAnalysisAssignment {
    fn validate(&self) -> Result<(), ContractError> {
        if self.path.is_root() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidRepositoryPath,
                "path",
                "an analysis assignment must point to a file",
            ));
        }
        if self.unit_ids.is_empty() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "unitIds",
                "every eligible language file requires at least one analysis unit",
            ));
        }
        ensure_unique(self.unit_ids.iter(), "unitIds")?;
        if !self.unit_ids.windows(2).all(|pair| pair[0] <= pair[1]) {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "unitIds",
                "analysis unit IDs must use deterministic sorted order",
            ));
        }
        Ok(())
    }
}

/// Complete planning receipt consumed by language-provider adapters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisPlan {
    pub schema: ContractSchema,
    pub workspace_id: WorkspaceId,
    pub source_manifest_digest: Sha256Digest,
    pub config_digest: Sha256Digest,
    pub units: Vec<AnalysisUnit>,
    pub assignments: Vec<FileAnalysisAssignment>,
    pub gaps: Vec<AnalysisGap>,
    pub plan_digest: Sha256Digest,
}

impl AnalysisPlan {
    /// Canonicalizes ordering, calculates config-set identity, and seals the plan.
    pub fn new(
        workspace_id: WorkspaceId,
        source_manifest_digest: Sha256Digest,
        mut units: Vec<AnalysisUnit>,
        mut assignments: Vec<FileAnalysisAssignment>,
        mut gaps: Vec<AnalysisGap>,
    ) -> Result<Self, ContractError> {
        units.sort_by(|left, right| left.id.cmp(&right.id));
        for assignment in &mut assignments {
            assignment.unit_ids.sort();
        }
        assignments
            .sort_by(|left, right| (&left.path, left.language).cmp(&(&right.path, right.language)));
        for gap in &mut gaps {
            gap.evidence_ids.sort();
        }
        gaps.sort_by_key(gap_sort_key);
        let config_digest = config_digest(&units);
        let mut plan = Self {
            schema: ContractSchema::AnalysisPlanV1,
            workspace_id,
            source_manifest_digest,
            config_digest,
            units,
            assignments,
            gaps,
            plan_digest: Sha256Digest::of_bytes(b"uninitialized analysis plan"),
        };
        plan.plan_digest = plan.expected_digest();
        plan.validate()?;
        Ok(plan)
    }

    /// Validates plan integrity plus complete ownership of census candidates.
    pub fn validate_against(&self, manifest: &SourceManifest) -> Result<(), ContractError> {
        self.validate()?;
        manifest.validate()?;
        if self.workspace_id != manifest.workspace_id
            || self.source_manifest_digest != manifest.manifest_digest
        {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "sourceManifestDigest",
                "analysis plan does not belong to the supplied source manifest",
            ));
        }
        let expected = manifest
            .files
            .iter()
            .filter(|file| file.state == SourceEntryState::Included)
            .flat_map(|file| {
                file.languages
                    .iter()
                    .copied()
                    .map(move |language| (file.path.clone(), language))
            })
            .collect::<BTreeSet<_>>();
        let actual = self
            .assignments
            .iter()
            .map(|assignment| (assignment.path.clone(), assignment.language))
            .collect::<BTreeSet<_>>();
        if expected != actual {
            return Err(ContractError::new(
                ContractErrorCode::InvalidReceipt,
                "assignments",
                "analysis plan must own every and only included language candidate",
            ));
        }
        Ok(())
    }

    /// Recomputes the plan digest from canonical semantic fields.
    pub fn expected_digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(ANALYSIS_PLAN_DIGEST_DOMAIN);
        hash_text(&mut hasher, self.schema.as_str());
        hash_text(&mut hasher, self.workspace_id.as_str());
        hash_text(&mut hasher, &self.source_manifest_digest.to_hex());
        hash_text(&mut hasher, &self.config_digest.to_hex());
        for unit in &self.units {
            hash_text(&mut hasher, "unit");
            hash_text(&mut hasher, unit.id.as_str());
            hash_text(&mut hasher, &unit.eligible_file_count.to_string());
        }
        for assignment in &self.assignments {
            hash_text(&mut hasher, "assignment");
            hash_text(&mut hasher, assignment.path.as_str());
            hash_text(&mut hasher, assignment.language.as_str());
            for unit_id in &assignment.unit_ids {
                hash_text(&mut hasher, unit_id.as_str());
            }
        }
        for gap in &self.gaps {
            hash_text(&mut hasher, "gap");
            hash_text(&mut hasher, gap.code.as_str());
            hash_scope(&mut hasher, &gap.scope);
            hash_text(
                &mut hasher,
                gap.capability.map(|value| value.as_str()).unwrap_or("-"),
            );
            for evidence_id in &gap.evidence_ids {
                hash_text(&mut hasher, evidence_id.as_str());
            }
        }
        Sha256Digest::of_bytes(&hasher.finalize())
    }
}

impl Validate for AnalysisPlan {
    fn validate(&self) -> Result<(), ContractError> {
        if self.schema != ContractSchema::AnalysisPlanV1 {
            return Err(ContractError::new(
                ContractErrorCode::InvalidSchema,
                "schema",
                "analysis plan requires the analysis-plan v1 schema",
            ));
        }
        ensure_unique(self.units.iter().map(|unit| &unit.id), "units.id")?;
        ensure_unique(
            self.assignments
                .iter()
                .map(|assignment| (&assignment.path, assignment.language)),
            "assignments",
        )?;
        ensure_unique(self.gaps.iter().map(gap_sort_key), "gaps")?;
        if !self.units.windows(2).all(|pair| pair[0].id <= pair[1].id)
            || !self
                .assignments
                .windows(2)
                .all(|pair| (&pair[0].path, pair[0].language) <= (&pair[1].path, pair[1].language))
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "analysisPlan",
                "units and assignments must use deterministic order",
            ));
        }
        if !self
            .gaps
            .windows(2)
            .all(|pair| gap_sort_key(&pair[0]) <= gap_sort_key(&pair[1]))
        {
            return Err(ContractError::new(
                ContractErrorCode::NonCanonicalValue,
                "gaps",
                "analysis gaps must use deterministic semantic order",
            ));
        }
        let units = self
            .units
            .iter()
            .map(|unit| (unit.id.clone(), unit))
            .collect::<BTreeMap<_, _>>();
        let mut assigned_counts = BTreeMap::<AnalysisUnitId, u64>::new();
        for (index, unit) in self.units.iter().enumerate() {
            unit.validate()
                .map_err(|error| error.under(&format!("units[{index}]")))?;
            if unit.workspace_id != self.workspace_id {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    format!("units[{index}].workspaceId"),
                    "analysis unit belongs to another workspace",
                ));
            }
            if unit.eligible_file_count == 0 {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    format!("units[{index}].eligibleFileCount"),
                    "an analysis plan may not contain an empty unit",
                ));
            }
        }
        for (index, assignment) in self.assignments.iter().enumerate() {
            assignment
                .validate()
                .map_err(|error| error.under(&format!("assignments[{index}]")))?;
            for unit_id in &assignment.unit_ids {
                let Some(unit) = units.get(unit_id) else {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidReceipt,
                        format!("assignments[{index}].unitIds"),
                        "assignment references an unknown analysis unit",
                    ));
                };
                if unit.language != assignment.language
                    || !path_is_within(&assignment.path, &unit.root)
                {
                    return Err(ContractError::new(
                        ContractErrorCode::InvalidReceipt,
                        format!("assignments[{index}]"),
                        "assignment language and path must belong to the referenced unit",
                    ));
                }
                *assigned_counts.entry(unit_id.clone()).or_default() += 1;
            }
        }
        for unit in &self.units {
            let actual = assigned_counts.get(&unit.id).copied().unwrap_or(0);
            if actual != unit.eligible_file_count {
                return Err(ContractError::new(
                    ContractErrorCode::InvalidReceipt,
                    "units.eligibleFileCount",
                    "analysis unit eligible-file count does not match assignments",
                ));
            }
        }
        for (index, gap) in self.gaps.iter().enumerate() {
            gap.validate()
                .map_err(|error| error.under(&format!("gaps[{index}]")))?;
        }
        if self.config_digest != config_digest(&self.units) {
            return Err(ContractError::new(
                ContractErrorCode::InvalidDigest,
                "configDigest",
                "config digest does not match analysis-unit contexts",
            ));
        }
        if self.plan_digest != self.expected_digest() {
            return Err(ContractError::new(
                ContractErrorCode::InvalidDigest,
                "planDigest",
                "analysis plan digest does not match canonical fields",
            ));
        }
        Ok(())
    }
}

fn config_digest(units: &[AnalysisUnit]) -> Sha256Digest {
    let mut context_rows = units
        .iter()
        .map(|unit| {
            (
                unit.language,
                unit.root.clone(),
                unit.context.id.clone(),
                unit.context.fingerprint,
            )
        })
        .collect::<Vec<_>>();
    context_rows.sort();
    let mut hasher = Sha256::new();
    hasher.update(CONFIG_SET_DIGEST_DOMAIN);
    for (language, root, context_id, fingerprint) in context_rows {
        hash_text(&mut hasher, language.as_str());
        hash_text(&mut hasher, root.as_str());
        hash_text(&mut hasher, context_id.as_str());
        hash_text(&mut hasher, &fingerprint.to_hex());
    }
    Sha256Digest::of_bytes(&hasher.finalize())
}

fn gap_sort_key(gap: &AnalysisGap) -> (String, String, String) {
    (
        gap.code.as_str().to_string(),
        scope_key(&gap.scope),
        gap.capability
            .map(|value| value.as_str().to_string())
            .unwrap_or_default(),
    )
}

fn scope_key(scope: &AnalysisScope) -> String {
    match scope {
        AnalysisScope::Workspace => "workspace".to_string(),
        AnalysisScope::AnalysisUnit { unit_id } => format!("unit:{}", unit_id.as_str()),
        AnalysisScope::File { unit_id, path } => format!(
            "file:{}:{}",
            unit_id.as_ref().map(|value| value.as_str()).unwrap_or("-"),
            path.as_str()
        ),
        AnalysisScope::RepositoryScope { path } => format!("scope:{}", path.as_str()),
        AnalysisScope::NativeSymbol { unit_id, symbol_id } => {
            format!("symbol:{}:{}", unit_id.as_str(), symbol_id.as_str())
        }
    }
}

fn hash_scope(hasher: &mut Sha256, scope: &AnalysisScope) {
    hash_text(hasher, &scope_key(scope));
}

fn path_is_within(path: &RepositoryPath, root: &RepositoryPath) -> bool {
    if root.is_root() {
        return true;
    }
    path.as_str() == root.as_str()
        || path
            .as_str()
            .strip_prefix(root.as_str())
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
