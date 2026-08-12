//! Codex를 제외한 정적 분석 단계별 시간 측정 기능이다.

use std::time::{Duration, Instant};

/// 파이프라인의 정적 단계만 측정하고 stderr로 출력한다.
#[derive(Debug)]
pub struct PipelineProfiler {
    enabled: bool,
    measured: Duration,
    codex_context_measured: Duration,
}

impl PipelineProfiler {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            measured: Duration::ZERO,
            codex_context_measured: Duration::ZERO,
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

    /// Codex CLI에 넘기기 전 후보정 컨텍스트 단계의 시간을 기록한다.
    /// 정적 파이프라인 누적 시간에는 포함하지 않아 두 시간을 분리해서 볼 수 있다.
    pub fn record_context_millis(&mut self, stage: &str, elapsed_ms: u64, details: String) {
        if !self.enabled {
            return;
        }
        let elapsed = Duration::from_millis(elapsed_ms);
        self.codex_context_measured += elapsed;
        eprintln!(
            "[profile] stage={stage} elapsed_ms={} codex_context_measured_ms={} {details}",
            elapsed.as_millis(),
            self.codex_context_measured.as_millis()
        );
    }

    /// 입력부터 Codex 후보정 JSON이 준비된 시점까지의 벽시계 시간을 기록한다.
    pub fn context_ready(&self, started: Instant) {
        if self.enabled {
            eprintln!(
                "[profile] codex_context_ready_elapsed_ms={} (input_to_codex_context)",
                started.elapsed().as_millis()
            );
        }
    }

    pub fn finish(&self) {
        if self.enabled {
            eprintln!(
                "[profile] static_pipeline_measured_ms={} (codex excluded)",
                self.measured.as_millis()
            );
            if self.codex_context_measured > Duration::ZERO {
                eprintln!(
                    "[profile] codex_context_postprocess_measured_ms={}",
                    self.codex_context_measured.as_millis()
                );
            }
        }
    }
}
