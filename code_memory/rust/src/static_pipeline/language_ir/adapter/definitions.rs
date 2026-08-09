//! Reconciles provider definitions with the independent source declaration inventory.
//!
//! Provider symbols keep identity, while exact source declarations own the
//! display name, callable header, visibility, kind refinement, and structural
//! owner. Ambiguous or missing ownership remains omitted and audited.

use super::{
    AnalysisUnit, BTreeMap, BTreeSet, DefinitionAudit, DefinitionAuditFailure, EvidenceId,
    FactNodeKind, ProgrammingLanguage, ProviderProtocol, ProviderSymbolId, RepositoryPath,
    SourceFileKind, SourceFlags, SymbolOutput, SyntaxDefinition, Visibility,
};

#[derive(Clone)]
pub(super) struct DefinitionDraft {
    pub(super) symbol_id: ProviderSymbolId,
    pub(super) native_kind: String,
    pub(super) canonical_kind_hint: FactNodeKind,
    pub(super) qualified_name: String,
    pub(super) display_name: String,
    pub(super) signature: Option<String>,
    pub(super) visibility: Visibility,
    pub(super) parent_symbol_id: Option<ProviderSymbolId>,
    pub(super) definition_evidence_id: EvidenceId,
    pub(super) flags: SourceFlags,
    pub(super) field_candidate: bool,
    pub(super) path: RepositoryPath,
    pub(super) provider_range: Vec<i32>,
    pub(super) syntax_match: Option<usize>,
}

pub(super) fn reconcile_definition_inventory(
    unit: &AnalysisUnit,
    protocol: ProviderProtocol,
    syntax_by_path: &BTreeMap<RepositoryPath, Vec<SyntaxDefinition>>,
    definitions: &mut BTreeMap<ProviderSymbolId, DefinitionDraft>,
    audit: &mut DefinitionAudit,
) -> BTreeMap<ProviderSymbolId, ProviderSymbolId> {
    let mut matched_syntax = BTreeMap::<(RepositoryPath, usize), ProviderSymbolId>::new();
    let mut provider_aliases = BTreeMap::<ProviderSymbolId, ProviderSymbolId>::new();

    // The source denominator is keyed by file, so index provider definitions by
    // file once as well. Scanning the complete provider definition map for each
    // source file made reconciliation O(files * definitions): a 9k-file Java
    // workspace repeated tens of millions of ordered-map visits before doing
    // any real matching. Keep the former deterministic length/id order inside
    // each bucket so this is a performance-only change.
    let mut provider_ids_by_path = BTreeMap::<RepositoryPath, Vec<ProviderSymbolId>>::new();
    for (id, draft) in definitions.iter() {
        provider_ids_by_path
            .entry(draft.path.clone())
            .or_default()
            .push(id.clone());
    }
    for provider_ids in provider_ids_by_path.values_mut() {
        provider_ids.sort_by(|left, right| {
            left.as_str()
                .len()
                .cmp(&right.as_str().len())
                .then_with(|| left.cmp(right))
        });
    }

    for (path, syntax) in syntax_by_path {
        let provider_ids = provider_ids_by_path.get(path).cloned().unwrap_or_default();
        let mut used = BTreeSet::<usize>::new();

        for id in provider_ids {
            let Some(draft) = definitions.get(&id) else {
                continue;
            };
            if draft.canonical_kind_hint == FactNodeKind::Namespace {
                continue;
            }
            let selected = select_syntax_definition(draft, syntax, &used, protocol);
            let Some(index) = selected else {
                if let Some(primary) = select_provider_definition_alias(
                    draft,
                    syntax,
                    &used,
                    path,
                    &matched_syntax,
                    definitions,
                    protocol,
                ) {
                    provider_aliases.insert(id.clone(), primary);
                    audit.provider_definition_alias_count += 1;
                    continue;
                }
                if !draft.field_candidate {
                    audit.extra_provider_definition_count += 1;
                    audit.failures.push(DefinitionAuditFailure {
                        unit_id: unit.id.as_str().to_string(),
                        language: unit.language,
                        path: path.clone(),
                        name: draft.display_name.clone(),
                        reason: "provider-definition-without-source-declaration",
                        expected_kind: None,
                        provider_kind: Some(draft.canonical_kind_hint),
                    });
                }
                continue;
            };
            used.insert(index);
            matched_syntax.insert((path.clone(), index), id.clone());
            audit.matched_definition_count += 1;
            if let Some(draft) = definitions.get_mut(&id) {
                draft.syntax_match = Some(index);
                // The source token is the authoritative human-readable name.
                // SCIP parameter and constructor descriptors may be encoded as
                // `(value)` or `<constructor>`, while LSPs may use `.ctor`.
                // Keep the provider symbol as identity, but never leak those
                // protocol spellings (or an empty name) into the product IR.
                draft.display_name = syntax[index].name.clone();
                // A source declaration is the uniform authority for callable
                // headers and language-defined accessibility. Provider
                // signatures remain a fallback for non-callable symbols.
                if syntax[index].signature.is_some() {
                    draft.signature = syntax[index].signature.clone();
                }
                draft.visibility = syntax[index].visibility;
                // Syntax reconciliation has now decided whether a provider
                // Variable is a real field (or another explicit declaration).
                // The old provider-only field heuristic must not delete the
                // reconciled definition later.
                draft.field_candidate = false;
                if draft.canonical_kind_hint != syntax[index].kind {
                    draft.canonical_kind_hint = syntax[index].kind;
                    audit.kind_refinement_count += 1;
                }
            }
        }

        for (index, definition) in syntax.iter().enumerate() {
            if used.contains(&index) {
                continue;
            }
            audit.missing_syntax_definition_count += 1;
            audit.failures.push(DefinitionAuditFailure {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                path: path.clone(),
                name: definition.name.clone(),
                reason: "source-declaration-missing-from-provider",
                expected_kind: Some(definition.kind),
                provider_kind: None,
            });
        }
    }

    let provider_kinds = definitions
        .iter()
        .map(|(id, draft)| (id.clone(), draft.canonical_kind_hint))
        .collect::<BTreeMap<_, _>>();
    let matched = definitions
        .iter()
        .filter_map(|(id, draft)| {
            draft
                .syntax_match
                .map(|index| (id.clone(), draft.path.clone(), index))
        })
        .collect::<Vec<_>>();
    let mut unresolved_definition_ids = BTreeSet::<ProviderSymbolId>::new();
    for (id, path, index) in matched {
        let Some(candidate) = syntax_by_path
            .get(&path)
            .and_then(|definitions| definitions.get(index))
        else {
            continue;
        };
        let expected_parent = candidate
            .parent_name_range(protocol)
            .and_then(|parent_range| {
                syntax_by_path.get(&path).and_then(|definitions| {
                    definitions
                        .iter()
                        .position(|definition| definition.name_range(protocol) == parent_range)
                })
            });
        let expected_parent =
            expected_parent.and_then(|parent| matched_syntax.get(&(path.clone(), parent)).cloned());
        let current_parent = definitions
            .get(&id)
            .and_then(|draft| draft.parent_symbol_id.clone());
        if candidate.parent_name_range(protocol).is_some() && expected_parent.is_none() {
            audit.unresolved_owner_count += 1;
            audit.matched_definition_count = audit.matched_definition_count.saturating_sub(1);
            unresolved_definition_ids.insert(id.clone());
            audit.failures.push(DefinitionAuditFailure {
                unit_id: unit.id.as_str().to_string(),
                language: unit.language,
                path: path.clone(),
                name: candidate.name.clone(),
                reason: "source-owner-missing-from-provider",
                expected_kind: Some(candidate.kind),
                provider_kind: definitions.get(&id).map(|draft| draft.canonical_kind_hint),
            });
            continue;
        }
        if candidate.parent_name_range(protocol).is_some() {
            audit.resolved_owner_count += 1;
        }
        let normalized_parent = match expected_parent {
            Some(parent) => Some(parent),
            None => current_parent
                .clone()
                .filter(|parent| provider_kinds.get(parent) != Some(&FactNodeKind::Namespace)),
        };
        if normalized_parent != current_parent {
            audit.owner_repair_count += 1;
            if let Some(draft) = definitions.get_mut(&id) {
                draft.parent_symbol_id = normalized_parent;
            }
        }
    }

    definitions.retain(|id, draft| {
        !unresolved_definition_ids.contains(id)
            && (draft.syntax_match.is_some() || !syntax_by_path.contains_key(&draft.path))
    });
    audit.failures.sort();
    audit.failures.dedup();
    provider_aliases
}

fn select_provider_definition_alias(
    draft: &DefinitionDraft,
    syntax: &[SyntaxDefinition],
    used: &BTreeSet<usize>,
    path: &RepositoryPath,
    matched_syntax: &BTreeMap<(RepositoryPath, usize), ProviderSymbolId>,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
    protocol: ProviderProtocol,
) -> Option<ProviderSymbolId> {
    let aliases = used
        .iter()
        .copied()
        .filter(|index| ranges_equal(&draft.provider_range, syntax[*index].name_range(protocol)))
        .filter_map(|index| {
            let primary = matched_syntax.get(&(path.clone(), index))?;
            let primary_kind = definitions.get(primary)?.canonical_kind_hint;
            definition_kinds_can_alias(primary_kind, draft.canonical_kind_hint, syntax[index].kind)
                .then(|| primary.clone())
        })
        .collect::<Vec<_>>();
    (aliases.len() == 1).then(|| aliases[0].clone())
}

fn definition_kinds_can_alias(
    primary: FactNodeKind,
    duplicate: FactNodeKind,
    syntax: FactNodeKind,
) -> bool {
    definition_kind_matches_source(primary, syntax)
        && definition_kind_matches_source(duplicate, syntax)
}

fn definition_kind_matches_source(provider: FactNodeKind, syntax: FactNodeKind) -> bool {
    provider == syntax
        || provider == FactNodeKind::Type && is_type_owner_kind(syntax)
        || provider == FactNodeKind::Method && syntax == FactNodeKind::Constructor
}

fn select_syntax_definition(
    draft: &DefinitionDraft,
    syntax: &[SyntaxDefinition],
    used: &BTreeSet<usize>,
    protocol: ProviderProtocol,
) -> Option<usize> {
    let positional = syntax
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            !used.contains(index)
                && candidate.matches_provider_range(&draft.provider_range, protocol)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let exact_range = positional
        .iter()
        .copied()
        .filter(|index| ranges_equal(&draft.provider_range, syntax[*index].name_range(protocol)))
        .collect::<Vec<_>>();
    if exact_range.len() == 1 {
        return exact_range.first().copied();
    }
    if let Some(point) = provider_symbol_source_point(&draft.symbol_id) {
        let exact = positional
            .iter()
            .copied()
            .filter(|index| range_start(syntax[*index].name_range(protocol)) == Some(point))
            .collect::<Vec<_>>();
        if exact.len() == 1 {
            return exact.first().copied();
        }
    }
    let provider_name = definition_base_name(&draft.display_name);
    let same_name = positional
        .iter()
        .copied()
        .filter(|index| syntax[*index].name == provider_name)
        .collect::<Vec<_>>();
    (same_name.len() == 1).then(|| same_name[0])
}

fn provider_symbol_source_point(symbol: &ProviderSymbolId) -> Option<(i32, i32)> {
    let value = symbol.as_str();
    if !value.starts_with("lsp ") {
        return None;
    }
    let (_, location) = value.rsplit_once('@')?;
    let (line, column) = location.split_once(':')?;
    Some((line.parse().ok()?, column.parse().ok()?))
}

fn range_start(range: &[i32]) -> Option<(i32, i32)> {
    match range {
        [line, start, ..] => Some((*line, *start)),
        _ => None,
    }
}

pub(super) fn ranges_equal(left: &[i32], right: &[i32]) -> bool {
    canonical_range_bounds(left) == canonical_range_bounds(right)
}

fn canonical_range_bounds(range: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match range {
        [line, start, end] => Some(((*line, *start), (*line, *end))),
        [start_line, start_column, end_line, end_column, ..] => {
            Some(((*start_line, *start_column), (*end_line, *end_column)))
        }
        _ => None,
    }
}

pub(super) fn canonical_definition_kind(native: &str) -> Option<FactNodeKind> {
    let compact = native
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match compact.as_str() {
        "namespace" | "package" | "module" => Some(FactNodeKind::Namespace),
        "type" => Some(FactNodeKind::Type),
        "class" => Some(FactNodeKind::Class),
        "interface" => Some(FactNodeKind::Interface),
        "trait" | "mixin" => Some(FactNodeKind::Trait),
        "struct" | "union" => Some(FactNodeKind::Struct),
        "enum" => Some(FactNodeKind::Enum),
        "typealias" | "typedef" => Some(FactNodeKind::TypeAlias),
        "function" | "operator" => Some(FactNodeKind::Function),
        "method" => Some(FactNodeKind::Method),
        "constructor" => Some(FactNodeKind::Constructor),
        "constant" => Some(FactNodeKind::Constant),
        "field" | "property" => Some(FactNodeKind::Field),
        _ => None,
    }
}

pub(super) fn is_variable_definition_kind(native: &str) -> bool {
    native
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .eq("variable".chars())
}

pub(super) fn reconcile_definition_drafts(
    language: ProgrammingLanguage,
    definitions: &mut BTreeMap<ProviderSymbolId, DefinitionDraft>,
) {
    // A provider can occasionally report a declaration as its own enclosing
    // symbol (clangd does this for some macro-heavy C declarations). Such an
    // ownership edge is structurally impossible. Preserve the declaration and
    // its evidence, but abstain from emitting the invalid containment claim.
    for (id, draft) in definitions.iter_mut() {
        if draft.parent_symbol_id.as_ref() == Some(id) {
            draft.parent_symbol_id = None;
        }
    }

    let owners = definitions
        .iter()
        .map(|(id, draft)| {
            (
                id.clone(),
                (draft.canonical_kind_hint, draft.display_name.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    definitions.retain(|_, draft| {
        if !draft.field_candidate {
            return true;
        }
        draft
            .parent_symbol_id
            .as_ref()
            .and_then(|parent| owners.get(parent))
            .is_some_and(|(kind, _)| is_type_owner_kind(*kind))
    });

    for draft in definitions.values_mut() {
        let parent = draft
            .parent_symbol_id
            .as_ref()
            .and_then(|parent| owners.get(parent));
        if draft.field_candidate {
            draft.canonical_kind_hint = FactNodeKind::Field;
            continue;
        }
        if draft.canonical_kind_hint != FactNodeKind::Method {
            continue;
        }
        if parent.is_some_and(|(kind, _)| *kind == FactNodeKind::Namespace) {
            draft.canonical_kind_hint = FactNodeKind::Function;
            continue;
        }
        if definition_is_constructor(language, draft, parent) {
            draft.canonical_kind_hint = FactNodeKind::Constructor;
        }
    }
}

pub(super) fn is_type_owner_kind(kind: FactNodeKind) -> bool {
    matches!(
        kind,
        FactNodeKind::Type
            | FactNodeKind::Class
            | FactNodeKind::Interface
            | FactNodeKind::Trait
            | FactNodeKind::Struct
            | FactNodeKind::Enum
    )
}

fn definition_is_constructor(
    language: ProgrammingLanguage,
    draft: &DefinitionDraft,
    parent: Option<&(FactNodeKind, String)>,
) -> bool {
    let child = definition_base_name(&draft.display_name);
    let Some((parent_kind, parent_name)) = parent else {
        return false;
    };
    if !is_type_owner_kind(*parent_kind) {
        return false;
    }
    if matches!(
        child.as_str(),
        "constructor" | "__init__" | ".ctor" | "<init>"
    ) || draft.qualified_name.contains("<constructor>")
        || draft.qualified_name.contains("`.ctor`")
        || draft.qualified_name.contains("<init>")
    {
        return true;
    }
    if child != definition_base_name(parent_name) {
        return false;
    }
    match language {
        ProgrammingLanguage::Dart => true,
        ProgrammingLanguage::Java => draft.signature.is_none(),
        _ => false,
    }
}

pub(super) fn definition_base_name(value: &str) -> String {
    let value = value
        .split('(')
        .next()
        .unwrap_or(value)
        .split('<')
        .next()
        .unwrap_or(value)
        .trim();
    value
        .rsplit(['#', '.', ':', '/', ' '])
        .next()
        .unwrap_or(value)
        .trim_matches('`')
        .to_string()
}

pub(super) fn definition_display_name(symbol: &SymbolOutput) -> String {
    symbol
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| short_symbol_name(&symbol.symbol).to_string())
}

pub(super) fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn short_symbol_name(symbol: &str) -> &str {
    // Only LSP fallback identities use a trailing `@line:column` suffix.
    // SCIP package identities may legitimately contain `@` (notably scoped
    // npm packages such as `@nestjs/core`), so splitting at the first `@`
    // corrupts otherwise exact provider definitions into the name `npm`.
    let symbol = strip_lsp_location_suffix(symbol);
    let property_descriptor = symbol.ends_with(':');
    let symbol = symbol.trim_end_matches(['.', ':', '/', '#']);
    let symbol = symbol.rsplit(['#', '.', ':', '/']).next().unwrap_or(symbol);
    // clangd gives unnamed declarations stable labels such as
    // `(anonymous enum)` and `(anonymous namespace)`. These are displayable
    // structural identities, not callable signatures. Splitting every value
    // at the first `(` turned them into an empty display name and rejected the
    // whole otherwise-valid Language IR stream.
    if symbol.starts_with('(') && symbol.ends_with(')') {
        return symbol;
    }
    let symbol = symbol.split('(').next().unwrap_or(symbol);
    let symbol = symbol.split_whitespace().last().unwrap_or(symbol);
    if property_descriptor {
        symbol.trim_end_matches(char::is_numeric)
    } else {
        symbol
    }
}

fn strip_lsp_location_suffix(symbol: &str) -> &str {
    let Some((identity, suffix)) = symbol.rsplit_once('@') else {
        return symbol;
    };
    let Some((line, column)) = suffix.split_once(':') else {
        return symbol;
    };
    if !line.is_empty()
        && !column.is_empty()
        && line.bytes().all(|value| value.is_ascii_digit())
        && column.bytes().all(|value| value.is_ascii_digit())
    {
        identity
    } else {
        symbol
    }
}

pub(super) fn source_flags(kind: SourceFileKind) -> SourceFlags {
    SourceFlags {
        test: kind == SourceFileKind::Test,
        generated: kind == SourceFileKind::Generated,
        vendor: kind == SourceFileKind::Vendor,
        external: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{reconcile_definition_drafts, short_symbol_name, DefinitionDraft};
    use codebase_fact_model::analysis::ProgrammingLanguage;
    use codebase_fact_model::fact_graph::{FactNodeKind, Visibility};
    use codebase_fact_model::identity::{EvidenceId, ProviderSymbolId};
    use codebase_fact_model::source::SourceFlags;
    use std::collections::BTreeMap;

    fn symbol(value: &str) -> ProviderSymbolId {
        ProviderSymbolId::parse(value).unwrap()
    }

    fn draft(
        id: &str,
        kind: FactNodeKind,
        display_name: &str,
        parent: Option<&str>,
        signature: Option<&str>,
        field_candidate: bool,
    ) -> DefinitionDraft {
        DefinitionDraft {
            symbol_id: symbol(id),
            native_kind: if field_candidate {
                "Variable"
            } else {
                "fixture"
            }
            .to_string(),
            canonical_kind_hint: kind,
            qualified_name: id.to_string(),
            display_name: display_name.to_string(),
            signature: signature.map(str::to_string),
            visibility: Visibility::Unknown,
            parent_symbol_id: parent.map(symbol),
            definition_evidence_id: EvidenceId::from_components(&[id]).unwrap(),
            flags: SourceFlags::default(),
            field_candidate,
            path: codebase_fact_model::source::RepositoryPath::parse("fixture.rs").unwrap(),
            provider_range: vec![0, 0, 1],
            syntax_match: None,
        }
    }

    #[test]
    fn variables_are_fields_only_when_a_type_directly_owns_them() {
        let mut definitions = BTreeMap::from([
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.value"),
                draft(
                    "Box.value",
                    FactNodeKind::Field,
                    "value",
                    Some("Box"),
                    None,
                    true,
                ),
            ),
            (
                symbol("local"),
                draft("local", FactNodeKind::Field, "local", None, None, true),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Go, &mut definitions);

        assert_eq!(
            definitions[&symbol("Box.value")].canonical_kind_hint,
            FactNodeKind::Field
        );
        assert!(!definitions.contains_key(&symbol("local")));
    }

    #[test]
    fn constructor_and_file_function_kinds_use_structural_context() {
        let mut dart = BTreeMap::from([
            (
                symbol("file"),
                draft(
                    "file",
                    FactNodeKind::Namespace,
                    "types.dart",
                    None,
                    None,
                    false,
                ),
            ),
            (
                symbol("file.run"),
                draft(
                    "file.run",
                    FactNodeKind::Method,
                    "run",
                    Some("file"),
                    Some("()"),
                    false,
                ),
            ),
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.ctor"),
                draft(
                    "Box.ctor",
                    FactNodeKind::Method,
                    "Box",
                    Some("Box"),
                    Some("(this.value)"),
                    false,
                ),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Dart, &mut dart);

        assert_eq!(
            dart[&symbol("file.run")].canonical_kind_hint,
            FactNodeKind::Function
        );
        assert_eq!(
            dart[&symbol("Box.ctor")].canonical_kind_hint,
            FactNodeKind::Constructor
        );
    }

    #[test]
    fn java_same_named_void_method_is_not_a_constructor() {
        let mut java = BTreeMap::from([
            (
                symbol("Box"),
                draft("Box", FactNodeKind::Class, "Box", None, None, false),
            ),
            (
                symbol("Box.constructor"),
                draft(
                    "Box.constructor",
                    FactNodeKind::Method,
                    "Box(T)",
                    Some("Box"),
                    None,
                    false,
                ),
            ),
            (
                symbol("Box.void_method"),
                draft(
                    "Box.void_method",
                    FactNodeKind::Method,
                    "Box()",
                    Some("Box"),
                    Some(": void"),
                    false,
                ),
            ),
        ]);

        reconcile_definition_drafts(ProgrammingLanguage::Java, &mut java);

        assert_eq!(
            java[&symbol("Box.constructor")].canonical_kind_hint,
            FactNodeKind::Constructor
        );
        assert_eq!(
            java[&symbol("Box.void_method")].canonical_kind_hint,
            FactNodeKind::Method
        );
    }

    #[test]
    fn clangd_anonymous_declarations_keep_a_non_empty_display_name() {
        assert_eq!(
            short_symbol_name("lsp . . . include.fmt.base.h#(anonymous enum)@447:0"),
            "(anonymous enum)"
        );
        assert_eq!(
            short_symbol_name("lsp . . . src.os.cc#(anonymous namespace)@62:10"),
            "(anonymous namespace)"
        );
    }

    #[test]
    fn scoped_npm_symbols_keep_their_real_definition_name() {
        assert_eq!(
            short_symbol_name(
                "scip-typescript npm @nestjs/core 11.1.26 src/cats/`cats.controller.ts`/CatsController#create()."
            ),
            "create"
        );
        assert_eq!(
            short_symbol_name("scip-typescript npm @scope/pkg 1.0.0 src/service.ts/ScopedService#"),
            "ScopedService"
        );
        assert_eq!(
            short_symbol_name("lsp . . . src/service.ts#ScopedService@17:4"),
            "ScopedService"
        );
    }

    #[test]
    fn impossible_self_parent_is_removed_without_dropping_the_definition() {
        let mut definitions = BTreeMap::from([(
            symbol("uv_loop_init"),
            draft(
                "uv_loop_init",
                FactNodeKind::Function,
                "uv_loop_init",
                Some("uv_loop_init"),
                Some("(uv_loop_t*)"),
                false,
            ),
        )]);

        reconcile_definition_drafts(ProgrammingLanguage::C, &mut definitions);

        let definition = &definitions[&symbol("uv_loop_init")];
        assert!(definition.parent_symbol_id.is_none());
        assert_eq!(definition.display_name, "uv_loop_init");
    }
}
