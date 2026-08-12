//! 문자열·멀티라인 SQL에서 테이블 접근 사실을 추출한다.

use crate::facts::{AccessMode, Evidence, FactBundle, ResourceAccess, ResourceKind, SourceSpan};
use crate::languages::common::metadata::stable_id;
use crate::languages::common::unit_index::UnitSpanIndex;
use crate::model::FileEntry;
use regex::Regex;
use std::collections::HashSet;

pub(super) fn extract_sql_resources(
    source: &str,
    file: &FileEntry,
    unit_index: &UnitSpanIndex,
    bundle: &mut FactBundle,
    sql_pattern: Option<&Regex>,
) {
    let Some(sql_pattern) = sql_pattern else {
        return;
    };
    let lines = source.lines().collect::<Vec<_>>();
    let mut seen = HashSet::new();
    for start in 0..lines.len() {
        if !looks_like_sql_start(lines[start]) {
            continue;
        }
        let limit = (start + 16).min(lines.len());
        let mut end = start + 1;
        while end < limit {
            let line = lines[end];
            end += 1;
            if line.contains(';') || line.contains('`') {
                break;
            }
        }
        let statement = lines[start..end].join(" ");
        if !looks_like_sql_statement(&statement) {
            continue;
        }
        let line_number = start as u32 + 1;
        let unit_id = unit_index.unit_for_line(line_number);
        let span = SourceSpan::new(
            file.file_id.clone(),
            file.relative_path.clone(),
            line_number,
            1,
            end as u32,
            lines[end.saturating_sub(1)].chars().count() as u32 + 1,
        );
        for captures in sql_pattern.captures_iter(&statement) {
            let Some(name) = captures.get(1).map(|value| value.as_str().to_string()) else {
                continue;
            };
            let Some(name) = sql_table_name(
                &statement,
                captures.get(0).map(|value| value.end()).unwrap_or_default(),
                &name,
            ) else {
                continue;
            };
            let mode = captures
                .get(0)
                .map(|match_value| {
                    sql_table_access_mode(&statement, match_value.start(), match_value.as_str())
                })
                .unwrap_or_else(|| sql_access_mode(&statement));
            let key = format!("{}:{}:{:?}:{}", unit_id, line_number, mode, name);
            if !seen.insert(key) {
                continue;
            }
            let id = stable_id(
                "resource",
                &format!("{}:{}:{:?}:{}", file.file_id, line_number, mode, name),
            );
            if bundle.resources.iter().any(|resource| resource.id == id) {
                continue;
            }
            bundle.resources.push(ResourceAccess {
                id,
                unit_id: unit_id.clone(),
                kind: ResourceKind::Table,
                name: name.clone(),
                mode: mode.clone(),
                evidence: vec![Evidence::new("resource", name, span.clone())],
            });
        }
    }
}

fn sql_table_name(statement: &str, match_end: usize, candidate: &str) -> Option<String> {
    let candidate = candidate.trim_matches(['`', '"', '\'']);
    if !SQL_NON_TABLE_WORDS.contains(&candidate.to_ascii_lowercase().as_str()) {
        return (!candidate.is_empty()).then(|| candidate.to_string());
    }

    statement
        .get(match_end..)?
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .find_map(|word| {
            let lower = word.to_ascii_lowercase();
            (!SQL_NON_TABLE_WORDS.contains(&lower.as_str())).then(|| word.to_string())
        })
}

const SQL_NON_TABLE_WORDS: &[&str] = &[
    "if", "not", "exists", "set", "skip", "locked", "only", "where", "on", "using",
];

fn looks_like_sql_start(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "merge ",
        "with ",
        "create table",
        "alter table",
        "drop table",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
        || lower.contains(" select ")
        || lower.contains("select ")
        || lower.contains(" insert ")
        || lower.contains("insert into ")
        || lower.contains(" update ")
        || lower.contains("update ")
        || lower.contains(" delete ")
        || lower.contains("delete from ")
        || lower.contains("create table ")
        || lower.contains("alter table ")
        || lower.contains("drop table ")
}

fn sql_access_mode(statement: &str) -> AccessMode {
    let lower = statement.to_ascii_lowercase();
    let has_read = lower.contains("select ") || lower.trim_start().starts_with("select");
    let has_write = [
        "insert ",
        "update ",
        "delete ",
        "merge ",
        "create table",
        "alter table",
        "drop table",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword));
    match (has_read, has_write) {
        (true, true) => AccessMode::ReadWrite,
        (true, false) => AccessMode::Read,
        (false, true) => AccessMode::Write,
        (false, false) => AccessMode::Unknown,
    }
}

fn sql_table_access_mode(statement: &str, match_start: usize, match_text: &str) -> AccessMode {
    let keyword = match_text
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .find(|token| {
            matches!(
                token.as_str(),
                "from" | "into" | "update" | "join" | "table"
            )
        })
        .unwrap_or_default();
    let before_match = statement[..match_start].to_ascii_lowercase();
    let nearest_statement_keyword = ["select", "delete", "insert", "update", "merge"]
        .iter()
        .filter_map(|keyword| {
            before_match
                .rfind(keyword)
                .map(|position| (position, *keyword))
        })
        .max_by_key(|(position, _)| *position)
        .map(|(_, keyword)| keyword);
    match keyword.as_str() {
        "from" if nearest_statement_keyword == Some("delete") => AccessMode::Write,
        "from" | "join" => AccessMode::Read,
        "into" | "update" | "table" => {
            if keyword == "into" && nearest_statement_keyword == Some("merge") {
                AccessMode::ReadWrite
            } else {
                AccessMode::Write
            }
        }
        _ => sql_access_mode(statement),
    }
}

fn looks_like_sql_statement(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = lower.trim_start();
    if trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        return false;
    }

    (lower.contains("select ") && lower.contains(" from "))
        || lower.contains("insert into ")
        || (lower.contains("update ") && lower.contains(" set "))
        || lower.contains("delete from ")
        || lower.contains("merge into ")
        || (lower.contains("join ") && lower.contains(" on "))
        || lower.contains("create table ")
        || lower.contains("alter table ")
        || lower.contains("drop table ")
        || (trimmed.starts_with("with ") && lower.contains(" select ") && lower.contains(" from "))
}
