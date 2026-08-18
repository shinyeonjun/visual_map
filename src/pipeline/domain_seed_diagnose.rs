//! domain seed diagnostics 실행.

use crate::domain::analyze_domain_seeds;
use crate::domain::DomainSeedDiagnostics;
use crate::model::AnalysisRequest;
use crate::EngineError;

use super::DomainAnalysisPipeline;

impl DomainAnalysisPipeline {
    pub fn diagnose_domain_seeds(
        &self,
        request: AnalysisRequest,
    ) -> Result<DomainSeedDiagnostics, EngineError> {
        let bundle = self.build_fact_bundle(&request)?;
        Ok(analyze_domain_seeds(
            &bundle.facts,
            &bundle.execution_flows,
            &request.options.config.domains,
            &request.options.config.paths,
        ))
    }
}
