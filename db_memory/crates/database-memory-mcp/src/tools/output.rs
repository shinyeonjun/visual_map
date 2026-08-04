pub(crate) fn tool_json<T, E>(result: Result<T, E>) -> Result<String, String>
where
    T: Serialize,
    E: Serialize + fmt::Display,
{
    match result {
        Ok(value) => serde_json::to_string(&value).map_err(|error| error.to_string()),
        Err(error) => Err(serde_json::to_string(&json!({
            "status": "failed",
            "error": error,
        }))
        .unwrap_or_else(|_| error.to_string())),
    }
}

fn ensure_parent_dir(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

