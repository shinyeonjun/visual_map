fn oracle_complete_capabilities(scope: &DictionaryScope) -> AdapterCapabilities {
    AdapterCapabilities {
        source_kind: ORACLE_SOURCE.to_owned(),
        metadata_only: true,
        schemas: true,
        tables: true,
        columns: true,
        constraints: true,
        indexes: true,
        views: CapabilitySupport::Supported,
        triggers: CapabilitySupport::Supported,
        routines: CapabilitySupport::Supported,
        dependencies: CapabilitySupport::Supported,
        limitations: Vec::new(),
        notes: vec![format!(
            "{}; unsupported Oracle object shapes fail the analysis instead of producing a partial snapshot",
            scope.mode.label()
        )],
    }
}
