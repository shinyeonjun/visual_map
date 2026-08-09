//! Deterministic static-analysis preparation before any language provider runs.

pub(crate) mod analysis_unit_planner;
pub(crate) mod canonical;
pub(crate) mod context_dimensions;
pub(crate) mod framework_ir;
pub(crate) mod language_ir;
pub(crate) mod provider_schedule;
pub(crate) mod source_census;
pub(crate) mod source_evidence;
pub(crate) mod test_ir;

#[cfg(test)]
mod tests;
