//! Streaming reader for the sealed Language IR JSONL artifact.
//!
//! Canonical linking and typed post-language adapters must consume the same
//! validated stream instead of rebuilding facts from another graph output.

use codebase_fact_model::language_ir::LanguageIrRecord;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub(crate) fn visit_language_ir_records(
    path: &Path,
    mut visitor: impl FnMut(LanguageIrRecord) -> Result<(), String>,
) -> Result<(), String> {
    let file = File::open(path).map_err(|error| {
        format!(
            "cannot open Language IR artifact {}: {error}",
            path.display()
        )
    })?;
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    let mut line = Vec::new();
    let mut ordinal = 0_u64;
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("cannot read Language IR artifact: {error}"))?;
        if read == 0 {
            break;
        }
        ordinal += 1;
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        if line.is_empty() {
            return Err(format!("Language IR record {ordinal} is empty"));
        }
        let record = serde_json::from_slice::<LanguageIrRecord>(&line)
            .map_err(|error| format!("cannot decode Language IR record {ordinal}: {error}"))?;
        visitor(record)?;
    }
    Ok(())
}
