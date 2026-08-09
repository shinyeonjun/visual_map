pub(crate) fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    parse_args_with_config(args, |path| load_optional_config(path).ok().flatten())
}

pub(crate) fn parse_args_with_config(
    args: impl IntoIterator<Item = String>,
    config_loader: impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("contract") => parse_contract_args(args),
        Some("index") => parse_index_args(args, &config_loader),
        Some("list-snapshots") => parse_list_snapshots_args(args),
        Some("describe-snapshot") => parse_describe_snapshot_args(args, &config_loader),
        Some(command @ ("list-objects" | "find-objects" | "describe-object")) => {
            parse_object_command(command, args, &config_loader)
        }
        Some(command) => Err(format!("unknown command '{command}'")),
        None => Err(usage().to_owned()),
    }
}

fn parse_contract_args(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut format = OutputFormat::Text;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--format" => {
                let value = args.next().ok_or("missing value for --format")?;
                format = parse_format(&value)?;
            }
            _ => return Err(format!("unknown contract flag '{arg}'")),
        }
    }
    Ok(Command::Contract { format })
}

fn parse_index_args(
    mut args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let mut source = None;
    let mut path = None;
    let mut alias = None;
    let mut connection_string = None;
    let mut requested_catalogs = Vec::new();
    let mut requested_schemas = Vec::new();
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut format = OutputFormat::Text;
    let mut cache_path = None;
    let mut config_path = None;

    while let Some(flag) = args.next() {
        if flag == "--json" {
            format = OutputFormat::Json;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--source" => source = Some(value),
            "--path" => path = Some(PathBuf::from(value)),
            "--connection-string" => connection_string = Some(value),
            "--alias" => alias = Some(value),
            "--catalog" => requested_catalogs.push(value),
            "--schema" => requested_schemas.push(value),
            "--timeout-ms" => {
                timeout_ms = value
                    .parse()
                    .map_err(|_| format!("invalid timeout '{value}'"))?;
            }
            "--format" => format = parse_format(&value)?,
            "--cache-path" => cache_path = Some(PathBuf::from(value)),
            "--config-path" => config_path = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown index flag '{flag}'")),
        }
    }

    let config_path = config_path.unwrap_or_else(default_config_file_path);
    let profile = profile_for_alias(alias.as_deref(), &config_path, config_loader);

    let source = source
        .or_else(|| profile.as_ref().map(|profile| profile.source.clone()))
        .ok_or("missing --source")?;
    let path = path.or_else(|| profile.as_ref().and_then(|profile| profile.path.clone()));
    let connection_string = connection_string.or_else(|| {
        profile
            .as_ref()
            .and_then(|profile| profile.connection_string.clone())
    });

    match source.as_str() {
        "sqlite" | "ddl-sqlite" if path.is_none() => return Err("missing --path".to_owned()),
        "postgres" | "yugabytedb" | "mysql" | "mariadb" | "sqlserver" | "oracle" | "odbc"
            if connection_string.is_none() =>
        {
            return Err("missing --connection-string".to_owned());
        }
        _ => {}
    }

    Ok(Command::Index {
        source,
        path,
        connection_string,
        alias: alias.ok_or("missing --alias")?,
        requested_catalogs,
        requested_schemas,
        timeout_ms,
        format,
        cache_path: cache_path
            .or_else(|| profile.and_then(|profile| profile.cache_path))
            .unwrap_or_else(default_cache_path),
    })
}

fn parse_list_snapshots_args(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut format = OutputFormat::Text;
    let mut cache_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--format" => {
                format = parse_format(&args.next().ok_or("missing value for --format")?)?;
            }
            "--cache-path" => {
                cache_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --cache-path")?,
                ));
            }
            _ => return Err(format!("unknown list-snapshots flag '{arg}'")),
        }
    }
    Ok(Command::ListSnapshots {
        format,
        cache_path: cache_path.unwrap_or_else(default_cache_path),
    })
}

fn parse_describe_snapshot_args(
    args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let ParsedReadCommand {
        mut positionals,
        format,
        cache_path,
        ..
    } = parse_read_command_args("describe-snapshot", args, config_loader)?;
    if positionals.len() != 1 {
        return Err("usage: database-memory describe-snapshot <alias-or-snapshot-key> [--format text|json] [--cache-path <path>]".to_owned());
    }
    Ok(Command::DescribeSnapshot {
        selector: positionals.remove(0),
        format,
        cache_path,
    })
}

fn parse_object_command(
    command: &str,
    args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let ParsedReadCommand {
        mut positionals,
        kind,
        offset,
        limit,
        relationship_limit,
        format,
        cache_path,
    } = parse_read_command_args(command, args, config_loader)?;
    match command {
        "list-objects" if positionals.len() == 1 => Ok(Command::ListObjects {
            selector: positionals.remove(0),
            kind,
            offset,
            limit,
            format,
            cache_path,
        }),
        "find-objects" if positionals.len() == 2 => Ok(Command::FindObjects {
            selector: positionals.remove(0),
            query: positionals.remove(0),
            kind,
            offset,
            limit,
            format,
            cache_path,
        }),
        "describe-object" if positionals.len() == 2 && kind.is_none() => {
            Ok(Command::DescribeObject {
                selector: positionals.remove(0),
                object_key: positionals.remove(0),
                relationship_limit,
                format,
                cache_path,
            })
        }
        "list-objects" => Err("usage: database-memory list-objects <alias-or-snapshot-key> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]".to_owned()),
        "find-objects" => Err("usage: database-memory find-objects <alias-or-snapshot-key> <query> [--kind <object-kind>] [--offset <n>] [--limit <n>] [--format text|json] [--cache-path <path>]".to_owned()),
        "describe-object" => Err("usage: database-memory describe-object <alias-or-snapshot-key> <object-key> [--relationship-limit <n>] [--format text|json] [--cache-path <path>]".to_owned()),
        _ => unreachable!(),
    }
}

struct ParsedReadCommand {
    positionals: Vec<String>,
    kind: Option<ObjectKind>,
    offset: usize,
    limit: usize,
    relationship_limit: usize,
    format: OutputFormat,
    cache_path: PathBuf,
}

fn parse_read_command_args(
    command: &str,
    mut args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<ParsedReadCommand, String> {
    let mut positionals = Vec::new();
    let mut kind = None;
    let mut offset = 0;
    let mut limit = DEFAULT_OBJECT_PAGE_LIMIT;
    let mut relationship_limit = DEFAULT_RELATIONSHIP_LIMIT;
    let mut format = OutputFormat::Text;
    let mut cache_path = None;
    let mut config_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--format" => {
                format = parse_format(&args.next().ok_or("missing value for --format")?)?;
            }
            "--kind" if matches!(command, "list-objects" | "find-objects") => {
                let value = args.next().ok_or("missing value for --kind")?;
                kind = Some(value.parse().map_err(|_| {
                    format!("unknown object kind '{value}'; use a contract object kind")
                })?);
            }
            "--offset" if matches!(command, "list-objects" | "find-objects") => {
                let value = args.next().ok_or("missing value for --offset")?;
                offset = value
                    .parse()
                    .map_err(|_| format!("invalid object offset '{value}'"))?;
            }
            "--limit" if matches!(command, "list-objects" | "find-objects") => {
                let value = args.next().ok_or("missing value for --limit")?;
                limit = positive_usize("object limit", &value)?;
            }
            "--relationship-limit" if command == "describe-object" => {
                let value = args
                    .next()
                    .ok_or("missing value for --relationship-limit")?;
                relationship_limit = positive_usize("relationship limit", &value)?;
            }
            "--cache-path" => {
                cache_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --cache-path")?,
                ));
            }
            "--config-path" => {
                config_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --config-path")?,
                ));
            }
            _ if arg.starts_with("--") => return Err(format!("unknown {command} flag '{arg}'")),
            _ => positionals.push(arg),
        }
    }
    let config_path = config_path.unwrap_or_else(default_config_file_path);
    let profile = profile_for_alias(
        positionals.first().map(String::as_str),
        &config_path,
        config_loader,
    );
    Ok(ParsedReadCommand {
        positionals,
        kind,
        offset,
        limit,
        relationship_limit,
        format,
        cache_path: cache_path
            .or_else(|| profile.and_then(|profile| profile.cache_path))
            .unwrap_or_else(default_cache_path),
    })
}
