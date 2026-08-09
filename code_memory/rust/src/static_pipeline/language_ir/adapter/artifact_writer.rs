use codebase_fact_model::{
    identity::Sha256Digest,
    language_ir::{LanguageIrRecord, LanguageIrStreamValidator},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

pub(super) trait LanguageIrSink {
    fn push(&mut self, record: LanguageIrRecord) -> Result<(), String>;
}

pub(super) struct ValidatingDigestSink<'a> {
    validator: LanguageIrStreamValidator,
    hasher: Sha256,
    semantic_hasher: Sha256,
    record_count: u64,
    record_buffer: Vec<u8>,
    artifact: &'a mut AtomicLanguageIrArtifactWriter,
}

impl LanguageIrSink for ValidatingDigestSink<'_> {
    fn push(&mut self, record: LanguageIrRecord) -> Result<(), String> {
        self.validator
            .push(&record)
            .map_err(|error| format!("invalid emitted Language IR record: {error}"))?;
        self.record_buffer.clear();
        serde_json::to_writer(&mut self.record_buffer, &record)
            .map_err(|error| format!("cannot serialize emitted Language IR record: {error}"))?;
        self.artifact.write_record(&self.record_buffer)?;
        self.hasher
            .update((self.record_buffer.len() as u64).to_be_bytes());
        self.hasher.update(&self.record_buffer);
        if matches!(
            record,
            LanguageIrRecord::Evidence(_)
                | LanguageIrRecord::Definition(_)
                | LanguageIrRecord::Relation(_)
        ) {
            self.semantic_hasher
                .update((self.record_buffer.len() as u64).to_be_bytes());
            self.semantic_hasher.update(&self.record_buffer);
        }
        self.record_count += 1;
        Ok(())
    }
}

impl<'a> ValidatingDigestSink<'a> {
    pub(super) fn new(artifact: &'a mut AtomicLanguageIrArtifactWriter) -> Self {
        Self {
            validator: LanguageIrStreamValidator::default(),
            hasher: Sha256::new(),
            semantic_hasher: Sha256::new(),
            record_count: 0,
            record_buffer: Vec::with_capacity(4 * 1024),
            artifact,
        }
    }

    pub(super) fn finish(self) -> Result<(Sha256Digest, Sha256Digest, u64), String> {
        self.validator
            .finish()
            .map_err(|error| format!("incomplete Language IR stream: {error}"))?;
        let digest = Sha256Digest::parse(&format!("{:x}", self.hasher.finalize()))
            .map_err(|error| format!("cannot encode Language IR stream digest: {error}"))?;
        let semantic_digest =
            Sha256Digest::parse(&format!("{:x}", self.semantic_hasher.finalize())).map_err(
                |error| format!("cannot encode Language IR semantic payload digest: {error}"),
            )?;
        Ok((digest, semantic_digest, self.record_count))
    }
}

pub(super) struct AtomicLanguageIrArtifactWriter {
    writer: Option<BufWriter<File>>,
    temporary_path: PathBuf,
    final_path: PathBuf,
    hasher: Sha256,
    record_count: u64,
    byte_count: u64,
    committed: bool,
}

pub(super) struct LanguageIrArtifactFile {
    pub(super) path: PathBuf,
    pub(super) content_digest: Sha256Digest,
    pub(super) record_count: u64,
    pub(super) byte_count: u64,
}

impl AtomicLanguageIrArtifactWriter {
    pub(super) fn create(
        project_root: &Path,
        artifact_root: &Path,
        snapshot_id: &str,
    ) -> Result<Self, String> {
        fs::create_dir_all(artifact_root).map_err(|error| {
            format!(
                "cannot create Language IR artifact root {}: {error}",
                artifact_root.display()
            )
        })?;
        let canonical_artifact_root = artifact_root.canonicalize().map_err(|error| {
            format!(
                "cannot resolve Language IR artifact root {}: {error}",
                artifact_root.display()
            )
        })?;
        let canonical_project_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        if canonical_artifact_root.starts_with(&canonical_project_root) {
            return Err(
                "Language IR artifact root must be outside the selected repository".to_string(),
            );
        }
        let final_path = canonical_artifact_root.join(format!("{snapshot_id}.jsonl"));
        let temporary_path = canonical_artifact_root.join(format!(".{snapshot_id}.jsonl.tmp"));
        if final_path.exists() || temporary_path.exists() {
            return Err(format!(
                "Language IR artifact target already exists: {}",
                final_path.display()
            ));
        }
        let file = File::create(&temporary_path).map_err(|error| {
            format!(
                "cannot create Language IR artifact {}: {error}",
                temporary_path.display()
            )
        })?;
        Ok(Self {
            writer: Some(BufWriter::with_capacity(1024 * 1024, file)),
            temporary_path,
            final_path,
            hasher: Sha256::new(),
            record_count: 0,
            byte_count: 0,
            committed: false,
        })
    }

    fn write_record(&mut self, bytes: &[u8]) -> Result<(), String> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| "Language IR artifact writer is already closed".to_string())?;
        writer.write_all(bytes).map_err(|error| {
            format!(
                "cannot write Language IR artifact {}: {error}",
                self.temporary_path.display()
            )
        })?;
        writer.write_all(b"\n").map_err(|error| {
            format!(
                "cannot write Language IR artifact {}: {error}",
                self.temporary_path.display()
            )
        })?;
        self.hasher.update(bytes);
        self.hasher.update(b"\n");
        self.record_count += 1;
        self.byte_count += bytes.len() as u64 + 1;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<LanguageIrArtifactFile, String> {
        let mut writer = self
            .writer
            .take()
            .ok_or_else(|| "Language IR artifact writer is already closed".to_string())?;
        writer.flush().map_err(|error| {
            format!(
                "cannot flush Language IR artifact {}: {error}",
                self.temporary_path.display()
            )
        })?;
        writer.get_ref().sync_all().map_err(|error| {
            format!(
                "cannot sync Language IR artifact {}: {error}",
                self.temporary_path.display()
            )
        })?;
        drop(writer);
        fs::rename(&self.temporary_path, &self.final_path).map_err(|error| {
            format!(
                "cannot publish Language IR artifact {}: {error}",
                self.final_path.display()
            )
        })?;
        self.committed = true;
        let digest = Sha256Digest::parse(&format!("{:x}", self.hasher.clone().finalize()))
            .map_err(|error| format!("cannot encode Language IR artifact digest: {error}"))?;
        Ok(LanguageIrArtifactFile {
            path: self.final_path.clone(),
            content_digest: digest,
            record_count: self.record_count,
            byte_count: self.byte_count,
        })
    }
}

impl Drop for AtomicLanguageIrArtifactWriter {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.temporary_path);
        }
    }
}
