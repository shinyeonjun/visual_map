/// Normalizes raw SCIP source coordinates to the engine's canonical provider
/// boundary: zero-based UTF-8 byte columns and half-open ranges.
///
/// SCIP deliberately permits each document to declare UTF-8, UTF-16 or UTF-32
/// columns. Treating all SCIP columns as bytes works for ASCII but corrupts
/// evidence as soon as source contains Korean text, emoji or another multibyte
/// character. This adapter runs before range matching, relation reconciliation
/// and canonical evidence construction so every downstream SCIP consumer sees
/// one coordinate system.
fn normalize_scip_document_ranges(
    documents: &mut [scip::types::Document],
    fallback_language: &str,
    project_root: &Path,
) -> Result<(), String> {
    for document in documents {
        let document_path = normalize_scip_path(&document.relative_path, project_root);
        let encoding = scip_document_encoding(document, fallback_language).map_err(|error| {
            format!("invalid SCIP position encoding for {document_path}: {error}")
        })?;
        let source_path = project_root.join(document_path.split('/').collect::<PathBuf>());
        let source_bytes = fs::read(&source_path).map_err(|error| {
            format!(
                "cannot read SCIP coordinate source {}: {error}",
                source_path.display()
            )
        })?;
        let source_bytes = source_bytes
            .strip_prefix(&[0xef, 0xbb, 0xbf])
            .unwrap_or(&source_bytes);
        let source = std::str::from_utf8(source_bytes).map_err(|error| {
            format!(
                "SCIP coordinate source is not UTF-8 {}: {error}",
                source_path.display()
            )
        })?;
        let lines = source
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect::<Vec<_>>();

        for (occurrence_index, occurrence) in document.occurrences.iter_mut().enumerate() {
            if let Some(typed) = occurrence.typed_range.take() {
                occurrence.range = scip_typed_range(typed)?;
            }
            if let Some(typed) = occurrence.typed_enclosing_range.take() {
                occurrence.enclosing_range = scip_typed_enclosing_range(typed)?;
            }
            normalize_scip_range(&lines, &mut occurrence.range, encoding).map_err(|error| {
                format!(
                    "invalid SCIP occurrence range in {document_path} at index {occurrence_index}: {error}"
                )
            })?;
            normalize_scip_range(&lines, &mut occurrence.enclosing_range, encoding).map_err(
                |error| {
                    format!(
                        "invalid SCIP enclosing range in {document_path} at index {occurrence_index}: {error}"
                    )
                },
            )?;
        }

        // The typed ranges were folded into the legacy vectors above. The raw
        // document is now an internal canonical SCIP document, not a verbatim
        // provider payload, so its declaration must match its normalized data.
        document.position_encoding = protobuf::EnumOrUnknown::new(
            scip::types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart,
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScipColumnEncoding {
    Utf8,
    Utf16,
    Utf32,
}

fn scip_document_encoding(
    document: &scip::types::Document,
    fallback_language: &str,
) -> Result<ScipColumnEncoding, String> {
    use scip::types::PositionEncoding;
    match document.position_encoding.enum_value() {
        Ok(PositionEncoding::UTF8CodeUnitOffsetFromLineStart) => Ok(ScipColumnEncoding::Utf8),
        Ok(PositionEncoding::UTF16CodeUnitOffsetFromLineStart) => Ok(ScipColumnEncoding::Utf16),
        Ok(PositionEncoding::UTF32CodeUnitOffsetFromLineStart) => Ok(ScipColumnEncoding::Utf32),
        Ok(PositionEncoding::UnspecifiedPositionEncoding) => {
            let language = normalize_scip_language(&document.language, fallback_language);
            match language.as_str() {
                // Compatibility contract for the supported legacy indexers.
                // Their implementation string model determines the unit.
                "typescript" | "javascript" | "csharp" => Ok(ScipColumnEncoding::Utf16),
                "c" | "cpp" => Ok(ScipColumnEncoding::Utf8),
                _ => Err(format!(
                    "document omitted position_encoding for unsupported SCIP language '{language}'"
                )),
            }
        }
        Err(value) => Err(format!("unknown SCIP position_encoding value {value}")),
    }
}

fn scip_typed_range(range: scip::types::occurrence::Typed_range) -> Result<Vec<i32>, String> {
    use scip::types::occurrence::Typed_range;
    match range {
        Typed_range::SingleLineRange(range) => {
            Ok(vec![range.line, range.start_character, range.end_character])
        }
        Typed_range::MultiLineRange(range) => Ok(vec![
            range.start_line,
            range.start_character,
            range.end_line,
            range.end_character,
        ]),
        _ => Err("SCIP typed range uses an unknown variant".to_string()),
    }
}

fn scip_typed_enclosing_range(
    range: scip::types::occurrence::Typed_enclosing_range,
) -> Result<Vec<i32>, String> {
    use scip::types::occurrence::Typed_enclosing_range;
    match range {
        Typed_enclosing_range::SingleLineEnclosingRange(range) => {
            Ok(vec![range.line, range.start_character, range.end_character])
        }
        Typed_enclosing_range::MultiLineEnclosingRange(range) => Ok(vec![
            range.start_line,
            range.start_character,
            range.end_line,
            range.end_character,
        ]),
        _ => Err("SCIP typed enclosing range uses an unknown variant".to_string()),
    }
}

fn normalize_scip_range(
    lines: &[&str],
    range: &mut Vec<i32>,
    encoding: ScipColumnEncoding,
) -> Result<(), String> {
    if range.is_empty() {
        return Ok(());
    }
    let (start_line, start_column, end_line, end_column) = match range.as_slice() {
        [line, start, end] => (*line, *start, *line, *end),
        [start_line, start_column, end_line, end_column] => {
            (*start_line, *start_column, *end_line, *end_column)
        }
        _ => return Err("range must contain exactly three or four coordinates".to_string()),
    };
    if [start_line, start_column, end_line, end_column]
        .iter()
        .any(|coordinate| *coordinate < 0)
    {
        return Err("range contains a negative coordinate".to_string());
    }
    if (end_line, end_column) < (start_line, start_column) {
        return Err("range end precedes its start".to_string());
    }
    let start_text = lines
        .get(start_line as usize)
        .ok_or_else(|| format!("start line {start_line} is outside the source file"))?;
    let end_text = lines
        .get(end_line as usize)
        .ok_or_else(|| format!("end line {end_line} is outside the source file"))?;
    let start = scip_column_to_byte(start_text, start_column as usize, encoding)?;
    let end = scip_column_to_byte(end_text, end_column as usize, encoding)?;
    let start = i32::try_from(start).map_err(|_| "start byte column exceeds i32".to_string())?;
    let end = i32::try_from(end).map_err(|_| "end byte column exceeds i32".to_string())?;
    *range = if start_line == end_line {
        vec![start_line, start, end]
    } else {
        vec![start_line, start, end_line, end]
    };
    Ok(())
}

fn scip_column_to_byte(
    text: &str,
    requested: usize,
    encoding: ScipColumnEncoding,
) -> Result<usize, String> {
    match encoding {
        ScipColumnEncoding::Utf8 => {
            if requested <= text.len() && text.is_char_boundary(requested) {
                Ok(requested)
            } else {
                Err(format!(
                    "UTF-8 column {requested} is outside a character boundary"
                ))
            }
        }
        ScipColumnEncoding::Utf16 => {
            let mut column = 0usize;
            for (byte, character) in text.char_indices() {
                if column == requested {
                    return Ok(byte);
                }
                let next = column + character.len_utf16();
                if requested < next {
                    return Err(format!("UTF-16 column {requested} splits a surrogate pair"));
                }
                column = next;
            }
            if column == requested {
                Ok(text.len())
            } else {
                Err(format!(
                    "UTF-16 column {requested} exceeds line length {column}"
                ))
            }
        }
        ScipColumnEncoding::Utf32 => {
            let mut column = 0usize;
            for (byte, _) in text.char_indices() {
                if column == requested {
                    return Ok(byte);
                }
                column += 1;
            }
            if column == requested {
                Ok(text.len())
            } else {
                Err(format!(
                    "UTF-32 column {requested} exceeds line length {column}"
                ))
            }
        }
    }
}

#[cfg(test)]
mod scip_coordinate_tests {
    use super::*;
    use crate::{ProviderKind, LANGUAGES};

    #[test]
    fn javascript_utf16_korean_columns_become_utf8_bytes() {
        let line = r#"          <button className="caps-icon-button" onClick={onRefresh} title="새로고침" type="button">"#;
        let mut range = vec![0, 80, 84];
        normalize_scip_range(&[line], &mut range, ScipColumnEncoding::Utf16).unwrap();
        assert_eq!(range, vec![0, 88, 92]);
        assert_eq!(&line[88..92], "type");
    }

    #[test]
    fn utf16_handles_bmp_and_supplementary_characters() {
        let lines = ["protected class Entityß", "class Rocket🚀"];
        let mut bmp = vec![0, 16, 23];
        normalize_scip_range(&lines, &mut bmp, ScipColumnEncoding::Utf16).unwrap();
        assert_eq!(bmp, vec![0, 16, 24]);

        let mut supplementary = vec![1, 6, 14];
        normalize_scip_range(&lines, &mut supplementary, ScipColumnEncoding::Utf16).unwrap();
        assert_eq!(supplementary, vec![1, 6, 16]);
    }

    #[test]
    fn utf32_counts_unicode_scalars() {
        let line = "a🚀한z";
        let mut range = vec![0, 1, 3];
        normalize_scip_range(&[line], &mut range, ScipColumnEncoding::Utf32).unwrap();
        assert_eq!(range, vec![0, 1, 8]);
        assert_eq!(&line[1..8], "🚀한");
    }

    #[test]
    fn utf8_rejects_a_column_inside_a_character() {
        let mut range = vec![0, 0, 2];
        let error =
            normalize_scip_range(&["한"], &mut range, ScipColumnEncoding::Utf8).unwrap_err();
        assert!(error.contains("character boundary"));
    }

    #[test]
    fn typed_range_takes_precedence_over_deprecated_range() {
        let root = temporary_scip_coordinate_root("typed");
        fs::write(root.join("typed.ts"), "const 이름 = 1;\r\n").unwrap();
        let mut document = scip::types::Document::new();
        document.language = "typescript".to_string();
        document.relative_path = "typed.ts".to_string();
        document.position_encoding = protobuf::EnumOrUnknown::new(
            scip::types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart,
        );
        let mut occurrence = scip::types::Occurrence::new();
        occurrence.range = vec![0, 99, 100];
        occurrence.typed_range = Some(scip::types::occurrence::Typed_range::SingleLineRange(
            scip::types::SingleLineRange {
                line: 0,
                start_character: 6,
                end_character: 8,
                ..Default::default()
            },
        ));
        document.occurrences.push(occurrence);

        normalize_scip_document_ranges(std::slice::from_mut(&mut document), "typescript", &root)
            .unwrap();
        assert_eq!(document.occurrences[0].range, vec![0, 6, 12]);
        assert!(document.occurrences[0].typed_range.is_none());
        assert_eq!(
            document.position_encoding.enum_value().unwrap(),
            scip::types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_scip_normalizes_before_document_output_and_range_matching() {
        let root = temporary_scip_coordinate_root("read-scip");
        fs::write(root.join("main.ts"), "const 이름 = 1;\n").unwrap();
        let mut document = scip::types::Document::new();
        document.language = "typescript".to_string();
        document.relative_path = "main.ts".to_string();
        document.position_encoding = protobuf::EnumOrUnknown::new(
            scip::types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart,
        );
        let mut occurrence = scip::types::Occurrence::new();
        occurrence.symbol = "scip-typescript npm fixture 1.0.0 main.ts/name.".to_string();
        occurrence.symbol_roles = SymbolRole::Definition as i32;
        occurrence.range = vec![0, 6, 8];
        document.occurrences.push(occurrence);
        let mut index = scip::types::Index::new();
        index.documents.push(document);
        let scip_path = root.join("index.scip");
        fs::write(
            &scip_path,
            protobuf::Message::write_to_bytes(&index).unwrap(),
        )
        .unwrap();

        let allowed = HashSet::from(["main.ts".to_string()]);
        let (documents, _) = read_scip(
            &scip_path,
            "typescript",
            ProviderProtocol::Scip,
            &root,
            &allowed,
            None,
        )
        .unwrap();
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].occurrences[0].range, vec![0, 6, 12]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bom_and_crlf_are_normalized_like_canonical_source_evidence() {
        let root = temporary_scip_coordinate_root("bom-crlf");
        fs::write(
            root.join("route.ts"),
            b"\xef\xbb\xbfconst x = 1;\r\nroute('\xed\x95\x9c');\r\n",
        )
        .unwrap();
        let mut document = scip::types::Document::new();
        document.language = "typescript".to_string();
        document.relative_path = "route.ts".to_string();
        document.position_encoding = protobuf::EnumOrUnknown::new(
            scip::types::PositionEncoding::UTF16CodeUnitOffsetFromLineStart,
        );
        let mut occurrence = scip::types::Occurrence::new();
        occurrence.range = vec![1, 7, 8];
        document.occurrences.push(occurrence);

        normalize_scip_document_ranges(std::slice::from_mut(&mut document), "typescript", &root)
            .unwrap();
        assert_eq!(document.occurrences[0].range, vec![1, 7, 10]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unspecified_legacy_provider_encodings_are_language_scoped() {
        for (language, expected) in [
            ("typescript", ScipColumnEncoding::Utf16),
            ("javascript", ScipColumnEncoding::Utf16),
            ("csharp", ScipColumnEncoding::Utf16),
            ("c", ScipColumnEncoding::Utf8),
            ("cpp", ScipColumnEncoding::Utf8),
        ] {
            let mut document = scip::types::Document::new();
            document.language = language.to_string();
            assert_eq!(
                scip_document_encoding(&document, language).unwrap(),
                expected
            );
        }
        let mut unsupported = scip::types::Document::new();
        unsupported.language = "provider-private".to_string();
        assert!(scip_document_encoding(&unsupported, "provider-private").is_err());
    }

    #[test]
    fn every_supported_language_has_an_explicit_coordinate_boundary() {
        assert_eq!(LANGUAGES.len(), 10);
        for language in LANGUAGES {
            match language.provider {
                ProviderKind::Scip => {
                    let mut document = scip::types::Document::new();
                    document.language = language.id.to_string();
                    scip_document_encoding(&document, language.id).unwrap_or_else(|error| {
                        panic!(
                            "{} omitted its SCIP coordinate contract: {error}",
                            language.id
                        )
                    });
                }
                ProviderKind::Lsp => assert!(matches!(
                    language.id,
                    "python" | "java" | "go" | "rust" | "dart"
                )),
            }
        }
    }

    #[test]
    fn unknown_position_encoding_is_not_guessed() {
        let mut document = scip::types::Document::new();
        document.position_encoding = protobuf::EnumOrUnknown::from_i32(99);
        let error = scip_document_encoding(&document, "typescript").unwrap_err();
        assert!(error.contains("unknown SCIP position_encoding value 99"));
    }

    fn temporary_scip_coordinate_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "code-memory-scip-coordinate-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
