fn profile_for_alias(
    alias: Option<&str>,
    config_path: &Path,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Option<ResolvedConnectionProfile> {
    alias.and_then(|alias| config_loader(config_path).and_then(|config| config.profile(alias)))
}

fn parse_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "text" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        _ => Err(format!("unknown format '{value}'; expected text or json")),
    }
}

fn positive_usize(label: &str, value: &str) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("invalid {label} '{value}'"))?;
    if value == 0 {
        return Err(format!("{label} must be at least 1"));
    }
    Ok(value)
}

fn usage() -> &'static str {
    "usage: database-memory contract [--format text|json]\n       database-memory index --source <source> (--path <path> | --connection-string <secret>) --alias <name> [--catalog <name>]... [--schema <name>]... [--timeout-ms <n>] [--format text|json] [--cache-path <path>] [--config-path <path>]
       database-memory list-snapshots [--format text|json] [--cache-path <path>]
       database-memory describe-snapshot <alias-or-snapshot-key> [--format text|json] [--cache-path <path>]
       database-memory list-objects <alias-or-snapshot-key> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]
       database-memory find-objects <alias-or-snapshot-key> <query> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]
       database-memory describe-object <alias-or-snapshot-key> <object-key> [--relationship-limit <n>] [--format text|json] [--cache-path <path>]"
}

fn default_cache_path() -> PathBuf {
    PathBuf::from(".database-memory").join("graph.sqlite")
}
