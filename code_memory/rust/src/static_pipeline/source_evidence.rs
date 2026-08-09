//! Verified source loading and canonical UTF-8 coordinate conversion.
//!
//! Language providers and typed boundary adapters must use the same source
//! bytes that the Source Census sealed.  This module is the single authority
//! for checking those bytes and producing canonical half-open source spans.

use codebase_fact_model::analysis::ProviderProtocol;
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source::{RepositoryPath, SourcePosition, SourceSpan};
use codebase_fact_model::source_manifest::{SourceEncoding, SourceManifestFile};
use codebase_fact_model::validation::Validate;
use std::fs;
use std::path::Path;

/// A verified UTF-8 view of one census-owned repository file.
pub(crate) struct VerifiedSourceFile {
    path: RepositoryPath,
    digest: Sha256Digest,
    text: String,
    raw_prefix_bytes: u64,
    line_starts: Vec<usize>,
}

impl VerifiedSourceFile {
    pub(crate) fn load(
        project_root: &Path,
        manifest_file: &SourceManifestFile,
    ) -> Result<Self, String> {
        let expected_digest = manifest_file.content_digest.ok_or_else(|| {
            format!(
                "source manifest omitted digest for {}",
                manifest_file.path.as_str()
            )
        })?;
        let absolute = project_root.join(
            manifest_file
                .path
                .as_str()
                .split('/')
                .collect::<std::path::PathBuf>(),
        );
        let bytes = fs::read(&absolute).map_err(|error| {
            format!(
                "cannot read source evidence {}: {error}",
                absolute.display()
            )
        })?;
        if bytes.len() as u64 != manifest_file.byte_size {
            return Err(format!(
                "source size changed after census: {}",
                manifest_file.path.as_str()
            ));
        }
        let actual_digest = Sha256Digest::of_bytes(&bytes);
        if actual_digest != expected_digest {
            return Err(format!(
                "source digest changed after census: {}",
                manifest_file.path.as_str()
            ));
        }
        let (text_bytes, raw_prefix_bytes) = match manifest_file.encoding {
            SourceEncoding::Utf8 => (bytes.as_slice(), 0),
            SourceEncoding::Utf8Bom if bytes.starts_with(&[0xef, 0xbb, 0xbf]) => (&bytes[3..], 3),
            SourceEncoding::Utf8Bom => {
                return Err(format!(
                    "source manifest declared a missing UTF-8 BOM: {}",
                    manifest_file.path.as_str()
                ));
            }
            _ => {
                return Err(format!(
                    "source evidence is not readable UTF-8: {}",
                    manifest_file.path.as_str()
                ));
            }
        };
        let text = std::str::from_utf8(text_bytes)
            .map_err(|error| {
                format!(
                    "source encoding changed after census for {}: {error}",
                    manifest_file.path.as_str()
                )
            })?
            .to_string();
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Ok(Self {
            path: manifest_file.path.clone(),
            digest: expected_digest,
            text,
            raw_prefix_bytes,
            line_starts,
        })
    }

    /// Converts a provider range into the canonical UTF-8/byte coordinate
    /// system. LSP columns are UTF-16; SCIP/compiler columns are UTF-8 bytes.
    pub(crate) fn span(
        &self,
        range: &[i32],
        protocol: ProviderProtocol,
    ) -> Result<SourceSpan, String> {
        let (start_line, start_column, end_line, end_column) = match range {
            [line, start, end] => (*line, *start, *line, *end),
            [start_line, start_column, end_line, end_column, ..] => {
                (*start_line, *start_column, *end_line, *end_column)
            }
            _ => return Err("provider range must contain three or four coordinates".to_string()),
        };
        if [start_line, start_column, end_line, end_column]
            .iter()
            .any(|value| *value < 0)
        {
            return Err("provider range contains a negative coordinate".to_string());
        }
        let start = self.position(start_line as usize, start_column as usize, protocol)?;
        let end = self.position(end_line as usize, end_column as usize, protocol)?;
        self.validated_span(start, end)
    }

    /// Converts a tree-sitter range, whose columns are UTF-8 byte offsets,
    /// into the canonical source coordinate system.
    pub(crate) fn utf8_span(&self, range: &[i32]) -> Result<SourceSpan, String> {
        self.span(range, ProviderProtocol::CompilerApi)
    }

    /// Returns a span covering complete zero-based source lines. `end_line`
    /// is inclusive while the resulting source span remains half-open.
    pub(crate) fn whole_lines_span(
        &self,
        start_line: usize,
        end_line: usize,
    ) -> Result<SourceSpan, String> {
        if end_line < start_line {
            return Err("source line range end precedes start".to_string());
        }
        let start = self.utf8_position(start_line, 0)?;
        let (_, end_offset) = self.line_bounds(end_line)?;
        let end_start = *self
            .line_starts
            .get(end_line)
            .ok_or_else(|| format!("source line {end_line} is outside the source file"))?;
        let end = SourcePosition {
            line: end_line as u32,
            utf8_column: (end_offset - end_start) as u32,
            byte_offset: self.raw_prefix_bytes + end_offset as u64,
        };
        self.validated_span(start, end)
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    fn validated_span(
        &self,
        start: SourcePosition,
        end: SourcePosition,
    ) -> Result<SourceSpan, String> {
        let span = SourceSpan {
            path: self.path.clone(),
            content_digest: self.digest,
            start,
            end,
        };
        span.validate()
            .map_err(|error| format!("invalid canonical source span: {error}"))?;
        Ok(span)
    }

    fn position(
        &self,
        line: usize,
        provider_column: usize,
        protocol: ProviderProtocol,
    ) -> Result<SourcePosition, String> {
        let (start, end) = self.line_bounds(line)?;
        let line_text = &self.text[start..end];
        let byte_column = match protocol {
            ProviderProtocol::LanguageServerProtocol => {
                utf16_column_to_byte(line_text, provider_column)?
            }
            ProviderProtocol::Scip | ProviderProtocol::CompilerApi => {
                if provider_column > line_text.len() || !line_text.is_char_boundary(provider_column)
                {
                    return Err(format!(
                        "UTF-8 provider column {provider_column} is outside a character boundary"
                    ));
                }
                provider_column
            }
        };
        self.utf8_position(line, byte_column)
    }

    fn utf8_position(&self, line: usize, byte_column: usize) -> Result<SourcePosition, String> {
        let (start, end) = self.line_bounds(line)?;
        let line_text = &self.text[start..end];
        if byte_column > line_text.len() || !line_text.is_char_boundary(byte_column) {
            return Err(format!(
                "UTF-8 column {byte_column} is outside a character boundary"
            ));
        }
        Ok(SourcePosition {
            line: line as u32,
            utf8_column: byte_column as u32,
            byte_offset: self.raw_prefix_bytes + (start + byte_column) as u64,
        })
    }

    fn line_bounds(&self, line: usize) -> Result<(usize, usize), String> {
        let start = *self
            .line_starts
            .get(line)
            .ok_or_else(|| format!("source line {line} is outside the source file"))?;
        let mut end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        if end > start && self.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && self.text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        Ok((start, end))
    }
}

fn utf16_column_to_byte(text: &str, requested: usize) -> Result<usize, String> {
    let mut utf16 = 0usize;
    for (byte, character) in text.char_indices() {
        if utf16 == requested {
            return Ok(byte);
        }
        let next = utf16 + character.len_utf16();
        if requested < next {
            return Err(format!(
                "UTF-16 provider column {requested} splits a surrogate pair"
            ));
        }
        utf16 = next;
    }
    if utf16 == requested {
        Ok(text.len())
    } else {
        Err(format!(
            "UTF-16 provider column {requested} exceeds line length {utf16}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::VerifiedSourceFile;
    use crate::static_pipeline::source_census::SourceCensus;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn whole_line_span_uses_utf8_bytes_and_preserves_bom_offset() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("source-evidence-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("route.ts"),
            b"\xef\xbb\xbfconst x = 1;\nroute('/\xed\x95\x9c');\r\n",
        )
        .unwrap();
        let census = SourceCensus::scan(&root).unwrap();
        let file = census
            .manifest
            .files
            .iter()
            .find(|file| file.path.as_str() == "route.ts")
            .unwrap();
        let source = VerifiedSourceFile::load(&root, file).unwrap();
        let span = source.whole_lines_span(1, 1).unwrap();
        assert_eq!(span.start.line, 1);
        assert_eq!(span.start.utf8_column, 0);
        assert_eq!(span.start.byte_offset, 16);
        assert_eq!(span.end.utf8_column, "route('/한');".len() as u32);
        assert_eq!(span.end.byte_offset, 16 + "route('/한');".len() as u64);
        let _ = fs::remove_dir_all(root);
    }
}
