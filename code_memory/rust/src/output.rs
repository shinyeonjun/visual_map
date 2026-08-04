fn write_index_outputs(
    root: &Path,
    out: &Path,
    architecture_out: &Path,
    pack_root: &Path,
    output: &IndexOutput,
    source_snapshot: &mut SourceSnapshot,
    project_config_digest: u64,
) -> Result<PathBuf, String> {
    let index_write_started = Instant::now();
    let file = fs::File::create(out).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    let mut writer = BufWriter::new(file);
    write_json(&mut writer, output).map_err(|e| format!("cannot serialize output: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot flush {}: {e}", out.display()))?;
    eprintln!(
        "timing stage=index_json_write elapsed_ms={}",
        index_write_started.elapsed().as_millis()
    );
    if let Some(parent) = architecture_out.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let architecture_started = Instant::now();
    let architecture_key = architecture_cache_key(
        root,
        pack_root,
        output,
        source_snapshot,
        project_config_digest,
    );
    let architecture_cache =
        project_cache_root(root).join(format!("architecture-{architecture_key}.json"));
    if architecture_cache.is_file() {
        fs::copy(&architecture_cache, architecture_out).map_err(|e| {
            format!(
                "cannot copy architecture cache {} to {}: {e}",
                architecture_cache.display(),
                architecture_out.display()
            )
        })?;
        eprintln!(
            "timing stage=architecture_and_json elapsed_ms={} cached=true key={architecture_key}",
            architecture_started.elapsed().as_millis()
        );
        println!("wrote {}", out.display());
        println!("wrote {}", architecture_out.display());
        return Ok(architecture_cache);
    }
    load_source_contents(root, source_snapshot);
    let architecture = architecture::build_with_sources(root, output, source_snapshot);
    let file = fs::File::create(architecture_out)
        .map_err(|e| format!("cannot write {}: {e}", architecture_out.display()))?;
    let mut writer = BufWriter::new(file);
    write_json(&mut writer, &architecture)
        .map_err(|e| format!("cannot serialize architecture output: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot flush {}: {e}", architecture_out.display()))?;
    if let Some(parent) = architecture_cache.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(architecture_out, &architecture_cache);
    eprintln!(
        "timing stage=architecture_and_json elapsed_ms={} cached=false key={architecture_key}",
        architecture_started.elapsed().as_millis()
    );
    println!("wrote {}", out.display());
    println!("wrote {}", architecture_out.display());
    Ok(architecture_cache)
}

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
            .map_err(|e| format!("cannot resolve output path: {e}"))?
            .join(path))
    }
}

fn default_architecture_output(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("language-index");
    out.with_file_name(format!("{stem}.architecture.json"))
}

fn required_path(args: &[String], flag: &str) -> Result<PathBuf, String> {
    optional_path(args, flag).ok_or_else(|| format!("missing {flag} <path>"))
}

fn optional_path(args: &[String], flag: &str) -> Option<PathBuf> {
    for (index, value) in args.iter().enumerate() {
        if value == flag {
            return args.get(index + 1).map(PathBuf::from);
        }
        if let Some(path) = value.strip_prefix(&format!("{flag}=")) {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn required_value(args: &[String], flag: &str) -> Result<String, String> {
    optional_value(args, flag).ok_or_else(|| format!("missing {flag} <value>"))
}

fn optional_value(args: &[String], flag: &str) -> Option<String> {
    for (index, value) in args.iter().enumerate() {
        if value == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = value.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn repeated_values(args: &[String], flag: &str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("missing value after {flag}"))?;
            values.push(value.clone());
            index += 2;
        } else {
            if let Some(value) = args[index].strip_prefix(&format!("{flag}=")) {
                values.push(value.to_string());
            }
            index += 1;
        }
    }
    Ok(values)
}
