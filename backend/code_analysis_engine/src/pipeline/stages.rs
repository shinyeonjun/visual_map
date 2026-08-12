//! 1단계 도메인 파이프라인의 고정 실행 순서.

/// 각 단계는 이전 단계가 만든 사실을 소비하고 다음 단계의 입력을 만든다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainAnalysisStage {
    ProjectConnection,
    FileInventory,
    LanguageDetection,
    AstFacts,
    LanguageFamilyFacts,
    LanguageSpecificFacts,
    FrameworkDetection,
    FrameworkFacts,
    FactsMerge,
    StaticRelationGraph,
    DomainCandidates,
    DomainGrouping,
    CodexSemanticReview,
    AiValidationAndMerge,
    DomainRelationAggregation,
    OverviewProjection,
}

impl DomainAnalysisStage {
    pub const ORDER: [Self; 16] = [
        Self::ProjectConnection,
        Self::FileInventory,
        Self::LanguageDetection,
        Self::AstFacts,
        Self::LanguageFamilyFacts,
        Self::LanguageSpecificFacts,
        Self::FrameworkDetection,
        Self::FrameworkFacts,
        Self::FactsMerge,
        Self::StaticRelationGraph,
        Self::DomainCandidates,
        Self::DomainGrouping,
        Self::CodexSemanticReview,
        Self::AiValidationAndMerge,
        Self::DomainRelationAggregation,
        Self::OverviewProjection,
    ];
}
