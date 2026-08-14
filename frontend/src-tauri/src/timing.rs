use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisTimingRecord {
    pub(crate) schema_version: u8,
    pub(crate) project_path: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) started_at_ms: u128,
    pub(crate) finished_at_ms: Option<u128>,
    pub(crate) status: String,
    pub(crate) total_elapsed_ms: Option<u128>,
    pub(crate) steps: Vec<AnalysisStepRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisStepRecord {
    pub(crate) step: u8,
    pub(crate) phase: String,
    pub(crate) label: String,
    pub(crate) elapsed_ms: u128,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) engine_profile: Vec<String>,
}

pub(crate) struct AnalysisTimingLog {
    json_path: PathBuf,
    text_path: PathBuf,
    project_path: String,
    provider: String,
    model: String,
    started_at_ms: u128,
    run_started: Instant,
    steps: Vec<AnalysisStepRecord>,
    active_step: Option<ActiveStep>,
    status: String,
    error: Option<String>,
}

struct ActiveStep {
    step: u8,
    phase: String,
    label: String,
    started: Instant,
    engine_profile: Vec<String>,
}

impl AnalysisTimingLog {
    pub(crate) fn new(
        workspace_directory: &Path,
        project_path: &str,
        provider: &str,
        model: &str,
    ) -> Self {
        let analysis_directory = workspace_directory.join("analysis");
        Self {
            json_path: analysis_directory.join("analysis-timing.json"),
            text_path: analysis_directory.join("analysis-timing.log"),
            project_path: project_path.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            started_at_ms: now_ms(),
            run_started: Instant::now(),
            steps: Vec::new(),
            active_step: None,
            status: "running".to_string(),
            error: None,
        }
    }

    pub(crate) fn begin_step(&mut self, step: u8, phase: &str, label: &str) {
        self.finish_active_step();
        self.active_step = Some(ActiveStep {
            step,
            phase: phase.to_string(),
            label: label.to_string(),
            started: Instant::now(),
            engine_profile: Vec::new(),
        });
        let _ = self.flush();
    }

    pub(crate) fn append_engine_profile<I>(&mut self, lines: I)
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let Some(active) = self.active_step.as_mut() else {
            return;
        };
        for line in lines {
            let line = line.as_ref();
            if line.contains("[profile]") {
                active.engine_profile.push(line.to_string());
            }
        }
    }

    pub(crate) fn complete(&mut self, status: &str, error: Option<String>) -> Result<(), String> {
        self.finish_active_step();
        self.status = status.to_string();
        self.error = error;
        self.flush()
    }

    fn finish_active_step(&mut self) {
        let Some(active) = self.active_step.take() else {
            return;
        };
        self.steps.push(AnalysisStepRecord {
            step: active.step,
            phase: active.phase,
            label: active.label,
            elapsed_ms: active.started.elapsed().as_millis(),
            engine_profile: active.engine_profile,
        });
    }

    pub(crate) fn flush(&self) -> Result<(), String> {
        if let Some(parent) = self.json_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("분석 타이밍 로그 폴더를 만들지 못했습니다: {error}"))?;
        }

        let finished_at_ms = if self.status == "running" {
            None
        } else {
            Some(now_ms())
        };
        let total_elapsed_ms = if self.status == "running" {
            Some(self.run_started.elapsed().as_millis())
        } else {
            Some(self.run_started.elapsed().as_millis())
        };

        let mut steps = self.steps.clone();
        if let Some(active) = &self.active_step {
            steps.push(AnalysisStepRecord {
                step: active.step,
                phase: active.phase.clone(),
                label: active.label.clone(),
                elapsed_ms: active.started.elapsed().as_millis(),
                engine_profile: active.engine_profile.clone(),
            });
        }

        let record = AnalysisTimingRecord {
            schema_version: 1,
            project_path: self.project_path.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            started_at_ms: self.started_at_ms,
            finished_at_ms,
            status: self.status.clone(),
            total_elapsed_ms,
            steps,
            error: self.error.clone(),
        };

        let json = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("분석 타이밍 JSON을 만들지 못했습니다: {error}"))?;
        fs::write(&self.json_path, json)
            .map_err(|error| format!("분석 타이밍 JSON을 저장하지 못했습니다: {error}"))?;

        let mut text = format!(
            "analysis started project={} provider={} model={}\n",
            self.project_path, self.provider, self.model
        );
        for step in &record.steps {
            text.push_str(&format_step_line(step));
            for profile in &step.engine_profile {
                text.push_str(&format!("  {profile}\n"));
            }
        }
        if self.status != "running" {
            if let Some(total) = record.total_elapsed_ms {
                text.push_str(&format!("total: {:.2}s status={}\n", ms_to_seconds(total), self.status));
            }
            if let Some(error) = &self.error {
                text.push_str(&format!("error: {error}\n"));
            }
        } else if let Some(active) = &self.active_step {
            text.push_str(&format!(
                "step {} {}: {:.2}s (in progress)\n",
                active.step,
                active.phase,
                ms_to_seconds(active.started.elapsed().as_millis())
            ));
        }
        fs::write(&self.text_path, text)
            .map_err(|error| format!("분석 타이밍 로그를 저장하지 못했습니다: {error}"))?;
        Ok(())
    }
}

fn format_step_line(step: &AnalysisStepRecord) -> String {
    format!(
        "step {} {} ({}): {:.2}s\n",
        step.step,
        step.phase,
        step.label,
        ms_to_seconds(step.elapsed_ms)
    )
}

fn ms_to_seconds(milliseconds: u128) -> f64 {
    milliseconds as f64 / 1000.0
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{format_step_line, AnalysisStepRecord};

    #[test]
    fn formats_step_duration_in_seconds() {
        let line = format_step_line(&AnalysisStepRecord {
            step: 2,
            phase: "static".into(),
            label: "정적 분석".into(),
            elapsed_ms: 1530,
            engine_profile: Vec::new(),
        });
        assert!(line.contains("step 2 static"));
        assert!(line.contains("1.53s"));
    }
}
