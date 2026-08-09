//! Shared syntax and source-coordinate primitives for independent inventories.
//!
//! Definition, import, and later type-relation inventories must parse the same
//! file with the same grammar and translate byte columns the same way. Keeping
//! these primitives here prevents capability-specific coordinate drift.

use tree_sitter::{Language, Node, Parser, Point, Tree};

pub(crate) fn parse_tree(
    language: &str,
    path: &str,
    source: &str,
    capability: &str,
) -> Result<Tree, String> {
    let parser_language = parser_language(language, path)
        .ok_or_else(|| format!("no syntax grammar is registered for {language}"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&parser_language)
        .map_err(|error| format!("cannot load {language} {capability} grammar: {error}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| format!("{language} {capability} parser returned no tree"))?;
    if tree.root_node().has_error() {
        return Err(format!(
            "{language} {capability} parser produced an incomplete syntax tree for {path}"
        ));
    }
    Ok(tree)
}

fn parser_language(language: &str, path: &str) -> Option<Language> {
    match language {
        "typescript" if path.ends_with(".tsx") => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "csharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "dart" => Some(tree_sitter_dart::LANGUAGE.into()),
        _ => None,
    }
}

pub(crate) fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

pub(crate) fn utf8_range(node: Node<'_>) -> Vec<i32> {
    point_range(node.start_position(), node.end_position())
}

pub(crate) fn point_range(start: Point, end: Point) -> Vec<i32> {
    vec![
        start.row as i32,
        start.column as i32,
        end.row as i32,
        end.column as i32,
    ]
}

pub(crate) fn node_utf16_range(source: &str, node: Node<'_>) -> Vec<i32> {
    utf16_range(source, node.start_position(), node.end_position())
}

pub(crate) fn utf16_range(source: &str, start: Point, end: Point) -> Vec<i32> {
    vec![
        start.row as i32,
        utf16_column(source, start) as i32,
        end.row as i32,
        utf16_column(source, end) as i32,
    ]
}

fn utf16_column(source: &str, point: Point) -> usize {
    source
        .lines()
        .nth(point.row)
        .and_then(|line| line.get(..point.column))
        .map(|prefix| prefix.encode_utf16().count())
        .unwrap_or(point.column)
}

pub(crate) fn ranges_equal(left: &[i32], right: &[i32]) -> bool {
    canonical_bounds(left) == canonical_bounds(right)
}

pub(crate) fn range_contains(outer: &[i32], inner: &[i32]) -> bool {
    let Some((outer_start, outer_end)) = canonical_bounds(outer) else {
        return false;
    };
    let Some((inner_start, inner_end)) = canonical_bounds(inner) else {
        return false;
    };
    outer_start <= inner_start && inner_end <= outer_end
}

fn canonical_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match range {
        [line, start, end] => Some(((*line, *start), (*line, *end))),
        [start_line, start_column, end_line, end_column, ..] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_and_utf16_ranges_share_lines_but_not_non_ascii_columns() {
        let source = "const 한글 = 1;\n";
        let tree = parse_tree("typescript", "fixture.ts", source, "test").unwrap();
        let identifier = tree
            .root_node()
            .descendant_for_byte_range(6, 12)
            .expect("identifier range");
        assert_eq!(utf8_range(identifier), vec![0, 6, 0, 12]);
        assert_eq!(node_utf16_range(source, identifier), vec![0, 6, 0, 8]);
    }

    #[test]
    fn range_comparison_accepts_compact_and_expanded_single_line_shapes() {
        assert!(ranges_equal(&[2, 3, 8], &[2, 3, 2, 8]));
        assert!(range_contains(&[2, 1, 10], &[2, 3, 2, 8]));
    }
}
