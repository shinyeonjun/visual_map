//! Converts the canonical [`AnalysisPlan`] into restartable provider work.
//!
//! The plan owns semantic file membership. Provider-specific sharding may
//! split one analysis unit for reliability, but it may never move a file to a
//! different unit or silently omit it.

use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::validation::Validate;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::project_model::ProjectModelUnit;
use crate::{active_c_family_files, active_csharp_files, LanguageSpec, LANGUAGES};

const MAX_DART_FILES_PER_PROVIDER: usize = 512;
const RECEIPT_SAMPLE_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub(crate) struct ScheduledProviderUnit {
    /// Canonical analysis-unit identity. Several execution shards may share it.
    pub(crate) analysis_unit_id: String,
    /// Provider grouping key. C/C++ and TS/JS counterparts intentionally share it.
    pub(crate) execution_scope_id: String,
    pub(crate) lang: LanguageSpec,
    pub(crate) root: PathBuf,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) provider_config: Option<PathBuf>,
    pub(crate) project_excluded_files: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderSchedule {
    pub(crate) units: Vec<ScheduledProviderUnit>,
    pub(crate) receipt: ProviderScheduleReceipt,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderScheduleReceipt {
    pub(crate) schema: &'static str,
    pub(crate) analysis_plan_digest: String,
    pub(crate) analysis_unit_count: usize,
    pub(crate) scheduled_analysis_unit_count: usize,
    pub(crate) execution_shard_count: usize,
    pub(crate) planned_file_language_count: usize,
    pub(crate) scheduled_file_language_count: usize,
    pub(crate) project_model_only_count: usize,
    pub(crate) project_config_excluded_count: usize,
    pub(crate) omission_sample: Vec<String>,
    pub(crate) details_truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OmissionReason {
    ProjectModelOnly,
    ProjectConfigExcluded,
}

impl OmissionReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProjectModelOnly => "project-model-only",
            Self::ProjectConfigExcluded => "project-config-excluded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Omission {
    language: ProgrammingLanguage,
    path: RepositoryPath,
    reason: OmissionReason,
}

/// Builds the only provider schedule accepted by the static pipeline.
///
/// Every provider file comes from one `AnalysisPlan` assignment. Files that
/// cannot be sent to a semantic provider are retained as explicit omissions:
/// Vue SFCs are measured by the TypeScript project model, while inactive C/C++
/// translation units are reported as project-config exclusions.
pub(crate) fn schedule_provider_units(
    project_root: &Path,
    plan: &AnalysisPlan,
    typescript_units: &[ProjectModelUnit],
) -> Result<ProviderSchedule, String> {
    plan.validate()
        .map_err(|error| format!("cannot schedule an invalid AnalysisPlan: {error}"))?;

    let mut files_by_unit = BTreeMap::<String, Vec<RepositoryPath>>::new();
    for assignment in &plan.assignments {
        if assignment.unit_ids.len() != 1 {
            return Err(format!(
                "provider scheduling requires one owner per file/language: {} has {}",
                assignment.path,
                assignment.unit_ids.len()
            ));
        }
        files_by_unit
            .entry(assignment.unit_ids[0].as_str().to_string())
            .or_default()
            .push(assignment.path.clone());
    }

    let mut scheduled = Vec::new();
    let mut scheduled_keys = BTreeSet::new();
    let mut omissions = BTreeSet::new();
    for unit in &plan.units {
        let lang = language_spec(unit.language)?;
        let root = absolute_repository_path(project_root, &unit.root);
        let relative_files = files_by_unit
            .get(unit.id.as_str())
            .cloned()
            .unwrap_or_default();
        let mut provider_files = Vec::new();
        for path in relative_files {
            if unit.language == ProgrammingLanguage::TypeScript && is_vue(&path) {
                omissions.insert(Omission {
                    language: unit.language,
                    path,
                    reason: OmissionReason::ProjectModelOnly,
                });
                continue;
            }
            provider_files.push((path.clone(), absolute_repository_path(project_root, &path)));
        }

        let mut project_excluded_files = 0;
        if matches!(
            unit.language,
            ProgrammingLanguage::C | ProgrammingLanguage::Cpp
        ) {
            let absolute = provider_files
                .iter()
                .map(|(_, path)| path.clone())
                .collect::<Vec<_>>();
            let (active, excluded) = active_c_family_files(&root, &absolute);
            let active = active.into_iter().collect::<HashSet<_>>();
            for (relative, absolute) in &provider_files {
                if !active.contains(absolute) {
                    omissions.insert(Omission {
                        language: unit.language,
                        path: relative.clone(),
                        reason: OmissionReason::ProjectConfigExcluded,
                    });
                }
            }
            provider_files.retain(|(_, path)| active.contains(path));
            project_excluded_files = excluded;
        }
        if unit.language == ProgrammingLanguage::CSharp {
            let absolute = provider_files
                .iter()
                .map(|(_, path)| path.clone())
                .collect::<Vec<_>>();
            let (active, excluded) = active_csharp_files(&root, &absolute);
            let active = active.into_iter().collect::<HashSet<_>>();
            for (relative, absolute) in &provider_files {
                if !active.contains(absolute) {
                    omissions.insert(Omission {
                        language: unit.language,
                        path: relative.clone(),
                        reason: OmissionReason::ProjectConfigExcluded,
                    });
                }
            }
            provider_files.retain(|(_, path)| active.contains(path));
            project_excluded_files = excluded;
        }

        if provider_files.is_empty() {
            continue;
        }
        let unit_id = unit.id.as_str().to_string();
        let root_scope = root_scope(&unit.root);
        let shards = if matches!(
            unit.language,
            ProgrammingLanguage::TypeScript | ProgrammingLanguage::JavaScript
        ) {
            typescript_shards(
                project_root,
                lang,
                &unit_id,
                &root,
                &root_scope,
                provider_files,
                typescript_units,
                project_excluded_files,
            )
        } else if unit.language == ProgrammingLanguage::Dart {
            chunked_shards(
                lang,
                &unit_id,
                &root,
                &format!("dart:{root_scope}"),
                provider_files,
                project_excluded_files,
            )
        } else {
            // C and C++ can share a physical header, but never a semantic
            // compile context. Keep their provider executions separate so a
            // C++ interpretation cannot overwrite the C contract (or vice
            // versa).
            let scope = format!("{}:{root_scope}", unit.language.as_str());
            vec![ScheduledProviderUnit {
                analysis_unit_id: unit_id,
                execution_scope_id: scope,
                lang,
                root,
                files: provider_files
                    .iter()
                    .map(|(_, path)| path.clone())
                    .collect(),
                provider_config: None,
                project_excluded_files,
            }]
        };

        for shard in shards {
            if shard.files.is_empty() {
                return Err(format!(
                    "provider schedule created an empty shard for {}",
                    shard.analysis_unit_id
                ));
            }
            for file in &shard.files {
                let relative = repository_path_from_absolute(project_root, file)?;
                let key = (
                    unit.language,
                    relative.clone(),
                    shard.analysis_unit_id.clone(),
                );
                if !scheduled_keys.insert(key) {
                    return Err(format!(
                        "provider schedule assigned {}:{} more than once",
                        unit.language.as_str(),
                        relative
                    ));
                }
            }
            scheduled.push(shard);
        }
    }

    let planned_keys = plan
        .assignments
        .iter()
        .map(|assignment| {
            (
                assignment.language,
                assignment.path.clone(),
                assignment.unit_ids[0].as_str().to_string(),
            )
        })
        .collect::<BTreeSet<_>>();
    let omitted_keys = omissions
        .iter()
        .filter_map(|omission| {
            plan.assignments
                .iter()
                .find(|assignment| {
                    assignment.language == omission.language && assignment.path == omission.path
                })
                .map(|assignment| {
                    (
                        omission.language,
                        omission.path.clone(),
                        assignment.unit_ids[0].as_str().to_string(),
                    )
                })
        })
        .collect::<BTreeSet<_>>();
    let accounted = scheduled_keys
        .union(&omitted_keys)
        .cloned()
        .collect::<BTreeSet<_>>();
    if accounted != planned_keys {
        let missing = planned_keys
            .difference(&accounted)
            .map(|(language, path, _)| format!("{}:{}", language.as_str(), path))
            .take(RECEIPT_SAMPLE_LIMIT)
            .collect::<Vec<_>>();
        let extra = accounted
            .difference(&planned_keys)
            .map(|(language, path, _)| format!("{}:{}", language.as_str(), path))
            .take(RECEIPT_SAMPLE_LIMIT)
            .collect::<Vec<_>>();
        return Err(format!(
            "provider schedule does not exactly account for AnalysisPlan assignments; missing=[{}] extra=[{}]",
            missing.join(", "),
            extra.join(", ")
        ));
    }

    scheduled.sort_by(|left, right| {
        (
            left.lang.id,
            left.analysis_unit_id.as_str(),
            left.execution_scope_id.as_str(),
            left.files.first(),
        )
            .cmp(&(
                right.lang.id,
                right.analysis_unit_id.as_str(),
                right.execution_scope_id.as_str(),
                right.files.first(),
            ))
    });
    let scheduled_analysis_unit_count = scheduled
        .iter()
        .map(|unit| unit.analysis_unit_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let project_model_only_count = omissions
        .iter()
        .filter(|item| item.reason == OmissionReason::ProjectModelOnly)
        .count();
    let project_config_excluded_count = omissions
        .iter()
        .filter(|item| item.reason == OmissionReason::ProjectConfigExcluded)
        .count();
    let omission_sample = omissions
        .iter()
        .take(RECEIPT_SAMPLE_LIMIT)
        .map(|item| {
            format!(
                "{}:{}:{}",
                item.language.as_str(),
                item.path,
                item.reason.as_str()
            )
        })
        .collect();
    let receipt = ProviderScheduleReceipt {
        schema: "codebase-workspace.provider-schedule.v1",
        analysis_plan_digest: plan.plan_digest.to_hex(),
        analysis_unit_count: plan.units.len(),
        scheduled_analysis_unit_count,
        execution_shard_count: scheduled.len(),
        planned_file_language_count: planned_keys.len(),
        scheduled_file_language_count: scheduled_keys.len(),
        project_model_only_count,
        project_config_excluded_count,
        omission_sample,
        details_truncated: omissions.len() > RECEIPT_SAMPLE_LIMIT,
    };
    Ok(ProviderSchedule {
        units: scheduled,
        receipt,
    })
}

#[allow(clippy::too_many_arguments)]
fn typescript_shards(
    project_root: &Path,
    lang: LanguageSpec,
    analysis_unit_id: &str,
    root: &Path,
    root_scope: &str,
    files: Vec<(RepositoryPath, PathBuf)>,
    project_units: &[ProjectModelUnit],
    project_excluded_files: usize,
) -> Vec<ScheduledProviderUnit> {
    let mut order = (0..project_units.len()).collect::<Vec<_>>();
    order.sort_by_key(|index| {
        let unit = &project_units[*index];
        let config_depth = unit
            .config
            .as_deref()
            .or(unit.base_config.as_deref())
            .map(|path| path.matches('/').count())
            .unwrap_or(0);
        (unit.synthetic, Reverse(config_depth), unit.id.as_str())
    });
    let mut remaining = files.into_iter().collect::<BTreeMap<_, _>>();
    let mut shards = Vec::new();
    for index in order {
        let project_unit = &project_units[index];
        let mut shard_files = Vec::new();
        for path in &project_unit.files {
            let Ok(path) = RepositoryPath::parse(path.replace('\\', "/")) else {
                continue;
            };
            if let Some(absolute) = remaining.remove(&path) {
                shard_files.push(absolute);
            }
        }
        if shard_files.is_empty() {
            continue;
        }
        shard_files.sort();
        shards.push(ScheduledProviderUnit {
            analysis_unit_id: analysis_unit_id.to_string(),
            execution_scope_id: format!("tsjs:{root_scope}:{}", sanitize_scope(&project_unit.id)),
            lang,
            root: root.to_path_buf(),
            files: shard_files,
            provider_config: project_unit.generated_config.clone().or_else(|| {
                project_unit
                    .config
                    .as_deref()
                    .map(|path| project_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)))
            }),
            project_excluded_files: if shards.is_empty() {
                project_excluded_files
            } else {
                0
            },
        });
    }
    if !remaining.is_empty() {
        let mut fallback_files = remaining.into_values().collect::<Vec<_>>();
        fallback_files.sort();
        shards.push(ScheduledProviderUnit {
            analysis_unit_id: analysis_unit_id.to_string(),
            execution_scope_id: format!("tsjs:{root_scope}:fallback"),
            lang,
            root: root.to_path_buf(),
            files: fallback_files,
            provider_config: None,
            project_excluded_files: if shards.is_empty() {
                project_excluded_files
            } else {
                0
            },
        });
    }
    shards
}

fn chunked_shards(
    lang: LanguageSpec,
    analysis_unit_id: &str,
    root: &Path,
    scope: &str,
    files: Vec<(RepositoryPath, PathBuf)>,
    project_excluded_files: usize,
) -> Vec<ScheduledProviderUnit> {
    files
        .chunks(MAX_DART_FILES_PER_PROVIDER)
        .enumerate()
        .map(|(index, chunk)| ScheduledProviderUnit {
            analysis_unit_id: analysis_unit_id.to_string(),
            execution_scope_id: format!("{scope}:shard-{index}"),
            lang,
            root: root.to_path_buf(),
            files: chunk.iter().map(|(_, path)| path.clone()).collect(),
            provider_config: None,
            project_excluded_files: if index == 0 {
                project_excluded_files
            } else {
                0
            },
        })
        .collect()
}

fn language_spec(language: ProgrammingLanguage) -> Result<LanguageSpec, String> {
    LANGUAGES
        .iter()
        .find(|candidate| candidate.contract_language == language)
        .copied()
        .ok_or_else(|| format!("no provider is registered for {}", language.as_str()))
}

fn absolute_repository_path(project_root: &Path, path: &RepositoryPath) -> PathBuf {
    if path.is_root() {
        project_root.to_path_buf()
    } else {
        path.as_str()
            .split('/')
            .fold(project_root.to_path_buf(), |root, segment| {
                root.join(segment)
            })
    }
}

fn repository_path_from_absolute(
    project_root: &Path,
    path: &Path,
) -> Result<RepositoryPath, String> {
    let relative = path.strip_prefix(project_root).map_err(|_| {
        format!(
            "provider schedule path escaped the selected project: {}",
            path.display()
        )
    })?;
    RepositoryPath::parse(relative.to_string_lossy().replace('\\', "/"))
        .map_err(|error| format!("provider schedule path is not canonical: {error}"))
}

fn root_scope(root: &RepositoryPath) -> String {
    if root.is_root() {
        "root".to_string()
    } else {
        sanitize_scope(root.as_str())
    }
}

fn sanitize_scope(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn is_vue(path: &RepositoryPath) -> bool {
    path.as_str().to_ascii_lowercase().ends_with(".vue")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::static_pipeline::analysis_unit_planner::plan_analysis_units;
    use crate::static_pipeline::source_census::SourceCensus;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn schedule_is_derived_from_plan_and_accounts_for_vue_without_sending_it() {
        let project = TestProject::new("authority");
        project.write("tsconfig.json", b"{}");
        project.write("src/app.ts", b"export const app = 1;\n");
        project.write("src/view.vue", b"<script>export default {}</script>\n");
        project.write("go/go.mod", b"module example.test/app\n");
        project.write("go/main.go", b"package main\n");
        let census = SourceCensus::scan(&project.root).unwrap();
        let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();

        let first = schedule_provider_units(&project.root, &plan, &[]).unwrap();
        let repeated = schedule_provider_units(&project.root, &plan, &[]).unwrap();
        assert_eq!(
            serde_json::to_value(&first.receipt).unwrap(),
            serde_json::to_value(&repeated.receipt).unwrap()
        );
        assert_eq!(first.receipt.project_model_only_count, 1);
        assert_eq!(first.receipt.planned_file_language_count, 3);
        assert_eq!(first.receipt.scheduled_file_language_count, 2);
        assert!(first
            .units
            .iter()
            .flat_map(|unit| &unit.files)
            .all(|path| path.extension().is_none_or(|extension| extension != "vue")));
        assert!(first
            .units
            .iter()
            .all(|scheduled| plan.units.iter().any(|unit| {
                unit.id.as_str() == scheduled.analysis_unit_id
                    && absolute_repository_path(&project.root, &unit.root) == scheduled.root
            })));
    }

    #[test]
    fn dart_reliability_shards_keep_one_analysis_unit_identity() {
        let project = TestProject::new("dart-shards");
        project.write("pubspec.yaml", b"name: sample\n");
        for index in 0..513 {
            project.write(&format!("lib/file_{index}.dart"), b"void value() {}\n");
        }
        let census = SourceCensus::scan(&project.root).unwrap();
        let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
        let schedule = schedule_provider_units(&project.root, &plan, &[]).unwrap();
        let dart = schedule
            .units
            .iter()
            .filter(|unit| unit.lang.id == "dart")
            .collect::<Vec<_>>();
        assert_eq!(dart.len(), 2);
        assert_eq!(dart[0].files.len(), 512);
        assert_eq!(dart[1].files.len(), 1);
        assert_eq!(dart[0].analysis_unit_id, dart[1].analysis_unit_id);
        assert_ne!(dart[0].execution_scope_id, dart[1].execution_scope_id);
    }

    #[test]
    fn typescript_project_references_remain_plan_owned_execution_shards() {
        let project = TestProject::new("typescript-project-references");
        project.write(
            "tsconfig.json",
            br#"{"files":[],"references":[{"path":"packages/a"},{"path":"packages/b"}]}"#,
        );
        project.write(
            "packages/a/tsconfig.json",
            br#"{"compilerOptions":{"composite":true}}"#,
        );
        project.write("packages/a/src/a.ts", b"export const a = 1;\n");
        project.write(
            "packages/b/tsconfig.json",
            br#"{"compilerOptions":{"composite":true}}"#,
        );
        project.write("packages/b/src/b.ts", b"export const b = 2;\n");
        let census = SourceCensus::scan(&project.root).unwrap();
        let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
        let project_units = vec![
            ProjectModelUnit {
                id: "packages/a/tsconfig.json".to_string(),
                config: Some("packages/a/tsconfig.json".to_string()),
                base_config: Some("packages/a/tsconfig.json".to_string()),
                files: vec!["packages/a/src/a.ts".to_string()],
                allow_js: false,
                synthetic: false,
                generated_config: None,
            },
            ProjectModelUnit {
                id: "packages/b/tsconfig.json".to_string(),
                config: Some("packages/b/tsconfig.json".to_string()),
                base_config: Some("packages/b/tsconfig.json".to_string()),
                files: vec!["packages/b/src/b.ts".to_string()],
                allow_js: false,
                synthetic: false,
                generated_config: None,
            },
        ];

        let schedule = schedule_provider_units(&project.root, &plan, &project_units).unwrap();
        let roots = schedule
            .units
            .iter()
            .map(|unit| {
                unit.root
                    .strip_prefix(&project.root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            roots,
            BTreeSet::from(["packages/a".to_string(), "packages/b".to_string()])
        );
        assert_eq!(schedule.receipt.analysis_unit_count, 2);
        assert_eq!(schedule.receipt.execution_shard_count, 2);
        assert_eq!(schedule.receipt.planned_file_language_count, 2);
        assert_eq!(schedule.receipt.scheduled_file_language_count, 2);
        assert!(schedule.units.iter().all(|unit| {
            unit.provider_config.as_ref().is_some_and(|config| {
                config.ends_with("tsconfig.json") && config.starts_with(&unit.root)
            })
        }));
    }

    struct TestProject {
        root: PathBuf,
    }

    impl TestProject {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "codebase-workspace-provider-schedule-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self
                .root
                .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, bytes).unwrap();
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
