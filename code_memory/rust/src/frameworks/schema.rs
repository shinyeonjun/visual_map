use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub(crate) struct FrameworkPack {
    pub(crate) id: String,
    pub(crate) language: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) signals: Vec<String>,
    pub(crate) outputs: Vec<String>,
    pub(crate) rules: Vec<String>,
    pub(crate) adapter: String,
    pub(crate) fixture: FrameworkFixture,
}

#[derive(Clone, Default)]
pub(crate) struct FrameworkFixture {
    pub(crate) files: Vec<FrameworkFixtureFile>,
    pub(crate) expected_facts: Vec<String>,
    pub(crate) expected_relations: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct FrameworkFixtureFile {
    pub(crate) path: String,
    pub(crate) source: String,
}

#[derive(Default)]
pub(crate) struct FastApiRouteContext {
    pub(crate) prefixes: HashMap<(String, String), String>,
    pub(crate) minimal_prefixes: HashMap<String, String>,
}

impl FastApiRouteContext {
    pub(crate) fn prefix_for(&self, path: &str, line: &str) -> Option<&str> {
        let receiver = line.trim_start().strip_prefix('@')?.split_once('.')?.0;
        if receiver == "app" {
            return None;
        }
        self.prefixes
            .get(&(path.to_string(), receiver.to_string()))
            .map(String::as_str)
    }

    pub(crate) fn minimal_prefix_for(&self, path: &str) -> Option<&str> {
        self.minimal_prefixes.get(path).map(String::as_str)
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FrameworkOutput {
    pub(crate) id: String,
    pub(crate) language: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) adapter: String,
    pub(crate) status: String,
    pub(crate) matched_signals: Vec<String>,
    pub(crate) files: Vec<String>,
    pub(crate) facts: Vec<FrameworkFact>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FrameworkFact {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) framework: String,
    pub(crate) symbol: Option<String>,
    pub(crate) method: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) source_file: String,
    pub(crate) source_line: usize,
    pub(crate) source_end_line: usize,
    pub(crate) source_range: Vec<i32>,
    pub(crate) evidence: Vec<String>,
    pub(crate) properties: BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct FrameworkRelation {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) framework: String,
    pub(crate) path: String,
    pub(crate) range: Vec<i32>,
    pub(crate) evidence: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Analysis {
    pub(crate) frameworks: Vec<FrameworkOutput>,
    pub(crate) relations: Vec<FrameworkRelation>,
}
