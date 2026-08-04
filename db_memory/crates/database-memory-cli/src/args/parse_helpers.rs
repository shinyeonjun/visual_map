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

fn parse_direction(value: &str) -> Result<Direction, String> {
    match value {
        "inbound" => Ok(Direction::Inbound),
        "outbound" => Ok(Direction::Outbound),
        "both" => Ok(Direction::Both),
        _ => Err(format!(
            "unknown direction '{value}'; expected inbound, outbound, or both"
        )),
    }
}

fn traversal_usage(command: &str) -> &'static str {
    match command {
        "impact-analysis" => "usage: database-memory impact-analysis <alias> [<table-name> | --table <name> [--column <name>] | --object-key <key>] [--direction inbound|outbound|both] [--max-depth <n>] [--limit <n>] [--cache-path <path>] [--config-path <path>]",
        "trace-relationships" => "usage: database-memory trace-relationships <alias> <object-key> [--direction inbound|outbound|both] [--max-depth <n>] [--limit <n>] [--cache-path <path>] [--config-path <path>]",
        _ => unreachable!(),
    }
}

fn describe_table_usage() -> &'static str {
    "usage: database-memory describe-table <alias> [<table-name> | --object-key <stable-key>] [--format text|json] [--cache-path <path>] [--config-path <path>]"
}

fn inventory_usage() -> &'static str {
    "usage: database-memory inventory <alias> [--offset <n>] [--limit <n>] [--format json] [--cache-path <path>] [--config-path <path>]"
}

fn usage() -> &'static str {
    "usage: database-memory contract [--format text|json]\n       database-memory index --source <source> (--path <path> | --connection-string <secret>) --alias <name> [--catalog <name>]... [--schema <name>]... [--timeout-ms <n>] [--format text|json] [--cache-path <path>] [--config-path <path>]
       database-memory list-snapshots [--format text|json] [--cache-path <path>]
       database-memory describe-snapshot <alias-or-snapshot-key> [--format text|json] [--cache-path <path>]
       database-memory list-objects <alias-or-snapshot-key> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]
       database-memory find-objects <alias-or-snapshot-key> <query> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]
       database-memory describe-object <alias-or-snapshot-key> <object-key> [--relationship-limit <n>] [--format text|json] [--cache-path <path>]
       database-memory describe-table <alias> [<table-name> | --object-key <stable-key>] [--format text|json] [--cache-path <path>] [--config-path <path>]
       database-memory inventory <alias> [--offset <n>] [--limit <n>] [--format json] [--cache-path <path>] [--config-path <path>]
       database-memory find-table <alias> <query> [--format text|json] [--cache-path <path>] [--config-path <path>]
       database-memory find-column <alias> <query> [--format text|json] [--cache-path <path>] [--config-path <path>]
       database-memory impact-analysis <alias> [<table-name> | --table <name> [--column <name>] | --object-key <key>] [--direction inbound|outbound|both] [--max-depth <n>] [--limit <n>] [--cache-path <path>] [--config-path <path>]
       database-memory trace-relationships <alias> <object-key> [--direction inbound|outbound|both] [--max-depth <n>] [--limit <n>] [--cache-path <path>] [--config-path <path>]"
}

fn default_cache_path() -> PathBuf {
    PathBuf::from(".database-memory").join("graph.sqlite")
}
