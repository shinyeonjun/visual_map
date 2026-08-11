# Semantic naming contract

Status: active product contract, 2026-08-10.
Prompt policy: `base-semantic-policy-v7`.

## Product outcome

An area label is the first information a developer reads on the map. It must answer “what does this area own?” without
requiring the reader to know the repository's directory names. A structurally honest raw label is still preferable to a
plausible but unsupported business name.

Semantic analysis is not limited to explicit call edges. It must infer meaning from all supplied code signals while keeping
the strength of each conclusion within the strength of its evidence. “Do not guess” means “do not assert beyond the
evidence”; it does not mean “ignore paths, identifiers, configuration, comments, or other contextual signals.”

The 2026-08-10 product snapshot exposed the measured failure that this contract addresses:

- raw top-level labels such as `controllers` and `audio` remained visible even when their summaries described a coherent
  responsibility;
- sibling labels mixed abstraction levels;
- repeated wrappers such as `레거시`, `기반`, `기능`, and `구조` displaced the actual owned object or outcome;
- local partition names and final global reconciliation did not share one explicit naming rubric.

The subsequent repository audit exposed the opposite failure as well: a material explicit `legacy/` source tree was merged
into primary implementation areas because lifecycle words were suppressed globally. A lifecycle marker is not automatically
a responsibility, but a proven material lifecycle boundary is architecture and must remain visible.

## Prompt design basis

The implementation follows these prompt-design rules:

1. put trusted instructions before untrusted repository data and delimit the payload;
2. state the desired result, evidence boundary, length, style, and output shape concretely;
3. state positive evidence-combination and boundary-preservation behavior instead of accumulating prohibitions;
4. keep mechanical JSON/reference repair separate from semantic inference;
5. keep the evidence and naming contracts shared by local map and global reduce prompts;
6. make readability a tie-breaker rather than a target area count;
7. version and evaluate prompt changes against representative repository outputs.

References:

- <https://help.openai.com/en/articles/6654000-how-to-use-advanced-prompt-engineering>
- <https://developers.openai.com/api/docs/guides/latest-model>
- <https://openai.com/index/the-instruction-hierarchy/>

## Label formula

```text
L0 = shared owned capability or system
     + only the boundary qualifier needed to distinguish it

L1 = concrete object or resource
     + only the action or outcome needed to distinguish it
```

The selected label is the shortest candidate that satisfies all three conditions:

1. covers every assigned region, rather than only the largest file;
2. is supported by supplied evidence;
3. is distinguishable from every sibling without reading the summary.

## Evidence and inference

Every supplied repository signal may contribute evidence. Signals are weighted in this order:

1. direct behavior: ordered traces, explicit routes/jobs/events, database or external boundaries, confirmed relations, and
   executable configuration or annotations;
2. repeated implementation evidence: consistent public symbols, signatures, persisted resources, framework bindings, path
   roots, and independently verified local meanings;
3. contextual hints: one path segment, identifier, comment, docstring, or prose fragment;
4. exact structural fallback when the signals cannot support a semantic claim.

One decisive direct signal may support a narrow claim. Otherwise at least two independent, mutually consistent signals are
required. Repetition of one name at one source location remains one signal. File names, directory structure, and comments are
not discarded; they are interpreted together and checked against stronger behavior. When signals conflict, the result must
narrow the claim, split different responsibilities, or abstain with an exact structural fallback.

## Application and lifecycle boundaries

A material application, deployment, language-runtime, or lifecycle boundary is preserved when the supplied code facts make
it explicit and useful. Evidence includes a dedicated source root/application, repeated explicit path segments such as
`legacy` or `deprecated`, deprecation annotations/configuration, or multiple consistent compatibility symbols.

- A material explicit legacy/deprecated implementation and its primary implementation cannot share one leaf area.
- The same responsibility may use lifecycle-separated L1 children under one L0 responsibility.
- Independently runnable applications may use separate L0 systems.
- A small compatibility bridge may remain with its owner when it is not cohesive enough to become an area, but its role must
  remain visible in the summary or aliases.
- A single stale comment, generic word, or apparent code age never establishes lifecycle status.

## Language and style

- Labels, summaries, and warnings use the requested output language.
- Standard technical tokens such as HTTP, API, OAuth, WebRTC, and SQL may remain untranslated when translation reduces
  precision.
- Original identifiers belong in `aliases` when a semantic label is available.
- Labels normally use one to four Korean eojeol or one to four English content words.
- A proven lifecycle qualifier is retained when it distinguishes separated implementations. It is neither suppressed nor
  invented from age alone.
- Generic wrappers such as `기능`, `관련`, `기반`, `구조`, and `모듈` are replaced by the supported object, action, or
  outcome.
- `및`/`and` cannot be used merely to hide unrelated responsibilities; split when evidence supports a split.

Style examples are not repository facts:

| Weak label | Supported semantic style |
|---|---|
| `controllers` plus repeated authentication routes/symbols | `사용자 인증` / `User Authentication` |
| `services` plus repeated invoice jobs/resources | `청구 처리` / `Billing Processing` |
| material `legacy/frontend` beside primary frontend | lifecycle-separated UI areas |

## Abstention remains mandatory

Prompt quality must not weaken truthfulness. When the supplied evidence cannot support a responsibility-specific label, the
model must use the structural tuple:

```text
labelSource = structural
category = structural
fallbackReason = non-null
label = one assigned structuralLabel copied byte-for-byte
```

The UI may identify this as a structural fallback, but neither the AI nor deterministic code may beautify the guess.

## Pipeline scope

The same `EVIDENCE AND INFERENCE POLICY` and `NAMING CONTRACT` are injected into:

1. every local semantic partition;
2. the compact repository-wide global reconciliation;
3. verifier-guided repair through preservation of the original system policy.

The final global schema also describes `label` as a requested-language responsibility name and `aliases` as the location for
raw identifiers.

## Versioning and validation

Changing semantic inference or naming behavior changes the semantic input identity. `base-semantic-policy-v7` therefore
intentionally invalidates v6 partition and published-semantic cache matches while leaving old immutable records intact. The next analysis recomputes
semantic names from the existing static Fact snapshot; static code analysis does not need to run again unless its snapshot
changed.

Required gates:

- local and global prompts contain the same naming contract;
- local and global prompts contain the same evidence/lifecycle contract;
- prompt policy version is part of packet/cache identity;
- strict JSON schema and evidence verifier still pass;
- structural fallback remains legal and exact;
- representative repository review checks raw-container rate, generic-wrapper rate, sibling distinctness, evidence support,
  lifecycle mixing, hierarchy depth, and name/membership stability for the same Fact snapshot/model/policy.

### 2026-08-10 v5 real-model prompt evaluation

Authenticated Codex CLI evaluations were run outside normal CI against the reviewed two-region commerce fixture.

- Local semantic compilation identified the order creation responsibility and the session-token authentication
  responsibility; it did not fall back to `src/orders` or `src/auth`.
- Compact global reconciliation returned `사용자 인증` and `주문 처리`.
- Neither result used the measured vague suffixes `기능` or `기반`.
- Both outputs passed the normal strict JSON parser, evidence verifier, assignment coverage, and canonical approval path.

These evaluations prove the earlier v5 naming behavior on the reviewed fixture, not v7 lifecycle behavior or universal
semantic accuracy. New real-repository failures must add a frozen inference/boundary case without weakening structural
abstention.
