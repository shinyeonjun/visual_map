//! capability pair rejection 진단 실행.

use crate::config::DomainClusteringMode;
use crate::domain::{CapabilityPairDiagnostics, DomainAnalyzer};
use crate::model::AnalysisRequest;
use crate::EngineError;

use super::fact_bundle::FactBundle;
use super::DomainAnalysisPipeline;

impl DomainAnalysisPipeline {
    pub fn diagnose_formation_pairs(
        &self,
        request: AnalysisRequest,
        modes: &[DomainClusteringMode],
        top_k: usize,
    ) -> Result<Vec<CapabilityPairDiagnostics>, EngineError> {
        let bundle = self.build_fact_bundle(&request)?;
        let analyzer = DomainAnalyzer::new(
            request.options.config.domains.clone(),
            request.options.config.paths.clone(),
        );
        Ok(diagnose_modes(&analyzer, &bundle, modes, top_k))
    }
}

fn diagnose_modes(
    analyzer: &DomainAnalyzer,
    bundle: &FactBundle,
    modes: &[DomainClusteringMode],
    top_k: usize,
) -> Vec<CapabilityPairDiagnostics> {
    modes
        .iter()
        .map(|mode| {
            analyzer.diagnose_capability_pairs(
                &bundle.facts,
                &bundle.execution_flows,
                *mode,
                top_k,
            )
        })
        .collect()
}
