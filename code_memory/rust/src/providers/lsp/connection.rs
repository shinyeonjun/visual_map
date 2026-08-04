use super::{
    collect_lsp_symbols, configuration_value, diagnostic_language, is_fatal_lsp_error,
    lsp_item_symbol, lsp_max_requests, lsp_message_length_allowed, lsp_session_timeout,
    parse_lsp_range, uri_to_relative_path, LspSymbol, NEXT_LSP_ID,
};
use crate::{Diagnostic, DiagnosticCode};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub(super) struct LspConnection {
    child: std::process::Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    timeout: Duration,
    deadline: Instant,
    request_count: usize,
    max_requests: usize,
    request_cache: HashMap<String, Value>,
    outgoing_call_cache: HashMap<String, Vec<(String, String, Vec<i32>)>>,
    workspace_settings: Value,
    pub(super) fatal_error: Option<String>,
    provider_diagnostics: Vec<ProviderDiagnostic>,
    type_hierarchy_supported: bool,
}

struct ProviderDiagnostic {
    uri: String,
    level: &'static str,
    line: Option<u32>,
    message: String,
}

pub(super) struct LspReference {
    pub(super) uri: String,
    pub(super) range: Vec<i32>,
}

impl LspConnection {
    pub(super) fn new(
        child: std::process::Child,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
        timeout: Duration,
        large_workspace: bool,
    ) -> Result<Self, String> {
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = stdout;
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    if stdout.read_line(&mut line).is_err() || line.is_empty() {
                        return;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                        let Ok(value) = value.trim().parse::<usize>() else {
                            return;
                        };
                        length = Some(value);
                    }
                }
                let Some(length) = length else {
                    return;
                };
                if !lsp_message_length_allowed(length) {
                    return;
                }
                let mut body = vec![0; length];
                if stdout.read_exact(&mut body).is_err() {
                    return;
                }
                let Ok(value) = serde_json::from_slice(&body) else {
                    return;
                };
                if sender.send(value).is_err() {
                    return;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            timeout,
            deadline: Instant::now() + lsp_session_timeout(large_workspace),
            request_count: 0,
            max_requests: lsp_max_requests(),
            request_cache: HashMap::new(),
            outgoing_call_cache: HashMap::new(),
            workspace_settings: Value::Object(serde_json::Map::new()),
            fatal_error: None,
            provider_diagnostics: Vec::new(),
            type_hierarchy_supported: false,
        })
    }

    fn send(&mut self, value: &Value) -> Result<(), String> {
        let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
        self.stdin.write_all(&body).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    fn receive(&mut self) -> Result<Value, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("native LSP session timeout".to_string());
        }
        match self.messages.recv_timeout(self.timeout.min(remaining)) {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "native LSP response timeout after {} ms",
                self.timeout.as_millis()
            )),
            Err(RecvTimeoutError::Disconnected) => Err("native LSP closed stdout".to_string()),
        }
    }

    pub(super) fn wait_for_retry(&self, duration: Duration) -> Result<(), String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining < duration {
            return Err("native LSP session timeout".to_string());
        }
        std::thread::sleep(duration);
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if Instant::now() >= self.deadline {
            return Err("native LSP session timeout".to_string());
        }
        if self.request_count >= self.max_requests {
            return Err(format!(
                "native LSP request budget exceeded after {} requests",
                self.max_requests
            ));
        }
        self.request_count += 1;
        let id = NEXT_LSP_ID.fetch_add(1, Ordering::Relaxed);
        self.send(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            let response = self.receive()?;
            if response.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.record_provider_diagnostics(&response);
                continue;
            }
            if response.get("id") == Some(&Value::from(id)) {
                if let Some(error) = response.get("error") {
                    return Err(format!("native LSP {method} failed: {error}"));
                }
                return Ok(response.get("result").cloned().unwrap_or(Value::Null));
            }
            if let (Some(other_id), Some(other_method)) =
                (response.get("id"), response.get("method"))
            {
                let result = if other_method.as_str() == Some("workspace/configuration") {
                    self.workspace_configuration(response.get("params"))
                } else {
                    Value::Null
                };
                self.send(&serde_json::json!({"jsonrpc":"2.0","id":other_id,"result":result}))?;
                let _ = other_method;
            }
        }
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&serde_json::json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    pub(super) fn set_workspace_settings(&mut self, settings: Value) {
        self.workspace_settings = settings;
    }

    fn workspace_configuration(&self, params: Option<&Value>) -> Value {
        if self
            .workspace_settings
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            return serde_json::json!([]);
        }
        let values = params
            .and_then(|params| params.get("items"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|item| {
                item.get("section")
                    .and_then(Value::as_str)
                    .and_then(|section| configuration_value(&self.workspace_settings, section))
                    .unwrap_or(Value::Null)
            })
            .collect();
        Value::Array(values)
    }

    fn record_provider_diagnostics(&mut self, message: &Value) {
        let Some(params) = message.get("params") else {
            return;
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return;
        };
        for diagnostic in params
            .get("diagnostics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let level = match diagnostic.get("severity").and_then(Value::as_u64) {
                Some(1) => "error",
                Some(2) => "warning",
                Some(3) => "info",
                _ => "warning",
            };
            let line = diagnostic
                .get("range")
                .and_then(|range| range.get("start"))
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .map(|line| line as u32 + 1);
            let Some(text) = diagnostic.get("message").and_then(Value::as_str) else {
                continue;
            };
            self.provider_diagnostics.push(ProviderDiagnostic {
                uri: uri.to_string(),
                level,
                line,
                message: text.to_string(),
            });
        }
    }

    fn drain_provider_notifications(&mut self) {
        while let Ok(message) = self.messages.try_recv() {
            if message.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.record_provider_diagnostics(&message);
            }
        }
    }

    pub(super) fn take_provider_diagnostics(
        &mut self,
        root: &Path,
        language: &str,
    ) -> Vec<Diagnostic> {
        self.drain_provider_notifications();
        let mut seen = HashSet::new();
        self.provider_diagnostics
            .drain(..)
            .filter_map(|diagnostic| {
                let path = uri_to_relative_path(&diagnostic.uri, root);
                let key = format!(
                    "{}:{}:{}:{}",
                    path,
                    diagnostic.line.unwrap_or_default(),
                    diagnostic.level,
                    diagnostic.message
                );
                if !seen.insert(key) {
                    return None;
                }
                let header_context = matches!(language, "c" | "cpp")
                    && matches!(
                        Path::new(&path)
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .map(|extension| extension.to_ascii_lowercase())
                            .as_deref(),
                        Some("h" | "hh" | "hpp" | "hxx" | "inc" | "inl" | "ipp" | "tpp")
                    );
                // LSP publishDiagnostics are source-code diagnostics, not
                // provider failures. Missing external packages, type-check
                // errors, and project-specific warnings must not make a
                // VisualMap index fail; startup, timeout, and invalid-output
                // failures are reported by the outer analysis layer.
                let level = if diagnostic.level == "error" {
                    "warning"
                } else {
                    diagnostic.level
                };
                let message = if header_context && diagnostic.level == "error" {
                    format!("header-context: {}", diagnostic.message)
                } else if diagnostic.level == "error" {
                    format!("provider-diagnostic: {}", diagnostic.message)
                } else {
                    diagnostic.message
                };
                Some(Diagnostic {
                    language: diagnostic_language(&path, language),
                    level,
                    code: DiagnosticCode::ProviderDiagnostic,
                    message,
                    detail: None,
                    path: Some(path),
                    line: diagnostic.line,
                })
            })
            .collect()
    }

    fn cached_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let key = format!(
            "{}:{}",
            method,
            serde_json::to_string(&params).map_err(|error| error.to_string())?
        );
        if let Some(value) = self.request_cache.get(&key) {
            return Ok(value.clone());
        }
        let value = self.request(method, params)?;
        self.request_cache.insert(key, value.clone());
        Ok(value)
    }

    pub(super) fn initialize(
        &mut self,
        root_uri: &str,
        root_path: &str,
        language: &str,
    ) -> Result<(), String> {
        let workspace_capabilities = if language == "rust" {
            serde_json::json!({"workspaceFolders": true, "configuration": true})
        } else {
            serde_json::json!({"workspaceFolders": true})
        };
        let initialization_options = if language == "java" {
            serde_json::json!({"settings": self.workspace_settings})
        } else {
            Value::Null
        };
        let response = self.request(
            "initialize",
            serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "rootPath": root_path,
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
                        "references": {},
                        "typeDefinition": {},
                        "implementation": {},
                        "callHierarchy": {},
                        "typeHierarchy": {}
                    },
                    "workspace": workspace_capabilities
                },
                "workspaceFolders": [{"uri": root_uri, "name": "code_memory"}],
                "initializationOptions": initialization_options
            }),
        )?;
        self.type_hierarchy_supported = response
            .get("capabilities")
            .and_then(|capabilities| capabilities.get("typeHierarchyProvider"))
            .is_some_and(|provider| provider.as_bool().unwrap_or(provider.is_object()));
        Ok(())
    }

    pub(super) fn did_open(&mut self, uri: &str, language: &str, text: &str) -> Result<(), String> {
        self.notify("textDocument/didOpen", serde_json::json!({"textDocument":{"uri":uri,"languageId":language,"version":1,"text":text}}))
    }

    pub(super) fn document_symbols(&mut self, uri: &str) -> Result<Vec<LspSymbol>, String> {
        // Rust projects can publish a partial symbol tree while Cargo and
        // proc-macro state are loading. Retries must reach the provider;
        // caching the first response would make the retry loop ineffective.
        let value = self.request(
            "textDocument/documentSymbol",
            serde_json::json!({"textDocument":{"uri":uri}}),
        )?;
        let mut symbols = Vec::new();
        if let Some(items) = value.as_array() {
            for item in items {
                collect_lsp_symbols(item, &mut symbols);
            }
        }
        Ok(symbols)
    }

    pub(super) fn workspace_symbols(&mut self) -> Result<Vec<(String, LspSymbol)>, String> {
        let value = self.cached_request("workspace/symbol", serde_json::json!({"query":""}))?;
        let mut symbols = Vec::new();
        for item in value.as_array().into_iter().flatten() {
            let Some(uri) = item
                .get("location")
                .and_then(|location| location.get("uri").or_else(|| location.get("targetUri")))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let mut parsed = Vec::new();
            collect_lsp_symbols(item, &mut parsed);
            symbols.extend(parsed.into_iter().map(|symbol| (uri.to_string(), symbol)));
        }
        Ok(symbols)
    }

    pub(super) fn references(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspReference>, String> {
        let value = self.optional_request(
            "textDocument/references",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character},
                "context":{"includeDeclaration":false}
            }),
        );
        Ok(value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(parse_lsp_reference)
            .collect())
    }

    fn optional_request(&mut self, method: &str, params: Value) -> Value {
        if self.fatal_error.is_some() {
            return Value::Null;
        }
        match self.cached_request(method, params) {
            Ok(value) => value,
            Err(error) => {
                // Optional call/type enrichment may time out on a large
                // workspace with unavailable external modules. Drop only
                // that enrichment request; document indexing remains usable.
                if is_fatal_lsp_error(&error)
                    && !error.contains("native LSP response timeout")
                    && self.fatal_error.is_none()
                {
                    self.fatal_error = Some(error);
                }
                Value::Null
            }
        }
    }

    fn optional_uncached_request(&mut self, method: &str, params: Value) -> Value {
        if self.fatal_error.is_some() {
            return Value::Null;
        }
        match self.request(method, params) {
            Ok(value) => value,
            Err(error) => {
                if is_fatal_lsp_error(&error)
                    && !error.contains("native LSP response timeout")
                    && self.fatal_error.is_none()
                {
                    self.fatal_error = Some(error);
                }
                Value::Null
            }
        }
    }

    pub(super) fn type_definitions(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        self.optional_request(
            "textDocument/typeDefinition",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character}
            }),
        )
        .as_array()
        .cloned()
        .unwrap_or_default()
    }

    pub(super) fn supertypes(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        if !self.type_hierarchy_supported {
            return Vec::new();
        }
        let items = self
            .optional_request(
                "textDocument/prepareTypeHierarchy",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            )
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut output = Vec::new();
        for item in items {
            let supers =
                self.optional_request("typeHierarchy/supertypes", serde_json::json!({"item":item}));
            if let Some(values) = supers.as_array() {
                output.extend(values.iter().cloned());
            }
        }
        output
    }

    pub(super) fn implementations(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        self.optional_request(
            "textDocument/implementation",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "position":{"line":line,"character":character}
            }),
        )
        .as_array()
        .cloned()
        .unwrap_or_default()
    }

    pub(super) fn outgoing_calls(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
        root: &Path,
    ) -> Vec<(String, String, Vec<i32>)> {
        let cache_key = format!("{uri}:{line}:{character}");
        if let Some(cached) = self.outgoing_call_cache.get(&cache_key) {
            return cached.clone();
        }
        let items = self
            .optional_request(
                "textDocument/prepareCallHierarchy",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            )
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut output = Vec::new();
        for item in items {
            let calls = self.optional_request(
                "callHierarchy/outgoingCalls",
                serde_json::json!({"item":item}),
            );
            for call in calls.as_array().into_iter().flatten() {
                let Some(target) = call.get("to") else {
                    continue;
                };
                let Some(target_symbol) = lsp_item_symbol(target, root) else {
                    continue;
                };
                let Some(target_uri) = target.get("uri").and_then(Value::as_str) else {
                    continue;
                };
                let target_relative = uri_to_relative_path(target_uri, root);
                for range in call
                    .get("fromRanges")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(parse_lsp_range)
                {
                    output.push((target_symbol.clone(), target_relative.clone(), range));
                }
            }
        }
        if self.fatal_error.is_none() {
            self.outgoing_call_cache.insert(cache_key, output.clone());
        }
        output
    }

    pub(super) fn definitions_at(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Vec<(Option<String>, Vec<i32>)> {
        if self.fatal_error.is_some() {
            return Vec::new();
        }
        for _ in 0..3 {
            if Instant::now() >= self.deadline {
                return Vec::new();
            }
            // Cold providers can answer before their semantic index is ready.
            // An empty definition response must not poison the request cache;
            // every bounded retry below needs to reach the provider.
            let value = self.optional_uncached_request(
                "textDocument/definition",
                serde_json::json!({
                    "textDocument":{"uri":uri},
                    "position":{"line":line,"character":character}
                }),
            );
            let values = match value {
                Value::Array(values) => values,
                Value::Object(_) => vec![value],
                _ => Vec::new(),
            };
            let results: Vec<_> = values
                .into_iter()
                .filter_map(|location| {
                    let target_uri = location
                        .get("uri")
                        .or_else(|| location.get("targetUri"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let range = location
                        .get("range")
                        .or_else(|| location.get("targetSelectionRange"))
                        .and_then(parse_lsp_range)?;
                    Some((target_uri, range))
                })
                .collect();
            if !results.is_empty() {
                return results;
            }
            if self.wait_for_retry(Duration::from_millis(250)).is_err() {
                return Vec::new();
            }
        }
        Vec::new()
    }

    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        self.request("shutdown", Value::Null)?;
        self.notify("exit", Value::Null)
    }

    pub(super) fn ensure_healthy(&self) -> Result<(), String> {
        self.fatal_error
            .as_ref()
            .map(|error| Err(error.clone()))
            .unwrap_or(Ok(()))
    }
}

impl Drop for LspConnection {
    fn drop(&mut self) {
        // Give the server a short grace period after the LSP exit notification.
        // Force-killing rust-analyzer while it is reloading can make it print a
        // misleading worker panic even though indexing succeeded.
        for _ in 0..100 {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => break,
            }
        }

        #[cfg(windows)]
        {
            if self.child.try_wait().ok().flatten().is_none() {
                use std::os::windows::process::CommandExt;
                let mut command = Command::new("taskkill");
                let _ = command
                    .creation_flags(0x08000000)
                    .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn parse_lsp_reference(value: &Value) -> Option<LspReference> {
    Some(LspReference {
        uri: value.get("uri")?.as_str()?.to_string(),
        range: parse_lsp_range(value.get("range")?)?,
    })
}
