//! 기본 설정 로딩과 TOML/JSON 부분 병합을 담당한다.

use super::policies::{
    AnalysisLimits, DomainPolicy, FrameworkPolicy, LanguageRegistry, ParserPolicy, PathPolicy,
    ScanPolicy, SemanticPolicy,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;

/// 전체 분석 정책이다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisConfig {
    pub scan: ScanPolicy,
    pub limits: AnalysisLimits,
    pub languages: LanguageRegistry,
    pub paths: PathPolicy,
    pub domains: DomainPolicy,
    pub parser: ParserPolicy,
    pub frameworks: FrameworkPolicy,
    pub semantic: SemanticPolicy,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("내장 기본 분석 설정이 올바른 TOML이어야 합니다")
    }
}

impl AnalysisConfig {
    /// 확장자에 맞는 설정 파일을 읽어 분석 정책을 교체한다.
    pub fn from_file(path: &Path) -> Result<Self, ConfigLoadError> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("json") => Self::from_json_file(path),
            _ => Self::from_toml_file(path),
        }
    }

    /// TOML 설정 파일을 읽어 분석 정책을 교체한다.
    pub fn from_toml_file(path: &Path) -> Result<Self, ConfigLoadError> {
        let source = std::fs::read_to_string(path).map_err(ConfigLoadError::Read)?;
        Self::from_toml_str(&source)
    }

    /// 기본 TOML과 사용자 TOML을 병합해 설정을 만든다.
    pub fn from_toml_str(source: &str) -> Result<Self, ConfigLoadError> {
        let mut base: toml::Value =
            toml::from_str(DEFAULT_CONFIG_TOML).map_err(ConfigLoadError::TomlParse)?;
        let overrides: toml::Value = toml::from_str(source).map_err(ConfigLoadError::TomlParse)?;
        merge_toml(&mut base, overrides);
        base.try_into().map_err(ConfigLoadError::TomlParse)
    }

    /// 기존 JSON 설정 파일과의 호환을 위해 JSON 로더도 유지한다.
    pub fn from_json_file(path: &Path) -> Result<Self, ConfigLoadError> {
        let source = std::fs::read_to_string(path).map_err(ConfigLoadError::Read)?;
        Self::from_json_str(&source)
    }

    /// 기존 JSON 설정과 기본 TOML을 병합한다.
    pub fn from_json_str(source: &str) -> Result<Self, ConfigLoadError> {
        let default_toml: toml::Value =
            toml::from_str(DEFAULT_CONFIG_TOML).map_err(ConfigLoadError::TomlParse)?;
        let mut base = serde_json::to_value(default_toml).map_err(ConfigLoadError::JsonConvert)?;
        let overrides: serde_json::Value =
            serde_json::from_str(source).map_err(ConfigLoadError::JsonParse)?;
        merge_json(&mut base, overrides);
        serde_json::from_value(base).map_err(ConfigLoadError::JsonParse)
    }
}

pub(crate) const DEFAULT_CONFIG_TOML: &str = include_str!("../../config/analysis.default.toml");

#[derive(Debug)]
pub enum ConfigLoadError {
    Read(std::io::Error),
    TomlParse(toml::de::Error),
    JsonParse(serde_json::Error),
    JsonConvert(serde_json::Error),
}

impl std::fmt::Display for ConfigLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "설정 파일을 읽지 못했습니다: {error}"),
            Self::TomlParse(error) => write!(formatter, "설정 TOML을 해석하지 못했습니다: {error}"),
            Self::JsonParse(error) => write!(formatter, "설정 JSON을 해석하지 못했습니다: {error}"),
            Self::JsonConvert(error) => write!(
                formatter,
                "기본 설정을 JSON으로 변환하지 못했습니다: {error}"
            ),
        }
    }
}

impl std::error::Error for ConfigLoadError {}

pub(crate) fn default_section<T>(section: &str) -> T
where
    T: DeserializeOwned,
{
    let document: toml::Value = toml::from_str(DEFAULT_CONFIG_TOML)
        .expect("내장 기본 분석 설정이 올바른 TOML이어야 합니다");
    document
        .get(section)
        .cloned()
        .and_then(|value| value.try_into().ok())
        .unwrap_or_else(|| panic!("내장 기본 분석 설정에 [{section}] 섹션이 없습니다"))
}

fn merge_toml(base: &mut toml::Value, overrides: toml::Value) {
    let current = std::mem::replace(base, toml::Value::String(String::new()));
    match (current, overrides) {
        (toml::Value::Table(mut base_table), toml::Value::Table(overrides)) => {
            for (key, value) in overrides {
                if let Some(base_value) = base_table.get_mut(&key) {
                    merge_toml(base_value, value);
                } else {
                    base_table.insert(key, value);
                }
            }
            *base = toml::Value::Table(base_table);
        }
        (_, overrides) => *base = overrides,
    }
}

fn merge_json(base: &mut serde_json::Value, overrides: serde_json::Value) {
    let current = std::mem::replace(base, serde_json::Value::Null);
    match (current, overrides) {
        (serde_json::Value::Object(mut base_object), serde_json::Value::Object(overrides)) => {
            for (key, value) in overrides {
                if let Some(base_value) = base_object.get_mut(&key) {
                    merge_json(base_value, value);
                } else {
                    base_object.insert(key, value);
                }
            }
            *base = serde_json::Value::Object(base_object);
        }
        (_, overrides) => *base = overrides,
    }
}
