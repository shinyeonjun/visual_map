//! Python 파일·import alias를 route 객체의 안정적인 심볼로 연결한다.

use crate::facts::{BindingKind, FactStore};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct RouteSymbol {
    pub(crate) key: String,
    pub(crate) file_id: String,
    pub(crate) local_name: String,
}

/// 같은 파일의 local 이름과 교차 파일 import alias를 route 심볼로 해석한다.
#[derive(Debug)]
pub(crate) struct SymbolIndex {
    modules_by_file: HashMap<String, Vec<String>>,
    imports_by_scope: HashMap<(String, String), Vec<String>>,
}

impl SymbolIndex {
    pub(crate) fn new(facts: &FactStore) -> Self {
        let modules_by_file = facts
            .units
            .values()
            .filter(|unit| unit.kind == crate::facts::CodeUnitKind::File)
            .map(|unit| (unit.file_id.clone(), module_aliases(&unit.relative_path)))
            .collect();

        let mut imports_by_scope: HashMap<(String, String), Vec<String>> = HashMap::new();
        for binding in &facts.bindings {
            if !matches!(binding.kind, BindingKind::Import | BindingKind::ImportAlias) {
                continue;
            }
            let Some(unit) = facts.unit(&binding.source_unit_id) else {
                continue;
            };
            imports_by_scope
                .entry((unit.file_id.clone(), binding.local_name.clone()))
                .or_default()
                .push(normalize_name(&binding.target_name));
        }

        Self {
            modules_by_file,
            imports_by_scope,
        }
    }

    pub(crate) fn route_symbol(&self, file_id: &str, local_name: &str) -> RouteSymbol {
        RouteSymbol {
            key: format!("{file_id}::{local_name}"),
            file_id: file_id.to_string(),
            local_name: local_name.to_string(),
        }
    }

    pub(crate) fn imported_alias_matches(
        &self,
        file_id: &str,
        local_name: &str,
        candidates: &[&str],
    ) -> bool {
        self.imports_by_scope
            .get(&(file_id.to_string(), local_name.to_string()))
            .is_some_and(|targets| {
                targets.iter().any(|target| {
                    let imported_name = target.rsplit('.').next().unwrap_or(target);
                    candidates.contains(&imported_name)
                })
            })
    }

    /// 호출 파일의 이름을 특정 route 심볼 후보로 해석한다.
    pub(crate) fn resolve(
        &self,
        file_id: &str,
        local_name: &str,
        symbols: &[RouteSymbol],
    ) -> Option<String> {
        let local_name = local_name.trim();
        if local_name.is_empty() {
            return None;
        }

        let direct_key = format!("{file_id}::{local_name}");
        if symbols.iter().any(|symbol| symbol.key == direct_key) {
            return Some(direct_key);
        }

        let mut requested = self
            .imports_by_scope
            .get(&(file_id.to_string(), local_name.to_string()))
            .cloned()
            .unwrap_or_default();
        requested.push(normalize_name(local_name));

        let matches = symbols
            .iter()
            .filter(|symbol| {
                let aliases = self.symbol_aliases(&symbol.file_id, &symbol.local_name);
                requested.iter().any(|name| aliases.contains(name))
            })
            .map(|symbol| symbol.key.clone())
            .collect::<HashSet<_>>();
        if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        }
    }

    fn symbol_aliases(&self, file_id: &str, local_name: &str) -> HashSet<String> {
        let mut aliases = self
            .modules_by_file
            .get(file_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|module| format!("{module}.{local_name}"))
            .collect::<HashSet<_>>();
        aliases.insert(normalize_name(local_name));
        aliases
    }
}

fn module_aliases(relative_path: &str) -> Vec<String> {
    let normalized = relative_path.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut module = path
        .with_extension("")
        .to_string_lossy()
        .replace('/', ".")
        .trim_matches('.')
        .to_string();
    if module.ends_with(".__init__") {
        module.truncate(module.len() - ".__init__".len());
    }

    let parts = module.split('.').collect::<Vec<_>>();
    let mut aliases = Vec::new();
    for start in 0..parts.len() {
        let alias = parts[start..].join(".");
        if !alias.is_empty() {
            aliases.push(alias);
        }
    }
    aliases
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('.')
        .replace("::", ".")
        .replace('\\', ".")
}
