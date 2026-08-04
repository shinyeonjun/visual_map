fn positive_usize(label: &str, value: &str) -> Result<usize, String> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("invalid {label} '{value}'"))?;
    if value == 0 {
        return Err(format!("{label} must be at least 1"));
    }
    Ok(value)
}

fn parse_describe_table_args(
    mut args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let mut positionals = Vec::new();
    let mut object_key = None;
    let mut format = OutputFormat::Text;
    let mut cache_path = None;
    let mut config_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--format" => {
                let value = args.next().ok_or("missing value for --format")?;
                format = parse_format(&value)?;
            }
            "--object-key" => {
                object_key = Some(args.next().ok_or("missing value for --object-key")?);
            }
            "--cache-path" => {
                let value = args.next().ok_or("missing value for --cache-path")?;
                cache_path = Some(PathBuf::from(value));
            }
            "--config-path" => {
                let value = args.next().ok_or("missing value for --config-path")?;
                config_path = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--") => {
                return Err(format!("unknown describe-table flag '{arg}'"));
            }
            _ => positionals.push(arg),
        }
    }

    if positionals.is_empty() || positionals.len() > 2 {
        return Err(describe_table_usage().to_owned());
    }

    let alias = positionals.remove(0);
    let table_name = positionals.pop();
    if object_key.is_some() == table_name.is_some() {
        return Err("pass one table selector: a positional table name or --object-key".to_owned());
    }
    let config_path = config_path.unwrap_or_else(default_config_file_path);
    let profile = profile_for_alias(Some(&alias), &config_path, config_loader);

    Ok(Command::DescribeTable {
        alias,
        object_key,
        table_name,
        format,
        cache_path: cache_path
            .or_else(|| profile.and_then(|profile| profile.cache_path))
            .unwrap_or_else(default_cache_path),
    })
}

fn parse_find_args(
    command: &str,
    mut args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let mut positionals = Vec::new();
    let mut format = OutputFormat::Text;
    let mut cache_path = None;
    let mut config_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => format = OutputFormat::Json,
            "--format" => {
                let value = args.next().ok_or("missing value for --format")?;
                format = parse_format(&value)?;
            }
            "--cache-path" => {
                let value = args.next().ok_or("missing value for --cache-path")?;
                cache_path = Some(PathBuf::from(value));
            }
            "--config-path" => {
                let value = args.next().ok_or("missing value for --config-path")?;
                config_path = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--") => return Err(format!("unknown {command} flag '{arg}'")),
            _ => positionals.push(arg),
        }
    }

    if positionals.len() != 2 {
        return Err(format!(
            "usage: database-memory {command} <alias> <query> [--cache-path <path>] [--config-path <path>]"
        ));
    }

    let alias = positionals.remove(0);
    let query = positionals.remove(0);
    let config_path = config_path.unwrap_or_else(default_config_file_path);
    let profile = profile_for_alias(Some(&alias), &config_path, config_loader);
    let cache_path = cache_path
        .or_else(|| profile.and_then(|profile| profile.cache_path))
        .unwrap_or_else(default_cache_path);
    match command {
        "find-table" => Ok(Command::FindTable {
            alias,
            query,
            format,
            cache_path,
        }),
        "find-column" => Ok(Command::FindColumn {
            alias,
            query,
            format,
            cache_path,
        }),
        _ => unreachable!(),
    }
}

fn parse_traversal_args(
    command: &str,
    mut args: impl Iterator<Item = String>,
    config_loader: &impl Fn(&Path) -> Option<DatabaseMemoryConfig>,
) -> Result<Command, String> {
    let mut positionals = Vec::new();
    let mut object_key = None;
    let mut table_name = None;
    let mut column_name = None;
    let mut direction = Direction::Both;
    let mut max_depth = DEFAULT_TRAVERSAL_DEPTH;
    let mut limit = DEFAULT_RESULT_LIMIT;
    let mut cache_path = None;
    let mut config_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => {}
            "--format" => {
                let value = args.next().ok_or("missing value for --format")?;
                if parse_format(&value)? != OutputFormat::Json {
                    return Err(format!("{command} supports JSON output only"));
                }
            }
            "--object-key" => {
                object_key = Some(args.next().ok_or("missing value for --object-key")?);
            }
            "--table" if command == "impact-analysis" => {
                table_name = Some(args.next().ok_or("missing value for --table")?);
            }
            "--column" if command == "impact-analysis" => {
                column_name = Some(args.next().ok_or("missing value for --column")?);
            }
            "--direction" => {
                let value = args.next().ok_or("missing value for --direction")?;
                direction = parse_direction(&value)?;
            }
            "--max-depth" => {
                let value = args.next().ok_or("missing value for --max-depth")?;
                max_depth = value
                    .parse()
                    .map_err(|_| format!("invalid max depth '{value}'"))?;
            }
            "--limit" => {
                let value = args.next().ok_or("missing value for --limit")?;
                limit = value
                    .parse()
                    .map_err(|_| format!("invalid result limit '{value}'"))?;
                if limit == 0 {
                    return Err("result limit must be at least 1".to_owned());
                }
            }
            "--cache-path" => {
                let value = args.next().ok_or("missing value for --cache-path")?;
                cache_path = Some(PathBuf::from(value));
            }
            "--config-path" => {
                let value = args.next().ok_or("missing value for --config-path")?;
                config_path = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--") => return Err(format!("unknown {command} flag '{arg}'")),
            _ => positionals.push(arg),
        }
    }

    if positionals.is_empty() || positionals.len() > 2 {
        return Err(traversal_usage(command).to_owned());
    }

    let alias = positionals.remove(0);
    let positional_selector = positionals.pop();
    let config_path = config_path.unwrap_or_else(default_config_file_path);
    let profile = profile_for_alias(Some(&alias), &config_path, config_loader);
    let cache_path = cache_path
        .or_else(|| profile.and_then(|profile| profile.cache_path))
        .unwrap_or_else(default_cache_path);

    match command {
        "impact-analysis" => {
            if positional_selector.is_some() && (object_key.is_some() || table_name.is_some()) {
                return Err(
                    "pass one impact target: a positional table, --table, or --object-key"
                        .to_owned(),
                );
            }
            if object_key.is_some() && (table_name.is_some() || column_name.is_some()) {
                return Err("--object-key cannot be combined with --table or --column".to_owned());
            }
            let table_name = table_name.or(positional_selector);
            if object_key.is_none() && table_name.is_none() {
                return Err(traversal_usage(command).to_owned());
            }
            if column_name.is_some() && table_name.is_none() {
                return Err("--column requires --table".to_owned());
            }
            Ok(Command::ImpactAnalysis {
                alias,
                object_key,
                table_name,
                column_name,
                direction,
                max_depth,
                limit,
                cache_path,
            })
        }
        "trace-relationships" => {
            if positional_selector.is_some() && object_key.is_some() {
                return Err("pass the start object key once".to_owned());
            }
            Ok(Command::TraceRelationships {
                alias,
                object_key: object_key
                    .or(positional_selector)
                    .ok_or_else(|| traversal_usage(command).to_owned())?,
                direction,
                max_depth,
                limit,
                cache_path,
            })
        }
        _ => unreachable!(),
    }
}

