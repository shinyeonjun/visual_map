use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

use crate::workspace::{FocusedCodeSearch, FocusedCodeSearchMatch};

use super::model::{
    CandidateLink, Evidence, InventoryItem, InventorySnapshot, SnapshotGap, SnapshotLink,
    SourceLocation,
};

pub(crate) const MAX_CANDIDATES_PER_CODE_ITEM: usize = 6;
const CANDIDATE_CACHE_LIMIT: usize = 8;
const GENERIC_DB_TERMS: &[&str] = &[
    "data", "id", "item", "items", "main", "object", "objects", "public", "record", "records",
    "state", "status", "table", "tables", "type", "value", "values",
];

static CANDIDATE_CACHE: OnceLock<Mutex<HashMap<CandidateCacheKey, Arc<Vec<CandidateLink>>>>> =
    OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CandidateSnapshotIdentity {
    saved_at: String,
    schema_version: u32,
    item_count: usize,
    link_count: usize,
    candidate_input_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CandidateCacheKey {
    workspace_id: String,
    identity: CandidateSnapshotIdentity,
}

struct TableTerms<'a> {
    table: &'a InventoryItem,
    aliases: Vec<String>,
}

struct RankedCandidate<'a> {
    table: &'a InventoryItem,
    score: u16,
    evidence: Vec<Evidence>,
}

pub(crate) struct AppliedCodeEvidence {
    pub matched_files: Vec<String>,
    pub matches: Vec<AppliedCodeMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedCodeMatch {
    pub item_id: String,
    pub file: String,
    pub start_line: u64,
    pub end_line: u64,
    pub match_lines: Vec<u64>,
}

pub(crate) fn apply_focused_code_evidence(
    snapshot: &mut InventorySnapshot,
    target_id: &str,
    search: &FocusedCodeSearch,
    schema_ambiguous: bool,
) -> AppliedCodeEvidence {
    let Some(target) = snapshot
        .items
        .iter()
        .find(|item| item.id == target_id)
        .cloned()
    else {
        return AppliedCodeEvidence {
            matched_files: Vec::new(),
            matches: Vec::new(),
        };
    };
    let link_kind = match target.kind.as_str() {
        "table" => "code_db_text_reference",
        "column" => "code_db_column_text_reference",
        _ => {
            return AppliedCodeEvidence {
                matched_files: Vec::new(),
                matches: Vec::new(),
            };
        }
    };

    record_search_coverage_gaps(snapshot, &target, search);
    let mapped = search
        .matches
        .iter()
        .filter_map(|search_match| {
            map_search_match(snapshot, search_match).map(|index| (index, search_match.clone()))
        })
        .collect::<Vec<_>>();
    if mapped.len() < search.matches.len() {
        record_code_search_gap(
            snapshot,
            target.id.as_str(),
            "code-search-unmapped",
            &format!(
                "검색 결과 {}개 중 {}개는 코드 인벤토리의 고유한 위치와 연결되지 않았습니다.",
                search.matches.len(),
                search.matches.len() - mapped.len()
            ),
            Vec::new(),
        );
    }

    let mut matched_files = BTreeSet::new();
    let mut applied_matches = Vec::new();
    for (index, search_match) in mapped {
        let item = &mut snapshot.items[index];
        let path = item
            .path
            .clone()
            .or_else(|| item.location.as_ref().map(|location| location.path.clone()))
            .unwrap_or_else(|| search_match.file.clone());
        let line = search_match
            .match_lines
            .first()
            .copied()
            .unwrap_or(search_match.start_line);
        matched_files.insert(path.clone());
        applied_matches.push(AppliedCodeMatch {
            item_id: item.id.clone(),
            file: path.clone(),
            start_line: search_match.start_line,
            end_line: search_match.end_line,
            match_lines: search_match.match_lines.clone(),
        });
        item.path.get_or_insert_with(|| path.clone());
        item.location = Some(SourceLocation {
            path: path.clone(),
            line: Some(line),
            column: None,
            end_line: Some(line),
            end_column: None,
        });

        let mut evidence = vec![Evidence {
            kind: "code-search-exact-token".to_string(),
            text: format!(
                "search_code가 {}:L{}의 {} 범위에서 {} 식별자 토큰을 찾았습니다.",
                path, line, search_match.label, target.name
            ),
        }];
        if schema_ambiguous {
            evidence.push(Evidence {
                kind: "code-search-schema-ambiguous".to_string(),
                text: "동일한 테이블명이 여러 스키마에 있어 이 텍스트 근거만으로 대상을 확정할 수 없습니다."
                    .to_string(),
            });
        }
        let link = SnapshotLink {
            id: format!("{link_kind}:{}->{}", item.id, target.id),
            from: item.id.clone(),
            to: target.id.clone(),
            kind: link_kind.to_string(),
            label: Some("search_code exact token".to_string()),
            truth_class: "candidate".to_string(),
            direction: "outbound".to_string(),
            engine_edge_type: Some("SEARCH_CODE_EXACT_TOKEN".to_string()),
            evidence,
        };
        if !snapshot.links.iter().any(|existing| {
            existing.kind == link.kind && existing.from == link.from && existing.to == link.to
        }) {
            snapshot.links.push(link);
        }
    }

    AppliedCodeEvidence {
        matched_files: matched_files.into_iter().collect(),
        matches: applied_matches,
    }
}

pub(crate) fn record_code_search_gap(
    snapshot: &mut InventorySnapshot,
    target_id: &str,
    kind: &str,
    message: &str,
    mut related_ids: Vec<String>,
) {
    related_ids.push(target_id.to_string());
    related_ids.sort();
    related_ids.dedup();
    let id = format!("gap:{kind}:{target_id}");
    if snapshot.metadata.gaps.iter().any(|gap| gap.id == id) {
        return;
    }
    snapshot.metadata.gaps.push(SnapshotGap {
        id,
        kind: kind.to_string(),
        message: message.to_string(),
        related_ids,
    });
}

fn record_search_coverage_gaps(
    snapshot: &mut InventorySnapshot,
    target: &InventoryItem,
    search: &FocusedCodeSearch,
) {
    if search.matches.is_empty() {
        record_code_search_gap(
            snapshot,
            target.id.as_str(),
            "code-search-empty",
            "정확한 식별자 토큰을 포함한 코드 위치를 찾지 못했습니다. 코드 영향 없음으로 확정하지 않습니다.",
            Vec::new(),
        );
    }
    if !search.partial_reasons.is_empty() {
        let reasons = search
            .partial_reasons
            .iter()
            .map(|reason| match reason.as_str() {
                "result-limit" => "결과 상한 도달",
                "grep-limit" => "원문 검색 500건 상한 도달",
                "unmapped-raw-matches" => "그래프 노드 밖 원문 일치 존재",
                "engine-stderr" => "일부 파일 검색 실패 가능",
                _ => "검색 범위 일부 미확인",
            })
            .collect::<Vec<_>>()
            .join(", ");
        record_code_search_gap(
            snapshot,
            target.id.as_str(),
            "code-search-partial",
            &format!("코드 텍스트 검색이 완전하지 않을 수 있습니다: {reasons}."),
            Vec::new(),
        );
    }
}

fn map_search_match(
    snapshot: &InventorySnapshot,
    search_match: &FocusedCodeSearchMatch,
) -> Option<usize> {
    let exact = snapshot
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.is_code()
                && item.qualified_name.as_deref() == Some(search_match.qualified_name.as_str())
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return exact.first().copied();
    }

    let mut candidates = snapshot.items.iter().enumerate().filter(|(_, item)| {
        item.is_code()
            && item
                .qualified_name
                .as_deref()
                .is_some_and(|name| engine_ascii(name) == search_match.qualified_name)
            && search_match_location_is_exact(item, search_match)
    });
    let (index, _) = candidates.next()?;
    candidates.next().is_none().then_some(index)
}

fn search_match_location_is_exact(
    item: &InventoryItem,
    search_match: &FocusedCodeSearchMatch,
) -> bool {
    item_path(item).is_some_and(|path| engine_ascii(&path.replace('\\', "/")) == search_match.file)
        && item.location.as_ref().and_then(|location| location.line)
            == Some(search_match.start_line)
        && item
            .location
            .as_ref()
            .and_then(|location| location.end_line)
            == Some(search_match.end_line)
        && item.engine_label.as_deref() == Some(search_match.label.as_str())
}

fn item_path(item: &InventoryItem) -> Option<&str> {
    item.path.as_deref().or_else(|| {
        item.location
            .as_ref()
            .map(|location| location.path.as_str())
    })
}

fn engine_ascii(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| if byte.is_ascii() { *byte as char } else { '?' })
        .collect()
}

pub(super) fn candidate_links(snapshot: &InventorySnapshot) -> Arc<Vec<CandidateLink>> {
    let identity = CandidateSnapshotIdentity {
        saved_at: snapshot.saved_at.clone(),
        schema_version: snapshot.schema_version,
        item_count: snapshot.items.len(),
        link_count: snapshot.links.len(),
        candidate_input_hash: candidate_input_hash(snapshot),
    };
    let key = CandidateCacheKey {
        workspace_id: snapshot.workspace_id.clone(),
        identity,
    };
    let cache = CANDIDATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(links) = cache.get(&key) {
            return Arc::clone(links);
        }
    }

    let (computed, _) = compute_candidate_links(snapshot, None, None, None, None);
    let links = Arc::new(computed);
    if let Ok(mut cache) = cache.lock() {
        while cache.len() >= CANDIDATE_CACHE_LIMIT && !cache.contains_key(&key) {
            let Some(oldest_key) = cache.keys().next().cloned() else {
                break;
            };
            cache.remove(&oldest_key);
        }
        cache.insert(key, Arc::clone(&links));
    }
    links
}

pub(super) fn candidate_links_for(
    snapshot: &InventorySnapshot,
    code_ids: &HashSet<String>,
    max_candidates: usize,
    max_source_items: usize,
    max_source_links: usize,
) -> (Vec<CandidateLink>, bool) {
    if code_ids.is_empty() {
        return (Vec::new(), false);
    }
    compute_candidate_links(
        snapshot,
        Some(code_ids),
        Some(max_candidates),
        Some(max_source_items),
        Some(max_source_links),
    )
}

fn candidate_input_hash(snapshot: &InventorySnapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    for item in &snapshot.items {
        item.id.hash(&mut hasher);
        item.kind.hash(&mut hasher);
        item.name.hash(&mut hasher);
        item.source.hash(&mut hasher);
        item.path.hash(&mut hasher);
    }
    for link in snapshot.links.iter().filter(|link| {
        matches!(
            link.kind.as_str(),
            "code_db_text_reference"
                | "code_db_column_text_reference"
                | "code_db_read"
                | "code_db_write"
                | "code_db_uses_column"
        )
    }) {
        link.id.hash(&mut hasher);
        link.from.hash(&mut hasher);
        link.to.hash(&mut hasher);
        link.kind.hash(&mut hasher);
        link.truth_class.hash(&mut hasher);
        for evidence in &link.evidence {
            evidence.kind.hash(&mut hasher);
            evidence.text.hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub(super) fn invalidate_candidate_links(workspace_id: &str) {
    if let Some(cache) = CANDIDATE_CACHE.get() {
        if let Ok(mut cache) = cache.lock() {
            cache.retain(|key, _| key.workspace_id != workspace_id);
        }
    }
}

fn compute_candidate_links(
    snapshot: &InventorySnapshot,
    code_ids: Option<&HashSet<String>>,
    max_candidates: Option<usize>,
    max_source_items: Option<usize>,
    max_source_links: Option<usize>,
) -> (Vec<CandidateLink>, bool) {
    let item_limit = max_source_items.unwrap_or(usize::MAX);
    let link_limit = max_source_links.unwrap_or(usize::MAX);
    let tables = snapshot
        .items
        .iter()
        .take(item_limit)
        .filter(|item| item.kind == "table")
        .map(|table| TableTerms {
            table,
            aliases: table_aliases(&table.name),
        })
        .collect::<Vec<_>>();
    let mut tables_by_alias = HashMap::<&str, Vec<usize>>::new();
    for (index, table) in tables.iter().enumerate() {
        for alias in &table.aliases {
            tables_by_alias.entry(alias).or_default().push(index);
        }
    }

    let mut links = Vec::new();
    let mut truncated = snapshot.items.len() > item_limit || snapshot.links.len() > link_limit;
    for link in snapshot.links.iter().take(link_limit).filter(|link| {
        link.truth_class == "candidate"
            && matches!(
                link.kind.as_str(),
                "code_db_text_reference" | "code_db_column_text_reference"
            )
            && code_ids.is_none_or(|ids| ids.contains(&link.from))
    }) {
        if max_candidates.is_some_and(|limit| links.len() == limit) {
            truncated = true;
            break;
        }
        links.push(CandidateLink {
            id: format!("candidate:{}->{}", link.from, link.to),
            from: link.from.clone(),
            to: link.to.clone(),
            confidence: explicit_confidence(&link.evidence).to_string(),
            evidence: link.evidence.clone(),
        });
    }

    'code_items: for code in snapshot
        .items
        .iter()
        .take(item_limit)
        .filter(|item| item.is_code())
        .filter(|item| code_ids.is_none_or(|ids| ids.contains(&item.id)))
    {
        let name_terms = identifier_terms(&code.name);
        let path = code.path.as_deref().unwrap_or_default();
        let path_terms = identifier_terms(path);
        let role_terms = identifier_tokens(&format!("{} {}", code.kind, code.name));
        let mut table_indexes = BTreeSet::new();
        for term in name_terms.iter().chain(path_terms.iter()) {
            if let Some(indexes) = tables_by_alias.get(term.as_str()) {
                table_indexes.extend(indexes.iter().copied());
            }
        }

        let mut ranked = table_indexes
            .into_iter()
            .filter_map(|index| {
                rank_candidate(
                    code,
                    &tables[index],
                    &name_terms,
                    &path_terms,
                    &role_terms,
                    path,
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.table.id.cmp(&right.table.id))
        });

        for candidate in ranked.into_iter().take(MAX_CANDIDATES_PER_CODE_ITEM) {
            if max_candidates.is_some_and(|limit| links.len() == limit) {
                truncated = true;
                break 'code_items;
            }
            links.push(CandidateLink {
                id: format!("candidate:{}->{}", code.id, candidate.table.id),
                from: code.id.clone(),
                to: candidate.table.id.clone(),
                confidence: confidence(candidate.score).to_string(),
                evidence: candidate.evidence,
            });
        }
    }

    let confirmed_pairs = snapshot
        .links
        .iter()
        .take(link_limit)
        .filter(|link| link.is_confirmed())
        .filter_map(|link| match link.kind.as_str() {
            "code_db_read" | "code_db_write" | "code_db_uses_column" => {
                Some((link.from.as_str(), link.to.as_str()))
            }
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut links = merge_candidate_links(links);
    links.retain(|link| !confirmed_pairs.contains(&(link.from.as_str(), link.to.as_str())));
    (links, truncated)
}

fn merge_candidate_links(links: Vec<CandidateLink>) -> Vec<CandidateLink> {
    let mut merged = BTreeMap::<(String, String), CandidateLink>::new();
    for link in links {
        let key = (link.from.clone(), link.to.clone());
        match merged.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(link);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                merge_evidence(&mut existing.evidence, link.evidence);
                existing.confidence = merged_confidence(
                    existing.confidence.as_str(),
                    link.confidence.as_str(),
                    &existing.evidence,
                )
                .to_string();
            }
        }
    }

    let mut links = merged.into_values().collect::<Vec<_>>();
    links.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| {
                candidate_confidence_rank(&left.confidence)
                    .cmp(&candidate_confidence_rank(&right.confidence))
            })
            .then_with(|| {
                has_explicit_evidence(&right.evidence).cmp(&has_explicit_evidence(&left.evidence))
            })
            .then_with(|| left.to.cmp(&right.to))
    });
    let mut counts = HashMap::<String, usize>::new();
    links.retain(|link| {
        let count = counts.entry(link.from.clone()).or_default();
        *count += 1;
        *count <= MAX_CANDIDATES_PER_CODE_ITEM
    });
    links.sort_by(|left, right| left.id.cmp(&right.id));
    links
}

fn explicit_confidence(evidence: &[Evidence]) -> &'static str {
    if schema_ambiguous(evidence) {
        "medium"
    } else {
        "high"
    }
}

fn merged_confidence<'a>(left: &'a str, right: &'a str, evidence: &[Evidence]) -> &'a str {
    if schema_ambiguous(evidence) {
        return "medium";
    }
    if candidate_confidence_rank(left) <= candidate_confidence_rank(right) {
        left
    } else {
        right
    }
}

fn candidate_confidence_rank(confidence: &str) -> u8 {
    match confidence {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

fn schema_ambiguous(evidence: &[Evidence]) -> bool {
    evidence
        .iter()
        .any(|entry| entry.kind == "code-search-schema-ambiguous")
}

fn has_explicit_evidence(evidence: &[Evidence]) -> bool {
    evidence
        .iter()
        .any(|entry| entry.kind == "code-search-exact-token")
}

pub(crate) fn merge_evidence(target: &mut Vec<Evidence>, source: Vec<Evidence>) {
    for evidence in source {
        if !target
            .iter()
            .any(|entry| entry.kind == evidence.kind && entry.text == evidence.text)
        {
            target.push(evidence);
        }
    }
}

fn rank_candidate<'a>(
    code: &InventoryItem,
    table: &'a TableTerms<'a>,
    name_terms: &HashSet<String>,
    path_terms: &HashSet<String>,
    role_terms: &[String],
    path: &str,
) -> Option<RankedCandidate<'a>> {
    let name_alias = strongest_alias(&table.aliases, name_terms);
    let path_alias = strongest_alias(&table.aliases, path_terms);
    let matched_alias = name_alias.or(path_alias)?;
    if is_generic_term(matched_alias) {
        return None;
    }

    let role = code_role(role_terms);
    let migration = is_migration_path(path);
    let mut evidence = Vec::new();
    let mut score = 0;

    if migration && path_alias.is_some() {
        score += 100;
        evidence.push(Evidence {
            kind: "migration-reference".to_string(),
            text: format!(
                "마이그레이션/DDL 경로가 {} 테이블 식별자를 포함합니다 ({})",
                table.table.name,
                compact_path(path)
            ),
        });
    }
    if let Some(alias) = name_alias {
        let contribution = if role.is_some() { 90 } else { 65 };
        score += contribution;
        evidence.push(Evidence {
            kind: if role.is_some() {
                "repository-name".to_string()
            } else {
                "table-token".to_string()
            },
            text: match role {
                Some(role) => format!(
                    "{} {} 이름의 '{}' 토큰이 {} 테이블과 일치합니다",
                    code.name, role, alias, table.table.name
                ),
                None => format!(
                    "{} 이름의 '{}' 토큰이 {} 테이블과 일치합니다",
                    code.name, alias, table.table.name
                ),
            },
        });
    }
    if name_alias.is_none() && path_alias.is_some() {
        score += 45;
        evidence.push(Evidence {
            kind: "path-token".to_string(),
            text: format!(
                "소스 경로의 '{}' 토큰이 {} 테이블과 일치합니다 ({})",
                matched_alias,
                table.table.name,
                compact_path(path)
            ),
        });
    }

    Some(RankedCandidate {
        table: table.table,
        score,
        evidence,
    })
}

pub(super) fn table_aliases(name: &str) -> Vec<String> {
    let tokens = identifier_tokens(name);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut aliases = BTreeSet::new();
    aliases.insert(tokens.join("_"));
    let mut singular = tokens;
    if let Some(last) = singular.last_mut() {
        *last = singularize(last);
    }
    aliases.insert(singular.join("_"));
    aliases
        .into_iter()
        .filter(|alias| !alias.is_empty())
        .collect()
}

pub(super) fn identifier_terms(value: &str) -> HashSet<String> {
    let tokens = identifier_tokens(value);
    let mut terms = HashSet::new();
    for start in 0..tokens.len() {
        for width in 1..=4.min(tokens.len() - start) {
            terms.insert(tokens[start..start + width].join("_"));
        }
    }
    terms
}

pub(super) fn identifier_tokens(value: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();

    for (index, character) in chars.iter().copied().enumerate() {
        if !character.is_alphanumeric() {
            push_token(&mut tokens, &mut current);
            continue;
        }
        let previous = index.checked_sub(1).and_then(|offset| chars.get(offset));
        let next = chars.get(index + 1);
        let camel_boundary = character.is_uppercase()
            && previous.is_some_and(|previous| previous.is_lowercase() || previous.is_numeric());
        let acronym_boundary = character.is_uppercase()
            && previous.is_some_and(|previous| previous.is_uppercase())
            && next.is_some_and(|next| next.is_lowercase());
        if (camel_boundary || acronym_boundary) && !current.is_empty() {
            push_token(&mut tokens, &mut current);
        }
        current.extend(character.to_lowercase());
    }
    push_token(&mut tokens, &mut current);
    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        tokens.push(std::mem::take(current));
    }
}

fn singularize(value: &str) -> String {
    if value.len() > 3 && value.ends_with("ies") {
        return format!("{}y", &value[..value.len() - 3]);
    }
    for suffix in ["ches", "shes", "xes", "zes", "ses"] {
        if value.len() > suffix.len() && value.ends_with(suffix) {
            return value[..value.len() - 2].to_string();
        }
    }
    if value.len() > 2
        && value.ends_with('s')
        && !value.ends_with("ss")
        && !value.ends_with("us")
        && !value.ends_with("is")
    {
        return value[..value.len() - 1].to_string();
    }
    value.to_string()
}

fn strongest_alias<'a>(aliases: &'a [String], terms: &HashSet<String>) -> Option<&'a str> {
    aliases
        .iter()
        .filter(|alias| terms.contains(alias.as_str()))
        .max_by_key(|alias| alias.len())
        .map(String::as_str)
}

fn code_role(tokens: &[String]) -> Option<&'static str> {
    if tokens
        .iter()
        .any(|token| token == "repository" || token == "repo")
    {
        Some("repository")
    } else if tokens.iter().any(|token| token == "dao") {
        Some("DAO")
    } else if tokens
        .iter()
        .any(|token| matches!(token.as_str(), "query" | "mapper" | "model" | "entity"))
    {
        Some("query/model")
    } else if tokens.iter().any(|token| token == "service") {
        Some("service")
    } else {
        None
    }
}

fn is_migration_path(path: &str) -> bool {
    identifier_tokens(path).iter().any(|token| {
        matches!(
            token.as_str(),
            "ddl" | "migration" | "migrations" | "schema" | "schemas"
        )
    })
}

fn is_generic_term(value: &str) -> bool {
    !value.contains('_') && GENERIC_DB_TERMS.contains(&value)
}

fn confidence(score: u16) -> &'static str {
    if score >= 85 {
        "high"
    } else if score >= 45 {
        "medium"
    } else {
        "low"
    }
}

fn compact_path(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
#[cfg(test)]
include!("linker_tests.rs");
