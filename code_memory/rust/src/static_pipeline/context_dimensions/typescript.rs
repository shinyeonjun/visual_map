use super::push_dimension;
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};

pub(super) fn collect(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    if !(name.ends_with(".json") && (name.starts_with("tsconfig") || name.starts_with("jsconfig")))
    {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(target) = value
        .pointer("/compilerOptions/target")
        .and_then(serde_json::Value::as_str)
    {
        push_dimension(output, ContextDimensionKind::Target, target);
    }
    if let Some(module) = value
        .pointer("/compilerOptions/module")
        .and_then(serde_json::Value::as_str)
    {
        push_dimension(output, ContextDimensionKind::ModuleMode, module);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_semantic_compiler_axes() {
        let mut output = Vec::new();
        collect(
            "tsconfig.json",
            r#"{"compilerOptions":{"target":"ES2022","module":"NodeNext","strict":true}}"#,
            &mut output,
        );
        assert_eq!(output.len(), 2);
        assert!(output
            .iter()
            .any(|item| item.kind == ContextDimensionKind::Target));
        assert!(output
            .iter()
            .any(|item| item.kind == ContextDimensionKind::ModuleMode));
    }
}
