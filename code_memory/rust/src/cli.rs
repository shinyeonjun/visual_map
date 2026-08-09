fn write_json<T: Serialize, W: Write>(writer: &mut W, value: &T) -> serde_json::Result<()> {
    if env::var_os("CODE_MEMORY_PRETTY_JSON").is_some() {
        serde_json::to_writer_pretty(writer, value)
    } else {
        serde_json::to_writer(writer, value)
    }
}

fn resolve_output_path(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .map_err(|error| format!("cannot resolve output path: {error}"))?
            .join(path))
    }
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, String> {
    optional_path(args, flag).ok_or_else(|| format!("missing {flag} <path>"))
}

fn optional_path(args: &[String], flag: &str) -> Option<PathBuf> {
    optional_value(args, flag).map(PathBuf::from)
}

fn optional_value(args: &[String], flag: &str) -> Option<String> {
    for (index, value) in args.iter().enumerate() {
        if value == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(path) = value.strip_prefix(&format!("{flag}=")) {
            return Some(path.to_string());
        }
    }
    None
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeEngineContract {
    schema: &'static str,
    version: &'static str,
    contract_version: &'static str,
    commands: &'static [&'static str],
}

fn code_engine_contract() -> CodeEngineContract {
    CodeEngineContract {
        schema: CODE_ENGINE_CONTRACT_SCHEMA,
        version: CODE_ENGINE_VERSION,
        contract_version: CODE_ENGINE_CONTRACT_VERSION,
        commands: CODE_ENGINE_COMMANDS,
    }
}

fn print_code_engine_contract() -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    write_json(&mut output, &code_engine_contract())
        .map_err(|error| format!("cannot encode code engine contract: {error}"))?;
    writeln!(&mut output).map_err(|error| format!("cannot write code engine contract: {error}"))
}

fn list_languages() -> Result<(), String> {
    for lang in LANGUAGES {
        let provider = match lang.provider {
            ProviderKind::Scip => "SCIP",
            ProviderKind::Lsp => "native LSP -> SCIP",
        };
        println!("{}\t{}\t{}\t{}", lang.id, lang.name, provider, lang.tool);
    }
    Ok(())
}

#[cfg(test)]
mod contract_tests {
    use super::{code_engine_contract, CODE_ENGINE_COMMANDS};

    #[test]
    fn code_engine_contract_declares_the_current_product_commands_only() {
        let contract = code_engine_contract();

        assert_eq!(
            contract.schema,
            "codebase-workspace.code-engine-contract.v1"
        );
        assert_eq!(contract.version, "0.1.0");
        assert_eq!(contract.contract_version, "3");
        for required in ["contract", "list", "doctor", "detect-languages", "index"] {
            assert!(contract.commands.contains(&required));
        }
        assert!(!CODE_ENGINE_COMMANDS.contains(&"collect"));
    }
}

fn validate_framework_packs(root: &Path) -> Result<(), String> {
    let catalog_path = root.join("packs").join("framework").join("catalog.json");
    let catalog: Value = serde_json::from_slice(
        &fs::read(&catalog_path)
            .map_err(|e| format!("cannot read {}: {e}", catalog_path.display()))?,
    )
    .map_err(|e| format!("invalid framework catalog: {e}"))?;
    if catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-pack-catalog.v1")
    {
        return Err("invalid framework catalog schema".to_string());
    }
    let adapter_path = root.join("packs").join("framework").join("adapters.json");
    let adapter_catalog: Value = serde_json::from_slice(
        &fs::read(&adapter_path)
            .map_err(|e| format!("cannot read {}: {e}", adapter_path.display()))?,
    )
    .map_err(|e| format!("invalid framework adapter catalog: {e}"))?;
    if adapter_catalog.get("schema").and_then(Value::as_str)
        != Some("code-memory.framework-adapter-catalog.v1")
    {
        return Err("invalid framework adapter catalog schema".to_string());
    }
    let adapters = adapter_catalog
        .get("adapters")
        .and_then(Value::as_object)
        .ok_or("framework adapter catalog has no adapters")?;
    let languages = catalog
        .get("languages")
        .and_then(Value::as_array)
        .ok_or("framework catalog has no languages")?;
    let mut seen = HashSet::new();
    let mut total = 0usize;
    for language in languages {
        let id = language
            .get("id")
            .and_then(Value::as_str)
            .ok_or("framework catalog language has no id")?;
        if !LANGUAGES.iter().any(|supported| supported.id == id) {
            return Err(format!("framework catalog language is not supported: {id}"));
        }
        let file = language
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("framework catalog language has no file: {id}"))?;
        let path = catalog_path.parent().unwrap_or(Path::new(".")).join(file);
        let document: Value = serde_json::from_slice(
            &fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
        )
        .map_err(|e| format!("invalid framework pack file {}: {e}", path.display()))?;
        if document.get("language").and_then(Value::as_str) != Some(id) {
            return Err(format!(
                "framework pack file has wrong language: {}",
                path.display()
            ));
        }
        for pack_ref in document
            .get("packs")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("framework pack file has no packs: {}", path.display()))?
        {
            let pack_id = pack_ref
                .get("id")
                .and_then(Value::as_str)
                .ok_or("framework pack has no id")?;
            let pack_file = pack_ref
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("framework pack has no path: {id}/{pack_id}"))?;
            let pack_path = path.parent().unwrap_or(Path::new(".")).join(pack_file);
            let pack: Value = serde_json::from_slice(
                &fs::read(&pack_path)
                    .map_err(|e| format!("cannot read {}: {e}", pack_path.display()))?,
            )
            .map_err(|e| format!("invalid framework pack {}: {e}", pack_path.display()))?;
            if pack.get("schema").and_then(Value::as_str) != Some("code-memory.framework-pack.v1")
                || pack.get("language").and_then(Value::as_str) != Some(id)
                || pack.get("id").and_then(Value::as_str) != Some(pack_id)
            {
                return Err(format!(
                    "framework pack reference mismatch: {}",
                    pack_path.display()
                ));
            }
            let fixture_path = pack_path
                .parent()
                .unwrap_or(Path::new("."))
                .join("fixture.json");
            let fixture: Value = serde_json::from_slice(
                &fs::read(&fixture_path)
                    .map_err(|e| format!("cannot read {}: {e}", fixture_path.display()))?,
            )
            .map_err(|e| format!("invalid framework fixture {}: {e}", fixture_path.display()))?;
            if fixture.get("schema").and_then(Value::as_str)
                != Some("code-memory.framework-fixture.v1")
                || fixture.get("language").and_then(Value::as_str) != Some(id)
                || fixture.get("framework").and_then(Value::as_str) != Some(pack_id)
            {
                return Err(format!(
                    "framework fixture reference mismatch: {}",
                    fixture_path.display()
                ));
            }
            let fixture_files =
                fixture
                    .get("files")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!("framework fixture has no files: {}", fixture_path.display())
                    })?;
            if fixture_files.is_empty() {
                return Err(format!(
                    "framework fixture has no files: {}",
                    fixture_path.display()
                ));
            }
            for file in fixture_files {
                for field in ["path", "source"] {
                    if file
                        .get(field)
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    {
                        return Err(format!(
                            "framework fixture file has no {field}: {}",
                            fixture_path.display()
                        ));
                    }
                }
            }
            let fixture_facts = fixture
                .get("expected")
                .and_then(|expected| expected.get("facts"))
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!(
                        "framework fixture has no expected facts: {}",
                        fixture_path.display()
                    )
                })?;
            let rules = pack
                .get("rule_sets")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack has no rule_sets: {id}/{pack_id}"))?;
            let fixture_fact_names = fixture_facts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            let rule_names = rules.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            if fixture_fact_names != rule_names {
                return Err(format!(
                    "framework fixture facts do not match rule_sets: {}",
                    fixture_path.display()
                ));
            }
            let qualified = format!("{id}/{pack_id}");
            if !seen.insert(qualified.clone()) {
                return Err(format!("duplicate framework pack: {qualified}"));
            }
            if adapters
                .get(&qualified)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("framework pack has no adapter: {qualified}"));
            }
            for field in ["name", "kind"] {
                if pack.get(field).and_then(Value::as_str).is_none() {
                    return Err(format!("framework pack {qualified} has no {field}"));
                }
            }
            for field in ["signals", "outputs", "rule_sets"] {
                if pack
                    .get(field)
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
                {
                    return Err(format!("framework pack {qualified} has no {field}"));
                }
            }
            let outputs = pack
                .get("outputs")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack {qualified} has invalid outputs"))?;
            let rules = pack
                .get("rule_sets")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("framework pack {qualified} has invalid rule_sets"))?;
            for output in outputs.iter().filter_map(Value::as_str) {
                if output != "HANDLES"
                    && !rules
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|rule| rule == output)
                {
                    return Err(format!(
                        "framework pack {qualified} output has no rule_set: {output}"
                    ));
                }
                if output == "HANDLES"
                    && !rules
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|rule| rule == "HTTP_ROUTE" || rule == "RPC_ENDPOINT")
                {
                    return Err(format!(
                        "framework pack {qualified} HANDLES has no route or RPC rule"
                    ));
                }
            }
            println!("{qualified}");
            total += 1;
        }
    }
    println!("framework-packs\t{total}");
    Ok(())
}

fn doctor(providers_root: Option<&Path>) -> Result<(), String> {
    let mut missing = 0;
    for lang in LANGUAGES {
        let ready = provider_ready(lang, providers_root);
        if !ready {
            missing += 1;
        }
        println!(
            "{}\t{}\ttool={}",
            lang.id,
            if ready { "READY" } else { "MISSING" },
            if matches!(lang.id, "c" | "cpp") && find_tool(lang.tool, providers_root).is_none() {
                "clangd (fallback)"
            } else {
                lang.tool
            }
        );
    }
    if missing > 0 {
        println!("missing_tools\t{}", missing);
    }
    Ok(())
}

fn enforce_managed_provider_policy(
    provenance: &[ProviderProvenance],
    discovered_files: &[(String, PathBuf)],
) -> Result<(), String> {
    if env::var("CODE_MEMORY_REQUIRE_MANAGED_PROVIDERS").as_deref() != Ok("1") {
        return Ok(());
    }
    let unmanaged: Vec<String> = provenance
        .iter()
        // `discovered_files` includes structural sources such as Vue SFCs.
        // Checking the final discovery list keeps the policy aligned with the
        // files that actually participate in project-model analysis.
        .filter(|item| {
            discovered_files
                .iter()
                .any(|(language, _)| language == &item.language)
        })
        .filter(|item| item.origin != "managed-manifest")
        .map(|item| format!("{}={} ({})", item.language, item.tool, item.origin))
        .collect();
    if unmanaged.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "managed provider policy rejected unmanaged providers: {}",
            unmanaged.join(", ")
        ))
    }
}
