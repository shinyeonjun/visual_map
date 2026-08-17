//! NestJS GraphQL `@Query`·`@Mutation` adapter.

use crate::facts::FactStore;
use crate::frameworks::common::graphql::add_graphql_resolver_routes;
use crate::frameworks::registry::detector::FrameworkDetection;

pub(super) fn enrich(facts: &mut FactStore, detections: &[FrameworkDetection]) {
    add_graphql_resolver_routes(facts, detections, "javascript.nestjs_graphql");
}
