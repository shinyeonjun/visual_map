use crate::{engine, EngineRegistry};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, time::Duration};

use super::store::engine_json_value;

const PRODUCT: &str = "database-memory";
const VERSION: &str = "0.2.0";
const CONTRACT_VERSION: &str = "2";

pub(crate) struct DatabaseMemoryAdapter<'a> {
    engine: &'a engine::EngineAvailability,
}

impl<'a> DatabaseMemoryAdapter<'a> {
    pub(crate) fn new(registry: &'a EngineRegistry) -> Result<Self, String> {
        let engine = registry
            .engines
            .iter()
            .find(|engine| engine.id == PRODUCT)
            .ok_or_else(|| "DB 읽기 도구가 등록되지 않았습니다".to_string())?;
        if engine.expected_version != VERSION || engine.contract_version != CONTRACT_VERSION {
            return Err(format!(
                "DB 읽기 도구 계약이 맞지 않습니다: expected {VERSION}/contract {CONTRACT_VERSION}, got {}/contract {}",
                engine.expected_version, engine.contract_version
            ));
        }

        Ok(Self { engine })
    }

    pub(crate) fn index(
        &self,
        args: &[String],
        envs: &[(&str, &str)],
        expected_snapshot_key: &str,
    ) -> Result<engine::EngineRunResult, String> {
        self.verify_runtime_contract()?;
        let run = if envs.is_empty() {
            engine::run_engine_command(self.engine, args, Duration::from_secs(120))?
        } else {
            engine::run_engine_command_with_env(self.engine, args, Duration::from_secs(120), envs)?
        };
        if run.ok {
            let value = response_json(&run, "DB 구조 읽기")?;
            validate_complete_index_response(&value, expected_snapshot_key)?;
        }
        Ok(run)
    }

    pub(crate) fn verify_complete_snapshot(
        &self,
        snapshot_key: &str,
        cache_path: &Path,
    ) -> Result<(), String> {
        self.verify_runtime_contract()?;
        let args = vec![
            "describe-snapshot".to_string(),
            snapshot_key.to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--cache-path".to_string(),
            cache_path.display().to_string(),
        ];
        let value = self.run_json(&args, Duration::from_secs(30), "DB snapshot 확인")?;
        validate_complete_snapshot_response(&value, snapshot_key)
    }

    pub(crate) fn inventory_page(
        &self,
        snapshot_key: &str,
        cache_path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<Value, String> {
        let args = vec![
            "inventory".to_string(),
            snapshot_key.to_string(),
            "--offset".to_string(),
            offset.to_string(),
            "--limit".to_string(),
            limit.to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--cache-path".to_string(),
            cache_path.display().to_string(),
        ];
        let value = self.run_json(&args, Duration::from_secs(120), "DB inventory")?;
        validate_inventory_page_contract(&value, snapshot_key, offset, limit)?;
        Ok(value)
    }

    fn verify_runtime_contract(&self) -> Result<(), String> {
        let args = ["contract", "--format", "json"].map(str::to_string);
        let value = self.run_json(&args, Duration::from_secs(10), "DB 엔진 계약 확인")?;
        validate_runtime_contract(&value)
    }

    fn run_json(
        &self,
        args: &[String],
        timeout: Duration,
        operation: &str,
    ) -> Result<Value, String> {
        let run = engine::run_engine_command(self.engine, args, timeout)?;
        if !run.ok {
            return Err(run_error(&run, operation));
        }
        response_json(&run, operation)
    }
}

fn response_json(run: &engine::EngineRunResult, operation: &str) -> Result<Value, String> {
    engine_json_value(&run.stdout)
        .ok_or_else(|| format!("{operation} 응답이 올바른 JSON이 아닙니다"))
}

fn run_error(run: &engine::EngineRunResult, operation: &str) -> String {
    if run.stderr.trim().is_empty() {
        format!("{operation}에 실패했습니다")
    } else {
        run.stderr.trim().to_string()
    }
}

pub(crate) fn validate_runtime_contract(value: &Value) -> Result<(), String> {
    require_string(value, "product", PRODUCT, "DB 엔진 product")?;
    require_string(value, "version", VERSION, "DB 엔진 version")?;
    require_contract_version(value, "contract_version", "DB 엔진 contract_version")?;
    require_contract_version(
        value,
        "complete_snapshot_contract_version",
        "DB 엔진 complete snapshot contract_version",
    )?;
    if value.get("metadata_only").and_then(Value::as_bool) != Some(true)
        || value.get("row_data_access").and_then(Value::as_bool) != Some(false)
    {
        return Err("DB 엔진이 metadata-only 읽기 계약을 보장하지 않습니다".to_string());
    }

    let outcomes = string_set(value.get("authoritative_outcomes"))?;
    if outcomes != BTreeSet::from(["complete", "failed"]) {
        return Err("DB 엔진 authoritative outcome 계약이 complete/failed가 아닙니다".to_string());
    }
    let commands = string_set(value.get("commands"))?;
    for command in ["contract", "index", "describe-snapshot", "inventory"] {
        if !commands.contains(command) {
            return Err(format!("DB 엔진 계약에 필수 명령이 없습니다: {command}"));
        }
    }
    Ok(())
}

pub(crate) fn validate_complete_index_response(
    value: &Value,
    expected_snapshot_key: &str,
) -> Result<(), String> {
    require_contract_version(value, "contract_version", "DB index contract_version")?;
    require_string(value, "status", "complete", "DB index status")?;
    require_string(
        value,
        "snapshot_key",
        expected_snapshot_key,
        "DB index snapshot_key",
    )?;
    let requested_source = value
        .get("requested_source")
        .and_then(Value::as_str)
        .ok_or_else(|| "DB index requested_source가 없습니다".to_string())?;
    require_string(
        value,
        "analyzed_source",
        requested_source,
        "DB index analyzed_source",
    )?;
    validate_completeness_certificate(
        value
            .get("completeness")
            .ok_or_else(|| "DB index completeness 인증서가 없습니다".to_string())?,
    )
}

pub(crate) fn validate_complete_snapshot_response(
    value: &Value,
    expected_snapshot_key: &str,
) -> Result<(), String> {
    let snapshot = value
        .get("snapshot")
        .ok_or_else(|| "DB snapshot 설명에 snapshot이 없습니다".to_string())?;
    require_contract_version(snapshot, "contract_version", "DB snapshot contract_version")?;
    require_string(snapshot, "authority", "complete", "DB snapshot authority")?;
    require_string(
        snapshot,
        "snapshot_key",
        expected_snapshot_key,
        "DB snapshot key",
    )?;
    validate_completeness_certificate(
        value
            .get("completeness")
            .ok_or_else(|| "DB snapshot completeness 인증서가 없습니다".to_string())?,
    )
}

pub(crate) fn validate_inventory_page_contract(
    value: &Value,
    expected_snapshot_key: &str,
    expected_offset: usize,
    expected_limit: usize,
) -> Result<(), String> {
    require_contract_version(value, "contract_version", "DB inventory contract_version")?;
    require_string(
        value,
        "snapshot_key",
        expected_snapshot_key,
        "DB inventory snapshot_key",
    )?;
    if require_usize(value, "offset", "DB inventory offset")? != expected_offset {
        return Err("DB inventory offset이 요청과 다릅니다".to_string());
    }
    if require_usize(value, "limit_requested", "DB inventory limit_requested")? != expected_limit {
        return Err("DB inventory 요청 한도가 응답과 다릅니다".to_string());
    }
    if value.get("limit_clamped").and_then(Value::as_bool) != Some(false) {
        return Err(
            "DB inventory 한도가 엔진에서 조정되어 완전성을 보장할 수 없습니다".to_string(),
        );
    }
    let tables = value
        .get("tables")
        .and_then(Value::as_array)
        .ok_or_else(|| "DB inventory tables 배열이 없습니다".to_string())?;
    let result_count = require_usize(value, "result_count", "DB inventory result_count")?;
    if result_count != tables.len() {
        return Err("DB inventory result_count와 tables 수가 다릅니다".to_string());
    }
    require_usize(value, "total_tables", "DB inventory total_tables")?;
    value
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or_else(|| "DB inventory truncated 값이 없습니다".to_string())?;
    value
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or_else(|| "DB inventory has_more 값이 없습니다".to_string())?;
    Ok(())
}

fn validate_completeness_certificate(value: &Value) -> Result<(), String> {
    require_string(value, "status", "complete", "DB completeness status")?;
    let adapter = value
        .get("adapter")
        .ok_or_else(|| "DB completeness adapter 정보가 없습니다".to_string())?;
    require_string(
        adapter,
        "version",
        VERSION,
        "DB completeness adapter version",
    )?;
    validate_count_certificate(value, "object_counts")?;
    validate_count_certificate(value, "relationship_counts")
}

fn validate_count_certificate(value: &Value, key: &str) -> Result<(), String> {
    let counts = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("DB completeness {key}가 없습니다"))?;
    for count in counts {
        let category = count
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let discovered = require_usize(count, "discovered", "DB completeness discovered")?;
        let emitted = require_usize(count, "emitted", "DB completeness emitted")?;
        if discovered != emitted {
            return Err(format!(
                "DB completeness {category} 수가 일치하지 않습니다: discovered {discovered}, emitted {emitted}"
            ));
        }
    }
    Ok(())
}

fn require_contract_version(value: &Value, key: &str, label: &str) -> Result<(), String> {
    let actual = value
        .get(key)
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|value| value.to_string()))
        })
        .ok_or_else(|| format!("{label}이 없습니다"))?;
    if actual == CONTRACT_VERSION {
        Ok(())
    } else {
        Err(format!(
            "{label}이 맞지 않습니다: expected {CONTRACT_VERSION}, got {actual}"
        ))
    }
}

fn require_string(value: &Value, key: &str, expected: &str, label: &str) -> Result<(), String> {
    let actual = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label}이 없습니다"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{label}이 맞지 않습니다: expected {expected}, got {actual}"
        ))
    }
}

fn require_usize(value: &Value, key: &str, label: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{label}이 없거나 올바르지 않습니다"))
}

fn string_set(value: Option<&Value>) -> Result<BTreeSet<&str>, String> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| "DB 엔진 문자열 배열 계약이 없습니다".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "DB 엔진 문자열 배열 계약이 올바르지 않습니다".to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn certificate() -> Value {
        json!({
            "status": "complete",
            "adapter": { "name": "database-memory-test", "version": "0.2.0" },
            "object_counts": [
                { "category": "table", "discovered": 2, "emitted": 2 }
            ],
            "relationship_counts": [
                { "category": "table_has_column", "discovered": 4, "emitted": 4 }
            ]
        })
    }

    #[test]
    fn runtime_contract_requires_metadata_only_complete_or_failed_v2() {
        let value = json!({
            "product": "database-memory",
            "version": "0.2.0",
            "contract_version": 2,
            "complete_snapshot_contract_version": 2,
            "metadata_only": true,
            "row_data_access": false,
            "authoritative_outcomes": ["complete", "failed"],
            "commands": ["contract", "index", "describe-snapshot", "inventory"]
        });

        assert!(validate_runtime_contract(&value).is_ok());
        let mut unsafe_contract = value;
        unsafe_contract["row_data_access"] = json!(true);
        assert!(validate_runtime_contract(&unsafe_contract).is_err());
    }

    #[test]
    fn index_contract_rejects_partial_or_source_substitution() {
        let complete = json!({
            "contract_version": 2,
            "status": "complete",
            "snapshot_key": "postgres:shop",
            "requested_source": "postgres",
            "analyzed_source": "postgres",
            "completeness": certificate()
        });
        assert!(validate_complete_index_response(&complete, "postgres:shop").is_ok());

        let mut partial = complete.clone();
        partial["status"] = json!("partial");
        assert!(validate_complete_index_response(&partial, "postgres:shop").is_err());

        let mut substituted = complete;
        substituted["analyzed_source"] = json!("odbc");
        assert!(validate_complete_index_response(&substituted, "postgres:shop").is_err());
    }

    #[test]
    fn snapshot_contract_rejects_legacy_or_non_authoritative_cache() {
        let complete = json!({
            "snapshot": {
                "contract_version": 2,
                "authority": "complete",
                "snapshot_key": "mysql:shop"
            },
            "completeness": certificate()
        });
        assert!(validate_complete_snapshot_response(&complete, "mysql:shop").is_ok());

        let mut legacy = complete;
        legacy["snapshot"]["contract_version"] = json!(1);
        legacy["snapshot"]["authority"] = json!("legacy_non_authoritative");
        assert!(validate_complete_snapshot_response(&legacy, "mysql:shop").is_err());
    }

    #[test]
    fn inventory_page_requires_exact_bounds_and_counts() {
        let page = json!({
            "contract_version": 2,
            "snapshot_key": "sqlite:shop",
            "offset": 0,
            "limit_requested": 1000,
            "limit_applied": 1000,
            "limit_clamped": false,
            "result_count": 1,
            "total_tables": 1,
            "truncated": false,
            "has_more": false,
            "next_offset": null,
            "tables": [{ "table_key": "sqlite:shop:main:main:table:orders" }]
        });
        assert!(validate_inventory_page_contract(&page, "sqlite:shop", 0, 1000).is_ok());

        let mut mismatched = page;
        mismatched["result_count"] = json!(2);
        assert!(validate_inventory_page_contract(&mismatched, "sqlite:shop", 0, 1000).is_err());
    }
}
