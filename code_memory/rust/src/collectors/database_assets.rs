use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::discovery::{find_files, relative_path, stable_segment};
use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionMode,
    CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "database-assets";

pub(crate) fn collect(root: &Path) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "orm-and-migrations", CollectionMode::Passive);
    let files = find_files(root, |path| classify(path).is_some());
    if files.is_empty() {
        return result;
    }

    let root_key = "database-assets:root".to_string();
    result.facts.push(CollectedFact {
        stable_key: root_key.clone(),
        kind: "database-assets".to_string(),
        name: "Database assets".to_string(),
        path: Some(".".to_string()),
        properties: BTreeMap::new(),
    });
    let mut groups: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for file in files {
        let path = relative_path(root, &file);
        let Some(asset) = classify(&file) else {
            continue;
        };
        result.summary.detected_by.push(path.clone());
        let key = format!("database-asset:{}", stable_segment(&path));
        result.facts.push(CollectedFact {
            stable_key: key.clone(),
            kind: asset.kind.to_string(),
            name: file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&path)
                .to_string(),
            path: Some(path.clone()),
            properties: properties(&[
                ("framework", Some(asset.framework)),
                ("sequence", sequence(&file).as_deref()),
                ("source_scope", source_scope(&path)),
            ]),
        });
        if asset.kind == "migration" {
            let directory = file
                .parent()
                .map(|parent| relative_path(root, parent))
                .unwrap_or_default();
            groups
                .entry((asset.framework.to_string(), directory))
                .or_default()
                .push((path, key));
        } else {
            result
                .relations
                .push(relation(&root_key, &key, "CONTAINS", &path));
        }
    }

    for ((framework, directory), mut migrations) in groups {
        migrations.sort_by(|left, right| left.0.cmp(&right.0));
        let group_key = format!(
            "migration-set:{}:{}",
            stable_segment(&framework),
            stable_segment(&directory)
        );
        result.facts.push(CollectedFact {
            stable_key: group_key.clone(),
            kind: "migration-set".to_string(),
            name: framework.clone(),
            path: Some(directory.clone()),
            properties: properties(&[("framework", Some(&framework))]),
        });
        result
            .relations
            .push(relation(&root_key, &group_key, "CONTAINS", &directory));
        for (index, (path, key)) in migrations.iter().enumerate() {
            result
                .relations
                .push(relation(&group_key, key, "CONTAINS", path));
            if let Some((next_path, next_key)) = migrations.get(index + 1) {
                result
                    .relations
                    .push(relation(key, next_key, "PRECEDES", next_path));
            }
        }
    }

    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = CollectionStatus::Collected;
    result
}

#[derive(Clone, Copy)]
struct Asset {
    kind: &'static str,
    framework: &'static str,
}

fn classify(path: &Path) -> Option<Asset> {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "schema.prisma" {
        return Some(Asset {
            kind: "orm-schema",
            framework: "prisma",
        });
    }
    if name == "ormconfig.json" || name.starts_with("sequelize.config.") {
        return Some(Asset {
            kind: "orm-config",
            framework: if name == "ormconfig.json" {
                "typeorm"
            } else {
                "sequelize"
            },
        });
    }
    if name.contains("changelog")
        && matches!(extension.as_str(), "xml" | "yaml" | "yml" | "json" | "sql")
    {
        return Some(Asset {
            kind: "migration",
            framework: "liquibase",
        });
    }
    let migration_path = normalized.contains("/migrations/")
        || normalized.contains("/migrate/")
        || normalized.contains("/db/migration/")
        || normalized.contains("/versions/");
    if !migration_path || !matches!(extension.as_str(), "sql" | "py" | "cs" | "js" | "ts") {
        return None;
    }
    let framework = if normalized.contains("/prisma/migrations/") {
        "prisma"
    } else if normalized.contains("/db/migration/") && extension == "sql" {
        "flyway"
    } else if normalized.contains("/versions/") && extension == "py" {
        "alembic"
    } else if extension == "cs" {
        "entity-framework"
    } else if extension == "py" {
        "django"
    } else if matches!(extension.as_str(), "js" | "ts") {
        "javascript-orm"
    } else {
        "sql"
    };
    Some(Asset {
        kind: "migration",
        framework,
    })
}

fn sequence(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let prefix: String = stem
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    (!prefix.is_empty()).then_some(prefix)
}

fn source_scope(path: &str) -> Option<&'static str> {
    path.split('/')
        .any(|segment| {
            matches!(
                segment.to_ascii_lowercase().as_str(),
                "test" | "tests" | "fixture" | "fixtures" | "example" | "examples"
            )
        })
        .then_some("test")
}

fn relation(from: &str, to: &str, kind: &str, path: &str) -> CollectedRelation {
    CollectedRelation {
        from: from.to_string(),
        to: to.to_string(),
        kind: kind.to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "DATABASE_ASSET".to_string(),
        evidence: vec![CollectedEvidence {
            path: path.to_string(),
            line: None,
            note: None,
        }],
        properties: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn migration_sets_keep_order_and_scope() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-database-assets-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("app/migrations")).unwrap();
        std::fs::create_dir_all(root.join("prisma")).unwrap();
        std::fs::write(root.join("app/migrations/0001_initial.py"), "").unwrap();
        std::fs::write(root.join("app/migrations/0002_user.py"), "").unwrap();
        std::fs::write(root.join("prisma/schema.prisma"), "").unwrap();

        let result = collect(&root);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "migration")
                .count(),
            2
        );
        assert!(result
            .relations
            .iter()
            .any(|relation| relation.kind == "PRECEDES"));
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "orm-schema" && fact.name == "schema.prisma"));
        let _ = std::fs::remove_dir_all(root);
    }
}
