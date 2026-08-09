# Codebase Semantic Model

This crate owns provider-neutral contracts for the AI-derived semantic layer.
It is downstream of `codebase-fact-model`: every referenced region, fact,
edge, trace, bundle, and evidence ID must already exist in a published static
snapshot.

The first implemented task is base semantic compilation only:

- L0/L1 semantic areas;
- short labels and present-tense responsibilities;
- one primary area assignment per static region;
- explicit structural fallback when semantic evidence is insufficient.

It deliberately contains no provider process, prompt, persistence, canvas,
chat, candidate relation, or source-reading behavior. Numeric AI confidence is
not part of the contract.
