use super::{
    collect_lsp_symbols, configuration_value, diagnostic_language, is_fatal_lsp_error,
    lsp_item_symbol, lsp_max_requests, lsp_message_length_allowed, lsp_session_timeout,
    parse_lsp_range, uri_to_relative_path, LspSymbol, NEXT_LSP_ID,
};
use crate::{Diagnostic, DiagnosticCode, ProviderProcessGuard};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

type DefinitionLocations = Vec<(Option<String>, Vec<i32>)>;

pub(super) struct LspConnection {
    child: std::process::Child,
    process_guard: ProviderProcessGuard,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    timeout: Duration,
    deadline: Instant,
    request_count: usize,
    max_requests: usize,
    request_metrics: HashMap<String, LspRequestMetric>,
    request_cache: HashMap<String, Value>,
    definition_cache: HashMap<String, DefinitionLocations>,
    outgoing_call_cache: HashMap<String, Vec<(String, String, Vec<i32>)>>,
    timed_out_query_scopes: HashSet<String>,
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

#[derive(Default)]
struct LspRequestMetric {
    batches: usize,
    requests: usize,
    errors: usize,
    total_wall_ms: u128,
    max_batch_ms: u128,
}

pub(super) struct LspReference {
    pub(super) uri: String,
    pub(super) range: Vec<i32>,
}

pub(super) fn complete_pending_with_error(
    pending: &mut HashMap<i64, usize>,
    results: &mut [Option<Result<Value, String>>],
    error: &str,
) -> Vec<i64> {
    let mut request_ids = pending.keys().copied().collect::<Vec<_>>();
    request_ids.sort_unstable();
    for request_id in &request_ids {
        if let Some(index) = pending.remove(request_id) {
            results[index] = Some(Err(error.to_string()));
        }
    }
    request_ids
}

pub(super) fn request_timeout_scope(method: &str, params: &Value) -> Option<String> {
    let uri = params
        .pointer("/textDocument/uri")
        .or_else(|| params.pointer("/item/uri"))
        .or_else(|| params.get("uri"))
        .and_then(Value::as_str)?;
    Some(format!("{method}\0{uri}"))
}

impl LspConnection {
    pub(super) fn new(
        child: std::process::Child,
        process_guard: ProviderProcessGuard,
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
            process_guard,
            stdin,
            messages,
            timeout,
            deadline: Instant::now() + lsp_session_timeout(large_workspace),
            request_count: 0,
            max_requests: lsp_max_requests(),
            request_metrics: HashMap::new(),
            request_cache: HashMap::new(),
            definition_cache: HashMap::new(),
            outgoing_call_cache: HashMap::new(),
            timed_out_query_scopes: HashSet::new(),
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

    fn receive_until(&mut self, response_deadline: Instant) -> Result<Value, String> {
        let now = Instant::now();
        let session_remaining = self.deadline.saturating_duration_since(now);
        if session_remaining.is_zero() {
            return Err("native LSP session timeout".to_string());
        }
        let response_remaining = response_deadline.saturating_duration_since(now);
        if response_remaining.is_zero() {
            return Err(format!(
                "native LSP response timeout after {} ms",
                self.timeout.as_millis()
            ));
        }
        match self
            .messages
            .recv_timeout(session_remaining.min(response_remaining))
        {
            Ok(value) => Ok(value),
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= self.deadline {
                    Err("native LSP session timeout".to_string())
                } else {
                    Err(format!(
                        "native LSP response timeout after {} ms",
                        self.timeout.as_millis()
                    ))
                }
            }
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

    pub(super) fn extend_session_for_large_workspace(&mut self) {
        self.deadline = self
            .deadline
            .max(Instant::now() + lsp_session_timeout(true));
    }

    pub(super) fn remaining_request_budget(&self) -> usize {
        self.max_requests.saturating_sub(self.request_count)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_batch(method, vec![params])?
            .pop()
            .ok_or_else(|| format!("native LSP {method} returned no response"))?
    }

    /// Pipelines independent requests over one LSP connection. The provider
    /// may return responses in any order; this method restores input order so
    /// callers produce exactly the same deterministic facts as serial calls.
    fn request_batch(
        &mut self,
        method: &str,
        params: Vec<Value>,
    ) -> Result<Vec<Result<Value, String>>, String> {
        if params.is_empty() {
            return Ok(Vec::new());
        }
        let request_total = params.len();
        let started = Instant::now();
        let result = self.request_batch_inner(method, params);
        let error_count = match &result {
            Ok(values) => values.iter().filter(|value| value.is_err()).count(),
            Err(_) => request_total,
        };
        let elapsed_ms = started.elapsed().as_millis();
        let metric = self.request_metrics.entry(method.to_string()).or_default();
        metric.batches += 1;
        metric.requests += request_total;
        metric.errors += error_count;
        metric.total_wall_ms += elapsed_ms;
        metric.max_batch_ms = metric.max_batch_ms.max(elapsed_ms);
        result
    }

    fn request_batch_inner(
        &mut self,
        method: &str,
        params: Vec<Value>,
    ) -> Result<Vec<Result<Value, String>>, String> {
        if Instant::now() >= self.deadline {
            return Err("native LSP session timeout".to_string());
        }
        let request_count = params
            .iter()
            .filter(|params| {
                request_timeout_scope(method, params)
                    .as_ref()
                    .is_none_or(|scope| !self.timed_out_query_scopes.contains(scope))
            })
            .count();
        if self.request_count.saturating_add(request_count) > self.max_requests {
            return Err(format!(
                "native LSP request budget exceeded after {} requests",
                self.max_requests
            ));
        }
        let mut pending = HashMap::with_capacity(params.len());
        let mut pending_scopes = HashMap::<i64, String>::new();
        let mut results = (0..params.len()).map(|_| None).collect::<Vec<_>>();
        for (index, params) in params.into_iter().enumerate() {
            let timeout_scope = request_timeout_scope(method, &params);
            if timeout_scope
                .as_ref()
                .is_some_and(|scope| self.timed_out_query_scopes.contains(scope))
            {
                results[index] = Some(Err(format!(
                    "native LSP response timeout previously observed for {method} on the same document"
                )));
                continue;
            }
            let id = NEXT_LSP_ID.fetch_add(1, Ordering::Relaxed);
            pending.insert(id, index);
            if let Some(timeout_scope) = timeout_scope {
                pending_scopes.insert(id, timeout_scope);
            }
            self.send(
                &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            )?;
        }
        self.request_count += pending.len();
        // One provider request can fail to answer even after its siblings have
        // completed (JDTLS does this for source sets outside the active build
        // path). Notifications must not keep extending the batch forever.
        let response_deadline = (Instant::now() + self.timeout).min(self.deadline);
        while !pending.is_empty() {
            let response = match self.receive_until(response_deadline) {
                Ok(response) => response,
                Err(error) if error.contains("native LSP response timeout") => {
                    let timed_out_scopes = pending
                        .keys()
                        .filter_map(|id| pending_scopes.get(id))
                        .cloned()
                        .collect::<HashSet<_>>();
                    self.timed_out_query_scopes
                        .extend(timed_out_scopes.iter().cloned());
                    if std::env::var_os("CODE_MEMORY_LSP_TIMING").is_some() {
                        eprintln!(
                            "lsp batch timeout method={method} pending={} quarantined_scopes={}",
                            pending.len(),
                            timed_out_scopes.len()
                        );
                    }
                    let pending_error = format!("{error} while awaiting {method}");
                    let cancelled =
                        complete_pending_with_error(&mut pending, &mut results, &pending_error);
                    for id in cancelled {
                        // Cancellation is best-effort. A provider that ignored
                        // the original request may also ignore cancellation;
                        // completed sibling responses are still valid facts.
                        let _ = self.notify("$/cancelRequest", serde_json::json!({"id":id}));
                    }
                    break;
                }
                Err(error) => return Err(error),
            };
            if response.get("method").and_then(Value::as_str)
                == Some("textDocument/publishDiagnostics")
            {
                self.record_provider_diagnostics(&response);
                continue;
            }
            if response.get("method").is_none() {
                if let Some(response_id) = response.get("id").and_then(Value::as_i64) {
                    if let Some(index) = pending.remove(&response_id) {
                        results[index] = Some(if let Some(error) = response.get("error") {
                            Err(format!("native LSP {method} failed: {error}"))
                        } else {
                            Ok(response.get("result").cloned().unwrap_or(Value::Null))
                        });
                        continue;
                    }
                }
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
        Ok(results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| Err(format!("native LSP {method} response was lost")))
            })
            .collect())
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
                // desktop index fail; startup, timeout, and invalid-output
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
        let workspace_capabilities = if matches!(language, "rust" | "java") {
            serde_json::json!({"workspaceFolders": true, "configuration": true})
        } else {
            serde_json::json!({"workspaceFolders": true})
        };
        let initialization_options =
            initialization_options(language, &self.workspace_settings);
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
        Ok(parse_document_symbols(value))
    }

    pub(super) fn document_symbols_batch(
        &mut self,
        uris: &[String],
    ) -> Result<Vec<Result<Vec<LspSymbol>, String>>, String> {
        let params = uris
            .iter()
            .map(|uri| serde_json::json!({"textDocument":{"uri":uri}}))
            .collect();
        Ok(self
            .request_batch("textDocument/documentSymbol", params)?
            .into_iter()
            .map(|result| result.map(parse_document_symbols))
            .collect())
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
                self.record_optional_error(error);
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
                self.record_optional_error(error);
                Value::Null
            }
        }
    }

    fn record_optional_error(&mut self, error: String) {
        if is_fatal_lsp_error(&error)
            && !error.contains("native LSP response timeout")
            && self.fatal_error.is_none()
        {
            self.fatal_error = Some(error);
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
            collect_outgoing_calls(&calls, root, &mut output);
        }
        if self.fatal_error.is_none() {
            self.outgoing_call_cache.insert(cache_key, output.clone());
        }
        output
    }

    /// Prefetches the two dependent call-hierarchy phases in bounded batches.
    /// The final cache key and value are identical to `outgoing_calls`, so the
    /// normal extraction loop remains the single consumer of call facts.
    pub(super) fn prefetch_outgoing_calls(
        &mut self,
        queries: &[(String, u32, u32)],
        root: &Path,
        batch_size: usize,
    ) {
        if queries.is_empty() || self.fatal_error.is_some() {
            return;
        }
        let width = batch_size.max(1);
        for chunk in queries.chunks(width) {
            // A call fact needs both phases. Do not spend the entire budget on
            // prepareCallHierarchy for the whole workspace before asking a
            // single outgoingCalls question. Each bounded chunk is completed
            // end-to-end and cached before advancing to the next chunk.
            if self.remaining_request_budget() < chunk.len() {
                return;
            }
            let params = chunk
                .iter()
                .map(|(uri, line, character)| {
                    serde_json::json!({
                        "textDocument":{"uri":uri},
                        "position":{"line":line,"character":character}
                    })
                })
                .collect();
            let responses = match self.request_batch("textDocument/prepareCallHierarchy", params) {
                Ok(responses) => responses,
                Err(error) => {
                    self.record_optional_error(error);
                    return;
                }
            };
            let mut prepared = (0..chunk.len())
                .map(|_| Vec::new())
                .collect::<Vec<Vec<Value>>>();
            for (offset, response) in responses.into_iter().enumerate() {
                match response {
                    Ok(value) => {
                        prepared[offset] = value.as_array().cloned().unwrap_or_default();
                    }
                    Err(error) => self.record_optional_error(error),
                }
            }
            if self.fatal_error.is_some() {
                return;
            }
            let tasks = prepared
                .iter()
                .enumerate()
                .flat_map(|(owner, items)| items.iter().cloned().map(move |item| (owner, item)))
                .collect::<Vec<_>>();
            if self.remaining_request_budget() < tasks.len() {
                return;
            }
            let mut outputs = (0..chunk.len())
                .map(|_| Vec::<(String, String, Vec<i32>)>::new())
                .collect::<Vec<_>>();
            for task_chunk in tasks.chunks(width) {
                let params = task_chunk
                    .iter()
                    .map(|(_, item)| serde_json::json!({"item":item}))
                    .collect();
                let responses = match self.request_batch("callHierarchy/outgoingCalls", params) {
                    Ok(responses) => responses,
                    Err(error) => {
                        self.record_optional_error(error);
                        return;
                    }
                };
                for ((owner, _), response) in task_chunk.iter().zip(responses) {
                    match response {
                        Ok(value) => collect_outgoing_calls(&value, root, &mut outputs[*owner]),
                        Err(error) => self.record_optional_error(error),
                    }
                }
                if self.fatal_error.is_some() {
                    return;
                }
            }

            for ((uri, line, character), output) in chunk.iter().zip(outputs) {
                self.outgoing_call_cache
                    .insert(format!("{uri}:{line}:{character}"), output);
            }
        }
    }

    pub(super) fn definitions_at(
        &mut self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Vec<(Option<String>, Vec<i32>)> {
        let cache_key = format!("{uri}:{line}:{character}");
        if let Some(cached) = self.definition_cache.get(&cache_key) {
            return cached.clone();
        }
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
            let results = parse_definition_locations(value);
            if !results.is_empty() {
                self.definition_cache.insert(cache_key, results.clone());
                return results;
            }
            if self.wait_for_retry(Duration::from_millis(250)).is_err() {
                return Vec::new();
            }
        }
        Vec::new()
    }

    /// Resolves independent definition positions in three provider rounds.
    /// Serial extraction previously slept and retried each empty position on
    /// its own. Grouping the same bounded retries lets a cold language server
    /// finish indexing once for the whole round while preserving the exact
    /// provider-only resolution rule.
    pub(super) fn prefetch_definitions(
        &mut self,
        queries: &[(String, u32, u32)],
        batch_size: usize,
    ) {
        self.prefetch_definitions_with_rounds(queries, batch_size, 3);
    }

    /// Resolves a definition work set exactly once. This is used only after a
    /// complete document-symbol census, where repeating every empty result
    /// cannot reveal more local source but can triple a large workspace's
    /// latency. Empty answers remain explicit cache entries, so later
    /// extraction cannot silently retry them one by one.
    pub(super) fn prefetch_definitions_once(
        &mut self,
        queries: &[(String, u32, u32)],
        batch_size: usize,
    ) {
        self.prefetch_definitions_with_rounds(queries, batch_size, 1);
    }

    fn prefetch_definitions_with_rounds(
        &mut self,
        queries: &[(String, u32, u32)],
        batch_size: usize,
        rounds: usize,
    ) {
        if queries.is_empty() || self.fatal_error.is_some() {
            return;
        }
        let mut seen = HashSet::new();
        let mut pending = queries
            .iter()
            .filter(|(uri, line, character)| {
                let key = format!("{uri}:{line}:{character}");
                !self.definition_cache.contains_key(&key) && seen.insert(key)
            })
            .cloned()
            .collect::<Vec<_>>();
        let width = batch_size.max(1);
        let rounds = rounds.max(1);
        for round in 0..rounds {
            let mut unresolved = Vec::new();
            for chunk in pending.chunks(width) {
                if self.remaining_request_budget() < chunk.len() {
                    for (uri, line, character) in &pending {
                        self.definition_cache
                            .entry(format!("{uri}:{line}:{character}"))
                            .or_default();
                    }
                    return;
                }
                let params = chunk
                    .iter()
                    .map(|(uri, line, character)| {
                        serde_json::json!({
                            "textDocument":{"uri":uri},
                            "position":{"line":line,"character":character}
                        })
                    })
                    .collect();
                let responses = match self.request_batch("textDocument/definition", params) {
                    Ok(responses) => responses,
                    Err(error) => {
                        self.record_optional_error(error);
                        return;
                    }
                };
                for ((uri, line, character), response) in chunk.iter().zip(responses) {
                    let cache_key = format!("{uri}:{line}:{character}");
                    match response {
                        Ok(value) => {
                            let locations = parse_definition_locations(value);
                            if locations.is_empty() {
                                unresolved.push((uri.clone(), *line, *character));
                            } else {
                                self.definition_cache.insert(cache_key, locations);
                            }
                        }
                        Err(error) => {
                            self.record_optional_error(error);
                            unresolved.push((uri.clone(), *line, *character));
                        }
                    }
                }
                if self.fatal_error.is_some() {
                    return;
                }
            }
            if unresolved.is_empty() {
                return;
            }
            if round + 1 < rounds {
                if self.wait_for_retry(Duration::from_millis(250)).is_err() {
                    return;
                }
                pending = unresolved;
            } else {
                for (uri, line, character) in unresolved {
                    self.definition_cache
                        .insert(format!("{uri}:{line}:{character}"), Vec::new());
                }
            }
        }
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

    pub(super) fn request_performance_summary(&self) -> Value {
        let mut methods = self.request_metrics.iter().collect::<Vec<_>>();
        methods.sort_unstable_by_key(|(method, _)| *method);
        serde_json::json!({
            "requestCount": self.request_count,
            "methods": methods
                .into_iter()
                .map(|(method, metric)| serde_json::json!({
                    "method": method,
                    "batches": metric.batches,
                    "requests": metric.requests,
                    "errors": metric.errors,
                    "wallMs": metric.total_wall_ms,
                    "maxBatchMs": metric.max_batch_ms,
                }))
                .collect::<Vec<_>>(),
        })
    }
}

fn parse_document_symbols(value: Value) -> Vec<LspSymbol> {
    let mut symbols = Vec::new();
    if let Some(items) = value.as_array() {
        for item in items {
            collect_lsp_symbols(item, &mut symbols);
        }
    }
    symbols
}

fn collect_outgoing_calls(
    calls: &Value,
    root: &Path,
    output: &mut Vec<(String, String, Vec<i32>)>,
) {
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

fn parse_definition_locations(value: Value) -> Vec<(Option<String>, Vec<i32>)> {
    let values = match value {
        Value::Array(values) => values,
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    values
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
        .collect()
}

pub(super) fn initialization_options(language: &str, workspace_settings: &Value) -> Value {
    match language {
        "java" => serde_json::json!({"settings": workspace_settings}),
        // rust-analyzer reads restart-only settings such as cargo.sysroot
        // before the later workspace/configuration exchange.
        "rust" => workspace_settings
            .get("rust-analyzer")
            .cloned()
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

impl Drop for LspConnection {
    fn drop(&mut self) {
        // Give the server a short grace period after the LSP exit notification.
        // Force-killing rust-analyzer while it is reloading can make it print a
        // misleading worker panic even though indexing succeeded.
        for _ in 0..100 {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => break,
            }
        }

        self.process_guard.terminate(&mut self.child);
    }
}

fn parse_lsp_reference(value: &Value) -> Option<LspReference> {
    Some(LspReference {
        uri: value.get("uri")?.as_str()?.to_string(),
        range: parse_lsp_range(value.get("range")?)?,
    })
}
