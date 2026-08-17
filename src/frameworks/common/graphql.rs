//! NestJS GraphQL `@Query`·`@Mutation` resolver를 HTTP 계약으로 materialize한다.
//!
//! GraphQL HTTP endpoint는 단일 POST 경로지만, resolver field는
//! `/{apiPath}/{fieldName}` 형태의 논리 계약으로 분리해 도메인·기능
//! 분석에 사용한다.

use crate::facts::{CodeUnitKind, Entrypoint, EntrypointKind, Evidence, FactStore, SourceSpan};
use crate::frameworks::registry::detector::FrameworkDetection;
use crate::languages::common::metadata::stable_id;
use std::collections::{HashMap, HashSet};

use super::FrameworkApplicabilityIndex;

const QUERY_DECORATORS: &[&str] = &["Query"];
const MUTATION_DECORATORS: &[&str] = &["Mutation"];

/// NestJS GraphQL resolver decorator를 HTTP entrypoint로 변환한다.
pub fn add_graphql_resolver_routes(
    facts: &mut FactStore,
    detections: &[FrameworkDetection],
    framework_id: &str,
) {
    if !detections
        .iter()
        .any(|detection| detection.id == framework_id)
    {
        return;
    }
    let applicability = FrameworkApplicabilityIndex::new(detections);
    let mut api_prefixes = HashSet::new();
    let mut additions: Vec<Entrypoint> = Vec::new();
    let mut base_units: HashMap<String, String> = HashMap::new();

    for decorator in facts.decorators.clone() {
        let is_query = QUERY_DECORATORS
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&decorator.name));
        let is_mutation = MUTATION_DECORATORS
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&decorator.name));
        if !is_query && !is_mutation {
            continue;
        }
        let target_units = resolver_target_units(facts, &decorator);
        for (unit_id, field_name) in target_units {
            let Some(unit) = facts.unit(&unit_id) else {
                continue;
            };
            if !applicability.applies(facts, &unit_id, framework_id)
                && graphql_api_prefix(&unit.relative_path).is_none()
            {
                continue;
            }
            let Some(api_prefix) = graphql_api_prefix(&unit.relative_path) else {
                continue;
            };
            api_prefixes.insert(api_prefix.clone());
            base_units
                .entry(api_prefix.clone())
                .or_insert_with(|| unit_id.clone());
            let path = format!("/{api_prefix}/{field_name}");
            let id = stable_id(
                "entry",
                &format!("{}:graphql:{}:{}", decorator.id, framework_id, path),
            );
            if facts.entrypoints.iter().any(|entrypoint| entrypoint.id == id) {
                continue;
            }
            additions.push(Entrypoint {
                id,
                unit_id: unit_id.clone(),
                kind: EntrypointKind::Http,
                name: path.clone(),
                method: Some("POST".to_string()),
                path: Some(path),
                framework_id: Some(framework_id.to_string()),
                evidence: decorator
                    .evidence
                    .iter()
                    .cloned()
                    .map(|mut evidence| {
                        evidence.kind = "graphqlResolver".to_string();
                        evidence.value = field_name.clone();
                        evidence
                    })
                    .collect::<Vec<_>>(),
            });
        }
    }

    for api_prefix in api_prefixes {
        let path = format!("/{api_prefix}");
        let id = stable_id("entry", &format!("graphql-base:{framework_id}:{path}"));
        if facts.entrypoints.iter().any(|entrypoint| {
            entrypoint.id == id
                || entrypoint.path.as_deref() == Some(path.as_str())
                    && entrypoint.method.as_deref() == Some("POST")
        }) {
            continue;
        }
        additions.push(Entrypoint {
            id,
            unit_id: base_units
                .get(&api_prefix)
                .cloned()
                .unwrap_or_else(|| "graphql:base".into()),
            kind: EntrypointKind::Http,
            name: path.clone(),
            method: Some("POST".to_string()),
            path: Some(path),
            framework_id: Some(framework_id.to_string()),
            evidence: vec![Evidence::new(
                "graphqlApi",
                &api_prefix,
                SourceSpan::new("graphql", &api_prefix, 1, 1, 1, 1),
            )],
        });
    }

    facts.entrypoints.extend(additions);
}

fn graphql_api_prefix(relative_path: &str) -> Option<String> {
    let path = relative_path.replace('\\', "/").to_ascii_lowercase();
    if path.contains("/resolvers/shop/") || path.contains("/resolver/shop/") {
        return Some("shop-api".into());
    }
    if path.contains("/resolvers/admin/") || path.contains("/resolver/admin/") {
        return Some("admin-api".into());
    }
    if path.contains("/shop/") && path.contains("resolver") {
        return Some("shop-api".into());
    }
    if path.contains("/admin/") && path.contains("resolver") {
        return Some("admin-api".into());
    }
    None
}

fn graphql_field_name(decorator: &crate::facts::DecoratorFact, fallback: &str) -> String {
    literal_argument(decorator.arguments.first().map(String::as_str))
        .unwrap_or_else(|| fallback.to_string())
}

fn resolver_target_units(
    facts: &FactStore,
    decorator: &crate::facts::DecoratorFact,
) -> Vec<(String, String)> {
    let Some(unit) = facts.unit(&decorator.unit_id) else {
        return Vec::new();
    };
    if matches!(unit.kind, CodeUnitKind::Method | CodeUnitKind::Function) {
        return vec![(
            unit.id.clone(),
            graphql_field_name(decorator, &unit.name),
        )];
    }
    if unit.kind != CodeUnitKind::Class {
        return Vec::new();
    }
    let decorator_line = decorator
        .evidence
        .first()
        .map(|evidence| evidence.span.start_line)
        .unwrap_or(0);
    let children = facts
        .units
        .values()
        .filter(|candidate| candidate.parent_id.as_deref() == Some(unit.id.as_str()))
        .filter(|candidate| {
            matches!(
                candidate.kind,
                CodeUnitKind::Method | CodeUnitKind::Function
            )
        })
        .collect::<Vec<_>>();
    if let Some(method) = children
        .iter()
        .filter(|candidate| candidate.span.start_line >= decorator_line)
        .min_by_key(|candidate| candidate.span.start_line)
    {
        let field_name = facts
            .decorators
            .iter()
            .filter(|item| item.unit_id == method.id)
            .find(|item| {
                QUERY_DECORATORS
                    .iter()
                    .chain(MUTATION_DECORATORS.iter())
                    .any(|name| name.eq_ignore_ascii_case(&item.name))
            })
            .map(|item| graphql_field_name(item, &method.name))
            .unwrap_or_else(|| graphql_field_name(decorator, &method.name));
        return vec![(method.id.clone(), field_name)];
    }
    Vec::new()
}

fn literal_argument(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.len() < 2 {
        return None;
    }
    let first = value.chars().next()?;
    let last = value.chars().last()?;
    matches!((first, last), ('"', '"') | ('\'', '\''))
        .then(|| value[1..value.len() - 1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{CodeUnit, DecoratorFact, SourceSpan};
    use crate::frameworks::registry::capabilities::FrameworkKind;
    use crate::frameworks::registry::detector::FrameworkDetection;
    use crate::model::Language;

    fn detection(file_id: &str) -> Vec<FrameworkDetection> {
        vec![FrameworkDetection {
            id: "javascript.nestjs_graphql".into(),
            display_name: "NestJS GraphQL".into(),
            kind: FrameworkKind::Web,
            capabilities: vec![],
            parent: None,
            languages: vec!["typescript".into()],
            confidence: 1.0,
            evidence: vec![crate::facts::Evidence::new(
                "marker",
                "@nestjs/graphql",
                SourceSpan::new(file_id, file_id, 1, 1, 1, 1),
            )],
        }]
    }

    fn resolver_method(id: &str, name: &str, path: &str) -> CodeUnit {
        CodeUnit {
            id: id.into(),
            kind: CodeUnitKind::Method,
            name: name.into(),
            qualified_name: format!("ShopAuthResolver::{name}"),
            file_id: format!("file:{id}"),
            relative_path: path.into(),
            language: Language::TypeScript,
            parent_id: Some("class:resolver".into()),
            span: SourceSpan::new(format!("file:{id}"), path, 1, 1, 1, 1),
            body_span: None,
            signature: Some(format!("async {name}()")),
            parameters: Vec::new(),
            return_type: None,
            visibility: Default::default(),
            modifiers: Vec::new(),
            exported: true,
        }
    }

    #[test]
    fn shop_resolver_mutation은_shop_api_계약이_된다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "method:register".into(),
            resolver_method(
                "method:register",
                "registerCustomerAccount",
                "packages/core/src/api/resolvers/shop/shop-auth.resolver.ts",
            ),
        );
        facts.decorators.push(DecoratorFact {
            id: "decorator:mutation".into(),
            unit_id: "method:register".into(),
            receiver: None,
            name: "Mutation".into(),
            arguments: Vec::new(),
            expression: "Mutation()".into(),
            evidence: Vec::new(),
        });

        add_graphql_resolver_routes(
            &mut facts,
            &detection("file:method:register"),
            "javascript.nestjs_graphql",
        );

        assert!(facts.entrypoints.iter().any(|entrypoint| {
            entrypoint.path.as_deref() == Some("/shop-api/registerCustomerAccount")
                && entrypoint.method.as_deref() == Some("POST")
        }));
        assert!(facts
            .entrypoints
            .iter()
            .any(|entrypoint| entrypoint.path.as_deref() == Some("/shop-api")));
    }

    #[test]
    fn typescript_resolver_fixture에서_mutation_decorator를_읽는다() {
        use crate::config::ParserPolicy;
        use crate::languages::common::LanguageAnalyzer;
        use crate::languages::ecma::common::typescript;
        use crate::model::FileEntry;

        let source = r#"
import { Mutation, Resolver } from "@nestjs/graphql";

@Resolver()
export class ShopAuthResolver {
  @Mutation()
  async registerCustomerAccount() {
    return {};
  }
}
"#;
        let file = FileEntry {
            file_id: "file:resolver".into(),
            relative_path: "api/resolvers/shop/shop-auth.resolver.ts".into(),
            language: Language::TypeScript,
            size_bytes: source.len() as u64,
            line_count: source.lines().count() as u64,
            modified_unix_ms: None,
            content_hash: None,
            is_test: false,
            parse_status: Default::default(),
        };
        let bundle = typescript().analyze(&file, source, &ParserPolicy::default());
        let mutation = bundle
            .decorators
            .iter()
            .find(|decorator| decorator.name == "Mutation")
            .expect("Mutation decorator가 있어야 한다");
        let unit = bundle
            .units
            .iter()
            .find(|unit| unit.id == mutation.unit_id)
            .expect("decorator 대상 유닛이 있어야 한다");
        assert_eq!(unit.name, "ShopAuthResolver");
    }

    #[test]
    fn admin_resolver_query는_admin_api_계약이_된다() {
        let mut facts = FactStore::default();
        facts.units.insert(
            "method:products".into(),
            resolver_method(
                "method:products",
                "products",
                "packages/core/src/api/resolvers/admin/product.resolver.ts",
            ),
        );
        facts.decorators.push(DecoratorFact {
            id: "decorator:query".into(),
            unit_id: "method:products".into(),
            receiver: None,
            name: "Query".into(),
            arguments: Vec::new(),
            expression: "Query()".into(),
            evidence: Vec::new(),
        });

        add_graphql_resolver_routes(
            &mut facts,
            &detection("file:method:products"),
            "javascript.nestjs_graphql",
        );

        assert!(facts.entrypoints.iter().any(|entrypoint| {
            entrypoint.path.as_deref() == Some("/admin-api/products")
                && entrypoint.method.as_deref() == Some("POST")
        }));
    }
}
