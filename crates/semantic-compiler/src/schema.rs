use codebase_semantic_model::BASE_SEMANTIC_SCHEMA_VERSION;
use serde_json::{json, Value};

pub fn base_semantic_output_schema() -> Value {
    let node_id = json!({"type":"string","pattern":"^node-[0-9a-f]{64}$"});
    let evidence_id = json!({"type":"string","pattern":"^evidence-[0-9a-f]{64}$"});
    let trace_id = json!({"type":"string","pattern":"^trace-[0-9a-f]{64}$"});
    let region_id = json!({"type":"string","pattern":"^region-[0-9a-f]{64}$"});
    let proposal_key = json!({"type":"string","pattern":"^[a-z][a-z0-9_-]{0,63}$"});

    json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "title": "Codebase Workspace Base Semantic Revision Proposal v2",
      "type": "object",
      "additionalProperties": false,
      "required": [
        "schemaVersion", "snapshotId", "semanticInputDigest", "project", "areas",
        "assignments", "unassignedRegions", "warnings"
      ],
      "properties": {
        "schemaVersion": {"type":"integer","enum":[BASE_SEMANTIC_SCHEMA_VERSION]},
        "snapshotId": {"type":"string","pattern":"^snapshot-[0-9a-f]{64}$"},
        "semanticInputDigest": {"type":"string","pattern":"^[0-9a-f]{64}$"},
        "project": {
          "type":"object",
          "additionalProperties":false,
          "required":["summary","aliases","representativeFactIds","evidenceIds"],
          "properties": {
            "summary":{"type":"string","minLength":1,"maxLength":300},
            "aliases":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":80}},
            "representativeFactIds":{"type":"array","maxItems":16,"items":node_id.clone()},
            "evidenceIds":{"type":"array","maxItems":24,"items":evidence_id.clone()}
          }
        },
        "areas": {
          "type":"array",
          "minItems":1,
          "maxItems":256,
          "items": {
            "type":"object",
            "additionalProperties":false,
            "required":[
              "proposalKey","parentProposalKey","level","label","summary","category",
              "representativeFactIds","representativeTracePathIds","evidenceIds","aliases",
              "labelSource","fallbackReason"
            ],
            "properties": {
              "proposalKey":proposal_key.clone(),
              "parentProposalKey":{"anyOf":[proposal_key,{"type":"null"}]},
              "level":{"type":"integer","enum":[0,1]},
              "label":{"type":"string","minLength":1,"maxLength":64},
              "summary":{"type":"string","minLength":1,"maxLength":300},
              "category":{"type":"string","enum":["domain","shared","infrastructure","integration","structural"]},
              "representativeFactIds":{"type":"array","maxItems":24,"items":node_id},
              "representativeTracePathIds":{"type":"array","maxItems":16,"items":trace_id},
              "evidenceIds":{"type":"array","maxItems":32,"items":evidence_id},
              "aliases":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":80}},
              "labelSource":{"type":"string","enum":["semantic","structural"]},
              "fallbackReason":{"anyOf":[
                {"type":"string","enum":["insufficient_semantic_signal","mixed_responsibility"]},
                {"type":"null"}
              ]}
            }
          }
        },
        "assignments": {
          "type":"array",
          "items": {
            "type":"object",
            "additionalProperties":false,
            "required":["regionId","areaProposalKey"],
            "properties":{"regionId":region_id.clone(),"areaProposalKey":proposal_key.clone()}
          }
        },
        "unassignedRegions": {
          "type":"array",
          "items": {
            "type":"object",
            "additionalProperties":false,
            "required":["regionId","reason"],
            "properties":{
              "regionId":region_id,
              "reason":{"type":"string","enum":["insufficient_input","mixed_responsibility"]}
            }
          }
        },
        "warnings":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":300}}
      }
    })
}
