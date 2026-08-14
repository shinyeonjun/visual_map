//! 정적 분석 파이프라인의 실행 단계.

/// `pipeline::runner`가 실제로 수행하는 정적 분석 단계다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticAnalysisStage {
    ProjectScan,
    FactExtraction,
    FrameworkDetection,
    StaticRelationGraph,
    ExecutionFlows,
    FeatureFirstDomainFormation,
    DomainRelationAggregation,
    OverviewProjection,
}

impl StaticAnalysisStage {
    pub const ORDER: [Self; 8] = [
        Self::ProjectScan,
        Self::FactExtraction,
        Self::FrameworkDetection,
        Self::StaticRelationGraph,
        Self::ExecutionFlows,
        Self::FeatureFirstDomainFormation,
        Self::DomainRelationAggregation,
        Self::OverviewProjection,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ProjectScan => "project_scan",
            Self::FactExtraction => "fact_extraction",
            Self::FrameworkDetection => "framework_detection",
            Self::StaticRelationGraph => "static_relation_graph",
            Self::ExecutionFlows => "execution_flows",
            Self::FeatureFirstDomainFormation => "feature_first_domain_formation",
            Self::DomainRelationAggregation => "domain_relation_aggregation",
            Self::OverviewProjection => "overview_projection",
        }
    }
}
