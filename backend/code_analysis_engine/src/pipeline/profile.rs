//! Codex를 제외한 정적 분석 단계별 시간 측정 기능이다.

use std::time::{Duration, Instant};

/// 파이프라인의 정적 단계만 측정하고 stderr로 출력한다.
#[derive(Debug)]
pub struct PipelineProfiler {
    enabled: bool,
    measured: Duration,
}

impl PipelineProfiler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            measured: Duration::ZERO,
        }
    }

    pub fn record(&mut self, stage: &str, started: Instant, details: String) {
        if !self.enabled {
            return;
        }
        let elapsed = started.elapsed();
        self.measured += elapsed;
        eprintln!(
            "[profile] stage={stage} elapsed_ms={} measured_total_ms={} {details}",
            elapsed.as_millis(),
            self.measured.as_millis()
        );
    }

    pub fn excluded(&self, stage: &str) {
        if self.enabled {
            eprintln!("[profile] stage={stage} elapsed_ms=excluded reason=codex");
        }
    }

    pub fn skipped(&self, stage: &str) {
        if self.enabled {
            eprintln!("[profile] stage={stage} elapsed_ms=skipped");
        }
    }

    pub fn finish(&self) {
        if self.enabled {
            eprintln!(
                "[profile] static_pipeline_measured_ms={} (codex excluded)",
                self.measured.as_millis()
            );
        }
    }
}
