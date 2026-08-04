use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CollectionMode {
    Passive,
    ToolAssisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CollectionStatus {
    Collected,
    Partial,
    NotDetected,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TruthClass {
    Confirmed,
}

#[derive(Serialize)]
pub(crate) struct CollectionReport {
    pub(crate) schema: &'static str,
    pub(crate) project_root: String,
    pub(crate) collectors: Vec<CollectorSummary>,
    pub(crate) facts: Vec<CollectedFact>,
    pub(crate) relations: Vec<CollectedRelation>,
    pub(crate) diagnostics: Vec<CollectionDiagnostic>,
}

impl CollectionReport {
    pub(crate) fn new(project_root: String) -> Self {
        Self {
            schema: "code-memory.collection-report.v1",
            project_root,
            collectors: Vec::new(),
            facts: Vec::new(),
            relations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, mut result: CollectorResult) {
        result.summary.fact_count = result.facts.len();
        result.summary.relation_count = result.relations.len();
        result.summary.diagnostic_count = result.diagnostics.len();
        self.collectors.push(result.summary);
        self.facts.append(&mut result.facts);
        self.relations.append(&mut result.relations);
        self.diagnostics.append(&mut result.diagnostics);
    }

    pub(crate) fn canonicalize(&mut self) {
        self.collectors.sort_by(|left, right| left.id.cmp(right.id));
        self.facts.sort_by(|left, right| {
            (&left.stable_key, &left.kind, &left.path).cmp(&(
                &right.stable_key,
                &right.kind,
                &right.path,
            ))
        });
        self.facts
            .dedup_by(|left, right| left.stable_key == right.stable_key);
        for relation in &mut self.relations {
            relation.evidence.sort_by(|left, right| {
                (&left.path, left.line, &left.note).cmp(&(&right.path, right.line, &right.note))
            });
            relation.evidence.dedup();
        }
        self.relations.sort_by(|left, right| {
            (&left.from, &left.to, &left.kind, &left.evidence).cmp(&(
                &right.from,
                &right.to,
                &right.kind,
                &right.evidence,
            ))
        });
        self.relations.dedup_by(|left, right| {
            left.from == right.from
                && left.to == right.to
                && left.kind == right.kind
                && left.evidence == right.evidence
        });
        self.diagnostics.sort_by(|left, right| {
            (&left.collector, &left.code, &left.path, &left.message).cmp(&(
                &right.collector,
                &right.code,
                &right.path,
                &right.message,
            ))
        });
    }
}

pub(crate) struct CollectorResult {
    pub(crate) summary: CollectorSummary,
    pub(crate) facts: Vec<CollectedFact>,
    pub(crate) relations: Vec<CollectedRelation>,
    pub(crate) diagnostics: Vec<CollectionDiagnostic>,
}

impl CollectorResult {
    pub(crate) fn new(id: &'static str, capability: &'static str, mode: CollectionMode) -> Self {
        Self {
            summary: CollectorSummary {
                id,
                capability,
                mode,
                status: CollectionStatus::NotDetected,
                detected_by: Vec::new(),
                tool: None,
                tool_origin: None,
                tool_version: None,
                fact_count: 0,
                relation_count: 0,
                diagnostic_count: 0,
            },
            facts: Vec::new(),
            relations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct CollectorSummary {
    pub(crate) id: &'static str,
    pub(crate) capability: &'static str,
    pub(crate) mode: CollectionMode,
    pub(crate) status: CollectionStatus,
    pub(crate) detected_by: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_origin: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_version: Option<String>,
    pub(crate) fact_count: usize,
    pub(crate) relation_count: usize,
    pub(crate) diagnostic_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CollectedFact {
    pub(crate) stable_key: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CollectedRelation {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) truth_class: TruthClass,
    pub(crate) evidence_type: String,
    pub(crate) evidence: Vec<CollectedEvidence>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct CollectedEvidence {
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CollectionDiagnostic {
    pub(crate) collector: &'static str,
    pub(crate) level: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
}

pub(crate) fn properties(values: &[(&str, Option<&str>)]) -> BTreeMap<String, String> {
    values
        .iter()
        .filter_map(|(key, value)| value.map(|value| ((*key).to_string(), value.to_string())))
        .collect()
}
