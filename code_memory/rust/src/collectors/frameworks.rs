use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use super::model::{
    CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic, CollectionMode,
    CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "frameworks";

pub(crate) fn collect(
    root: &Path,
    pack_root: &Path,
    snapshot: &crate::SourceSnapshot,
) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "framework-semantics", CollectionMode::Passive);
    let analysis = match crate::frameworks::analyze_with_sources(root, &[], pack_root, snapshot) {
        Ok(analysis) => analysis,
        Err(message) => {
            result.summary.status = CollectionStatus::Failed;
            result.diagnostics.push(CollectionDiagnostic {
                collector: ID,
                level: "error",
                code: "framework-analysis-failed",
                message,
                path: None,
            });
            return result;
        }
    };
    if analysis.frameworks.is_empty() {
        return result;
    }

    let mut fact_keys = HashMap::new();
    for framework in analysis
        .frameworks
        .iter()
        .filter(|framework| !framework.facts.is_empty())
    {
        result.summary.detected_by.push(framework.id.clone());
        let framework_key = format!("framework:{}", framework.id);
        result.facts.push(CollectedFact {
            stable_key: framework_key.clone(),
            kind: "framework".to_string(),
            name: framework.name.clone(),
            path: framework.files.first().cloned(),
            properties: map_properties(&[
                ("language", &framework.language),
                ("framework_kind", &framework.kind),
                ("adapter", &framework.adapter),
                ("status", &framework.status),
            ]),
        });
        for fact in &framework.facts {
            let key = format!("framework-fact:{}", fact.id);
            fact_keys.insert(fact.id.clone(), key.clone());
            let mut properties = fact.properties.clone();
            properties.insert("framework".to_string(), framework.id.clone());
            if let Some(method) = &fact.method {
                properties.insert("method".to_string(), method.clone());
            }
            if let Some(route) = &fact.path {
                properties.insert("route".to_string(), route.clone());
            }
            if let Some(symbol) = &fact.symbol {
                properties.insert("symbol".to_string(), symbol.clone());
            }
            result.facts.push(CollectedFact {
                stable_key: key.clone(),
                kind: fact.kind.to_ascii_lowercase().replace('_', "-"),
                name: fact
                    .path
                    .as_deref()
                    .or(fact.symbol.as_deref())
                    .unwrap_or(&fact.kind)
                    .to_string(),
                path: Some(fact.source_file.clone()),
                properties,
            });
            result.relations.push(relation(
                framework_key.clone(),
                key,
                "DECLARES",
                &fact.source_file,
                Some(fact.source_line as u32),
                fact.evidence.first().cloned(),
            ));
        }
    }

    if fact_keys.is_empty() {
        return CollectorResult::new(ID, "framework-semantics", CollectionMode::Passive);
    }

    let mut symbols = HashSet::new();
    for relation_source in &analysis.relations {
        let Some(target) = fact_keys.get(&relation_source.to).cloned() else {
            result.diagnostics.push(CollectionDiagnostic {
                collector: ID,
                level: "warning",
                code: "missing-framework-target",
                message: format!(
                    "{} relation target {} was not emitted",
                    relation_source.kind, relation_source.to
                ),
                path: Some(relation_source.path.clone()),
            });
            continue;
        };
        let source = format!("code-symbol:{}", relation_source.from);
        if symbols.insert(source.clone()) {
            result.facts.push(CollectedFact {
                stable_key: source.clone(),
                kind: "code-symbol-reference".to_string(),
                name: relation_source.from.clone(),
                path: Some(relation_source.path.clone()),
                properties: map_properties(&[("framework", &relation_source.framework)]),
            });
        }
        result.relations.push(relation(
            source,
            target,
            &relation_source.kind,
            &relation_source.path,
            relation_source
                .range
                .first()
                .and_then(|line| u32::try_from(*line).ok())
                .map(|line| line + 1),
            relation_source.evidence.first().cloned(),
        ));
    }

    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn map_properties(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

fn relation(
    from: String,
    to: String,
    kind: &str,
    path: &str,
    line: Option<u32>,
    note: Option<String>,
) -> CollectedRelation {
    CollectedRelation {
        from,
        to,
        kind: kind.to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "framework-source".to_string(),
        evidence: vec![CollectedEvidence {
            path: path.to_string(),
            line,
            note,
        }],
        properties: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn framework_facts_are_self_contained() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-framework-collector-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(
            root.join("lib/main.dart"),
            "import 'package:shelf/shelf.dart';\nfinal router = Router();\nrouter.get('/health', handler);\n",
        )
        .unwrap();
        let pack_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

        let snapshot = crate::source::load_source_snapshot(&root);
        let result = collect(&root, pack_root, &snapshot);
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "http-route" && fact.name == "/health"));
        let keys: HashSet<_> = result
            .facts
            .iter()
            .map(|fact| fact.stable_key.as_str())
            .collect();
        assert!(result
            .relations
            .iter()
            .all(|relation| keys.contains(relation.from.as_str())
                && keys.contains(relation.to.as_str())));
        let _ = std::fs::remove_dir_all(root);
    }
}
