//! Exact SQL-literal inventory shared by every supported source language.
//!
//! This layer deliberately does not try to evaluate string concatenation,
//! interpolation, variables, query builders, or ORM expressions.  It emits a
//! fact only when one syntax-tree string node contains a complete SQL data
//! access statement and a concrete table identifier.  Dynamic cases stay for
//! the later AI/candidate layer instead of being promoted as static truth.

use codebase_fact_model::identity::Sha256Digest;
use tree_sitter::Node;

use super::syntax::{node_text, utf8_range};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SqlTableAccessKind {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SqlTableAccess {
    pub(super) table: String,
    pub(super) kind: SqlTableAccessKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SqlQuerySite {
    pub(super) utf8_range: Vec<i32>,
    pub(super) operation: String,
    pub(super) digest: Sha256Digest,
    pub(super) tables: Vec<SqlTableAccess>,
}

impl SqlQuerySite {
    pub(super) fn display_name(&self) -> String {
        let primary = self
            .tables
            .first()
            .map(|access| access.table.as_str())
            .unwrap_or("query");
        format!("{} {primary}", self.operation)
    }
}

pub(super) fn inventory_sql_query_literals_from_root(
    language: &str,
    root: Node<'_>,
    source: &str,
) -> Vec<SqlQuerySite> {
    let mut sites = Vec::new();
    visit(language, root, source, &mut sites);
    sites.sort_by(|left, right| {
        (&left.utf8_range, &left.operation, left.digest).cmp(&(
            &right.utf8_range,
            &right.operation,
            right.digest,
        ))
    });
    sites
        .dedup_by(|left, right| left.utf8_range == right.utf8_range && left.digest == right.digest);
    sites
}

fn visit(language: &str, node: Node<'_>, source: &str, sites: &mut Vec<SqlQuerySite>) {
    if is_string_node(language, node.kind()) {
        if has_sql_context(node, source) {
            if let Some(value) = exact_literal_value(language, node_text(node, source)) {
                if let Some((operation, tables)) = classify_sql(&value) {
                    sites.push(SqlQuerySite {
                        utf8_range: utf8_range(node),
                        operation,
                        digest: Sha256Digest::of_bytes(normalize_sql(&value).as_bytes()),
                        tables,
                    });
                }
            }
        }
        // String nodes can contain grammar-specific content children.  They
        // are part of the same literal and must not become duplicate sites.
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(language, child, source, sites);
    }
}

fn has_sql_context(mut node: Node<'_>, source: &str) -> bool {
    for _ in 0..6 {
        let Some(parent) = node.parent() else {
            break;
        };
        let parent_text = node_text(parent, source);
        let prefix_len = node.start_byte().saturating_sub(parent.start_byte());
        let prefix = parent_text
            .get(..prefix_len.min(parent_text.len()))
            .unwrap_or(parent_text)
            .to_ascii_lowercase();
        if [
            ".execute",
            ".executemany",
            ".query",
            "preparestatement",
            "createquery",
            "createnativequery",
            "sqlx::query",
            "query!",
            "query_as!",
            "sql_query",
        ]
        .iter()
        .any(|marker| prefix.contains(marker))
        {
            return true;
        }
        if prefix.contains('=') {
            let before_equals = prefix
                .rsplit_once('=')
                .map(|(left, _)| left)
                .unwrap_or(&prefix);
            let variable_tokens = before_equals
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_');
            if variable_tokens
                .filter(|token| !token.is_empty())
                .any(|token| matches!(token, "sql" | "query" | "statement" | "command" | "stmt"))
            {
                return true;
            }
        }
        node = parent;
    }
    false
}

fn is_string_node(language: &str, kind: &str) -> bool {
    match language {
        "typescript" | "javascript" => matches!(kind, "string" | "template_string"),
        "python" => matches!(kind, "string" | "concatenated_string"),
        "java" | "csharp" | "c" | "cpp" | "rust" | "dart" => {
            matches!(kind, "string_literal" | "raw_string_literal")
        }
        "go" => matches!(kind, "interpreted_string_literal" | "raw_string_literal"),
        _ => false,
    }
}

fn exact_literal_value(language: &str, raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    if matches!(language, "typescript" | "javascript") && text.starts_with('`') {
        if !text.ends_with('`') || text.contains("${") {
            return None;
        }
        return Some(text[1..text.len() - 1].to_string());
    }
    if language == "python" {
        let quote_at = text.find(['\'', '"'])?;
        let prefix = text[..quote_at].to_ascii_lowercase();
        if prefix.contains('f') {
            return None;
        }
        return strip_quoted(&text[quote_at..]);
    }
    if language == "rust" && text.starts_with('r') {
        let quote = text.find('"')?;
        let hashes = &text[1..quote];
        if !hashes.bytes().all(|byte| byte == b'#') {
            return None;
        }
        let suffix = format!("\"{hashes}");
        return text
            .strip_suffix(&suffix)
            .and_then(|body| body.get(quote + 1..))
            .map(ToString::to_string);
    }
    if language == "cpp" && text.starts_with("R\"") {
        let open = text.find('(')?;
        let delimiter = &text[2..open];
        let suffix = format!("){delimiter}\"");
        return text
            .strip_suffix(&suffix)
            .and_then(|body| body.get(open + 1..))
            .map(ToString::to_string);
    }
    if language == "csharp" && text.starts_with("@\"") && text.ends_with('"') {
        return Some(text[2..text.len() - 1].replace("\"\"", "\""));
    }
    if language == "dart"
        && (text.starts_with("r\"")
            || text.starts_with("r'")
            || text.starts_with("R\"")
            || text.starts_with("R'"))
    {
        return strip_quoted(&text[1..]);
    }
    strip_quoted(text)
}

fn strip_quoted(text: &str) -> Option<String> {
    for delimiter in ["\"\"\"", "'''", "\"", "'", "`"] {
        if let Some(body) = text
            .strip_prefix(delimiter)
            .and_then(|body| body.strip_suffix(delimiter))
        {
            return Some(body.to_string());
        }
    }
    None
}

fn classify_sql(sql: &str) -> Option<(String, Vec<SqlTableAccess>)> {
    let tokens = sql_tokens(sql);
    let first = tokens.iter().find(|token| is_identifier(token))?;
    let operation = match first.to_ascii_uppercase().as_str() {
        "SELECT" | "WITH" => "SELECT",
        "INSERT" => "INSERT",
        "UPDATE" => "UPDATE",
        "DELETE" => "DELETE",
        "MERGE" => "MERGE",
        _ => return None,
    }
    .to_string();

    let cte_names = cte_names(&tokens);
    let mut accesses = Vec::new();
    match operation.as_str() {
        "INSERT" => push_after_keyword(&tokens, "INTO", SqlTableAccessKind::Write, &mut accesses),
        "UPDATE" => push_after_index(&tokens, 0, SqlTableAccessKind::Write, &mut accesses),
        "DELETE" => push_after_keyword(&tokens, "FROM", SqlTableAccessKind::Write, &mut accesses),
        "MERGE" => push_after_keyword(&tokens, "INTO", SqlTableAccessKind::Write, &mut accesses),
        _ => {}
    }
    for (index, token) in tokens.iter().enumerate() {
        if token.eq_ignore_ascii_case("FROM") || token.eq_ignore_ascii_case("JOIN") {
            if let Some(table) = table_after(&tokens, index + 1) {
                if !cte_names.iter().any(|cte| cte.eq_ignore_ascii_case(&table)) {
                    accesses.push(SqlTableAccess {
                        table,
                        kind: SqlTableAccessKind::Read,
                    });
                }
            }
        }
    }
    accesses.sort();
    accesses.dedup();
    (!accesses.is_empty()).then_some((operation, accesses))
}

fn push_after_keyword(
    tokens: &[String],
    keyword: &str,
    kind: SqlTableAccessKind,
    accesses: &mut Vec<SqlTableAccess>,
) {
    if let Some(index) = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case(keyword))
    {
        push_after_index(tokens, index, kind, accesses);
    }
}

fn push_after_index(
    tokens: &[String],
    index: usize,
    kind: SqlTableAccessKind,
    accesses: &mut Vec<SqlTableAccess>,
) {
    if let Some(table) = table_after(tokens, index + 1) {
        accesses.push(SqlTableAccess { table, kind });
    }
}

fn table_after(tokens: &[String], mut index: usize) -> Option<String> {
    while tokens
        .get(index)
        .is_some_and(|token| matches!(token.to_ascii_uppercase().as_str(), "ONLY" | "LATERAL"))
    {
        index += 1;
    }
    let first = tokens.get(index)?;
    if first == "(" || !is_identifier(first) || reserved_table_token(first) {
        return None;
    }
    let mut table = normalize_identifier(first);
    while tokens.get(index + 1).is_some_and(|token| token == ".") {
        let next = tokens.get(index + 2)?;
        if !is_identifier(next) {
            break;
        }
        table.push('.');
        table.push_str(&normalize_identifier(next));
        index += 2;
    }
    (!table.is_empty()).then_some(table)
}

fn cte_names(tokens: &[String]) -> Vec<String> {
    if !tokens
        .first()
        .is_some_and(|token| token.eq_ignore_ascii_case("WITH"))
    {
        return Vec::new();
    }
    let mut names = Vec::new();
    let mut index = 1;
    if tokens
        .get(index)
        .is_some_and(|token| token.eq_ignore_ascii_case("RECURSIVE"))
    {
        index += 1;
    }
    while let Some(name) = tokens.get(index) {
        if !is_identifier(name) {
            break;
        }
        let mut cursor = index + 1;
        if tokens.get(cursor).is_some_and(|token| token == "(") {
            let mut depth = 1_i32;
            cursor += 1;
            while let Some(token) = tokens.get(cursor) {
                match token.as_str() {
                    "(" => depth += 1,
                    ")" => {
                        depth -= 1;
                        if depth == 0 {
                            cursor += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
        }
        if !tokens
            .get(cursor)
            .is_some_and(|token| token.eq_ignore_ascii_case("AS"))
        {
            break;
        }
        names.push(normalize_identifier(name));
        // Find the balanced CTE body, then continue only after a comma.
        cursor += 1;
        if tokens.get(cursor).is_none_or(|token| token != "(") {
            break;
        }
        let mut depth = 1_i32;
        cursor += 1;
        while let Some(token) = tokens.get(cursor) {
            match token.as_str() {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        cursor += 1;
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if tokens.get(cursor).is_some_and(|token| token == ",") {
            index = cursor + 1;
        } else {
            break;
        }
    }
    names
}

fn reserved_table_token(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "SELECT" | "VALUES" | "UNNEST" | "TABLE" | "SET" | "WHERE" | "RETURNING"
    )
}

fn is_identifier(token: &str) -> bool {
    !token.is_empty()
        && token != "."
        && token != ","
        && token != "("
        && token != ")"
        && token != ";"
        && token != "?"
        && !token.starts_with('$')
        && !token.starts_with(':')
        && !token.starts_with('%')
        && token
            .chars()
            .any(|character| character.is_alphanumeric() || character == '_')
}

fn normalize_identifier(token: &str) -> String {
    token
        .trim_matches(|character| matches!(character, '"' | '`' | '[' | ']'))
        .to_ascii_lowercase()
}

fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sql_tokens(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if byte == b'\'' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\'' {
                    if bytes.get(index + 1) == Some(&b'\'') {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            tokens.push("?".to_string());
            continue;
        }
        if matches!(byte, b'(' | b')' | b',' | b'.' | b';') {
            tokens.push((byte as char).to_string());
            index += 1;
            continue;
        }
        if matches!(byte, b'"' | b'`' | b'[') {
            let closing = if byte == b'[' { b']' } else { byte };
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == closing {
                    index += 1;
                    break;
                }
                index += 1;
            }
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).to_string());
            continue;
        }
        let start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && !matches!(bytes[index], b'(' | b')' | b',' | b'.' | b';' | b'\'')
        {
            index += 1;
        }
        if start != index {
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).to_string());
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_pipeline::language_ir::syntax::parse_tree;

    fn inventory(language: &str, path: &str, source: &str) -> Vec<SqlQuerySite> {
        let tree = parse_tree(language, path, source, "sql-literal-test").unwrap();
        inventory_sql_query_literals_from_root(language, tree.root_node(), source)
    }

    #[test]
    fn python_multiline_query_reports_exact_read_and_write_tables() {
        let source = r#"def save(connection):
    connection.execute('''
        INSERT INTO public.sessions (id)
        SELECT id FROM staged_sessions
    ''')
"#;
        let sites = inventory("python", "repository.py", source);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].operation, "INSERT");
        assert_eq!(
            sites[0].tables,
            vec![
                SqlTableAccess {
                    table: "public.sessions".to_string(),
                    kind: SqlTableAccessKind::Write,
                },
                SqlTableAccess {
                    table: "staged_sessions".to_string(),
                    kind: SqlTableAccessKind::Read,
                },
            ]
        );
    }

    #[test]
    fn interpolated_or_non_sql_strings_are_not_promoted() {
        let source = "const dynamic = `SELECT * FROM ${table}`;\nconst prose = 'SELECT a choice FROM menu';\n";
        assert!(inventory("javascript", "fixture.js", source).is_empty());
    }

    #[test]
    fn common_supported_language_literals_share_the_same_table_contract() {
        let fixtures = [
            (
                "typescript",
                "fixture.ts",
                "const sql = \"UPDATE accounts SET active = 1\";",
            ),
            (
                "java",
                "Fixture.java",
                "class Fixture { String sql = \"UPDATE accounts SET active = 1\"; }",
            ),
            (
                "csharp",
                "Fixture.cs",
                "class Fixture { string sql = \"UPDATE accounts SET active = 1\"; }",
            ),
            (
                "go",
                "fixture.go",
                "package fixture\nvar sql = `UPDATE accounts SET active = 1`",
            ),
            (
                "rust",
                "fixture.rs",
                "const SQL: &str = r#\"UPDATE accounts SET active = 1\"#;",
            ),
            (
                "dart",
                "fixture.dart",
                "const sql = 'UPDATE accounts SET active = 1';",
            ),
        ];
        for (language, path, source) in fixtures {
            let sites = inventory(language, path, source);
            assert_eq!(sites.len(), 1, "{language}");
            assert_eq!(sites[0].tables[0].table, "accounts", "{language}");
            assert_eq!(
                sites[0].tables[0].kind,
                SqlTableAccessKind::Write,
                "{language}"
            );
        }
    }
}
