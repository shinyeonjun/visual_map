# Backend Visual Map Developer Value Direction

Status: Product direction research
Scale: Large
Date: 2026-07-07

## Cold Read

If I were a backend developer, I would not use this because it is a pretty graph.
I would use it if it reliably answers three questions faster than my IDE plus grep:

1. Where do I start reading for this API or feature?
2. What code and DB objects are likely involved?
3. If I change this table or column, what might break, and why does the tool think so?

The current product is close to being useful for onboarding and demo review, but not yet sticky for daily work. The engine path is real, the privacy posture is strong, and the Workbench/Atlas IA is pointed in the right direction. The weak part is the final answer layer: the map still makes the user interpret too much instead of turning the graph into a ranked reading path, impact board, and evidence ledger.

## Current Product Facts

From current repo state:

- `README.md` defines the product as a Windows-first Tauri + React desktop app that indexes a Git repo and relational DB metadata with bundled `codebase-memory-mcp.exe` and `database-memory.exe`.
- `src-tauri/src/workspace/code.rs` indexes code, loads architecture/routes/services/files, and ingests `CALLS` through `query_graph` when available.
- `src-tauri/src/atlas/visual_map.rs` already has mode projections: atlas overview, API flow, table usage, column impact, and search-focus fallback.
- `src-tauri/src/atlas/linker.rs` still uses simple name matching for code-to-table candidates.
- `src/components/atlas/AtlasCanvas.tsx` currently renders inventory-derived API/code/DB bands and overlays candidate/FK hints, but it does not yet fully render the backend `VisualMap` node-edge evidence model.
- `src/components/workbench/InspectorPanel.tsx` separates confirmed, inferred, and candidate relationships and exposes copy actions.
- `docs/reports/backend-visual-map.qa-e2e.md` says the app is demo-usable, but still lacks search empty-state strength, Atlas group drilldown, better candidate evidence, and release license notices.

Interpretation:

- The product has a trustworthy local foundation.
- The remaining value gap is not "more graph"; it is "less interpretation work for the developer."

## External Evidence

### Developer Questions Are Concrete, Not Decorative

Sillito, Murphy, and De Volder cataloged 44 kinds of questions programmers ask during software evolution. The high-signal questions map directly to this product: where code is involved in behavior, where a method is called, where data is accessed or modified, how control gets from here to there, and what the direct/total impact of a change will be.

Source: Questions Programmers Ask During Software Evolution Tasks
https://dl.acm.org/doi/10.1145/1181775.1181779

### Reachability Is The Hard, Valuable Problem

LaToza and Myers define reachability questions as searches across feasible program paths. In their studies, professional developers reported asking such questions more than nine times a day; 82% rated at least one as hard to answer; and 9 of the 10 longest observed activities involved reachability questions.

Source: Developers Ask Reachability Questions
https://dl.acm.org/doi/10.1145/1806799.1806829

Product implication:

- API Flow and Impact modes should be treated as the core product, not secondary visual modes.
- A good answer is a bounded path with evidence, not a whole-codebase diagram.

### Developers Need Seek, Relate, Collect

Ko, Myers, Coblenz, and Aung observed developers seeking relevant code, following dependencies, and collecting information to use later. Developers spent an average of 35% of their time on navigation mechanics, and frequently lost track of relevant code as IDE state changed.

Source: An Exploratory Study of How Developers Seek, Relate, and Collect Relevant Information during Software Maintenance Tasks
https://faculty.washington.edu/ajko/papers/Ko2006SeekRelateCollect.pdf

Product implication:

- Backend Visual Map needs a persistent "reading path / evidence tray / saved trail" more than another canvas effect.
- Copy buttons are good; open-in-editor and saved investigation trails would be much better.

### Existing Tools Validate Navigation, References, And Impact

GitHub code navigation frames value as definitions and references for understanding code relationships.
Source: https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github

Sourcegraph Code Navigation sells onboarding, unfamiliar-code review, impact confidence, and root-cause tracing.
Source: https://sourcegraph.com/docs/code-navigation

JetBrains Code Vision puts usages, inheritors, returning APIs, and VCS info directly near declarations.
Source: https://www.jetbrains.com/help/rider/Code_Vision.html

CodeScene hotspots combine code health with temporal and organizational data to prioritize interesting areas.
Source: https://codescene.io/docs/guides/technical/hotspots.html

Product implication:

- Users pay attention when the UI reduces switching and ranks what matters.
- "Impact" needs priority and confidence, not just connectivity.

### Complex UI Must Hide Secondary Controls

NN/g progressive disclosure recommends deferring advanced or rare controls to reduce learning errors.
Source: https://www.nngroup.com/articles/progressive-disclosure/

NN/g visual hierarchy emphasizes color/contrast, scale, and grouping to guide the eye to the most important information.
Source: https://www.nngroup.com/articles/visual-hierarchy-ux-definition/

WCAG 2.2 target size sets a 24 x 24 CSS pixel minimum for pointer targets, with exceptions.
Source: https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html

Product implication:

- Dense is fine for developers, but only if the hierarchy is ruthless.
- The primary answer, evidence confidence, and next action must dominate; filters and toggles should not compete.

## Would I Use It?

Yes, for these jobs:

- onboarding into a backend I do not know;
- checking the likely code path behind an API before editing;
- reviewing DB schema changes where row data must not leave the machine;
- preparing a refactor or handoff with a visual explanation;
- finding "where is this table/column probably touched?" faster than raw search.

I would stop using it if:

- clicking a node does not tell me the next file/function/table to inspect;
- candidates are noisy and not clearly ranked;
- I cannot jump from evidence to source location;
- broad maps stay pretty but do not narrow into task-relevant answers;
- I have to manually remember what I already inspected.

## Product Direction

The product should become an answer-first backend navigator:

```text
Question mode -> ranked path/impact answer -> evidence ledger -> source jump
```

The canvas remains important, but it should be the spatial explanation of an answer, not the answer itself.

## Priority Stack

### P0: Make The Existing Product Sticky

1. Answer Summary Panel

For each mode, show the answer before the details:

- API Flow: "Read these files in this order", "confirmed calls", "candidate DB touchpoints", "unknowns".
- Table Usage: "Confirmed DB structure", "candidate code usage", "top risky callers".
- Column Impact: "direct DB impact", "candidate code impact", "safe/unsafe unknowns".

Why:

- This directly answers Sillito-style impact and location questions.
- It can reuse the current `VisualMap`, `InventorySnapshot`, and inspector evidence model.

Smallest next slice:

- Add a read-only summary block in `InspectorPanel` or the right panel from current map edges.
- No new engine, no new data model at first.

2. Atlas Group Drilldown

Group nodes must be clickable. Clicking `API /x`, `folder / service`, or `DB public` should open a focused group map/list.

Why:

- This is already in the final product target and QA notes as missing.
- Without drilldown, the overview is a poster, not a tool.

Smallest next slice:

- Encode group membership in atlas projection output or derive it deterministically from snapshot items.
- On group click, show the group's top routes/code/tables sorted by type and relevance.

3. Source Jump

Add "Open in editor" for file/line evidence, alongside copy.

Why:

- Developers live in the IDE. A map that cannot take them to code adds friction.
- Current copy buttons are useful but still make the user switch manually.

Smallest next slice:

- Support VS Code/Cursor style command URLs or a configurable editor command.
- Keep it local and optional.

4. Search-To-Focus UX

Search should behave like a command palette for code + DB:

- auto-focus search when entering search mode;
- keep grouped results visible until selection;
- show a strong empty-state prompt when no query exists;
- preserve last query and selected result trail.

Why:

- Search is the fastest entry point for "I know one word: session/account/order".
- Current implementation already groups results, so this is mostly UX completion.

### P1: Improve Trust And Impact Quality

5. Evidence Ledger

Create a compact evidence table for the selected answer:

- type: confirmed / inferred / candidate;
- source: CALLS, FK, name match, column match, snapshot;
- file/table/column;
- confidence;
- reason;
- action: copy/open.

Why:

- Candidate links are acceptable only when users can inspect why they exist.
- This supports skeptical senior developers.

6. Better Code-To-DB Candidate Ranking

Name matching alone is too weak. Improve candidates by adding cheap evidence layers:

- SQL/table-name literals in code files;
- migration/DDL references when present;
- repository/query class naming;
- route domain token overlap;
- call-path proximity from selected API to repository/query code.

Why:

- The current `candidate_links` function is intentionally simple.
- Precision matters more than visual quantity.

Smallest next slice:

- Keep all links candidate.
- Add evidence kinds and rank score; do not mark code-to-DB as confirmed without direct engine proof.

7. Impact Review Board

For a selected table/column, show four lanes:

- confirmed DB impact;
- likely code impact;
- unknown / needs verification;
- suggested checks.

Why:

- This is the real "I would use this before changing code" feature.
- It turns the map into a pre-change checklist.

### P2: Expand When P0/P1 Are Solid

8. Diff / PR Impact Mode

Input a Git diff or changed files, then show impacted APIs, code paths, DB objects, and unknowns.

Why:

- This is a high-value paid/team feature.
- It should wait until source jump, evidence ledger, and candidate ranking are reliable.

9. Exportable Investigation Snapshot

Export a Markdown/HTML artifact:

- selected question;
- map screenshot or compact diagram;
- evidence ledger;
- source paths;
- caveats.

Why:

- Useful for PR review, onboarding, and architecture discussion.

10. Large Project Relevance Ranking

Atlas groups should rank by relevance, not alphabetically:

- selected focus proximity;
- route count;
- table count;
- call degree;
- candidate confidence;
- recent change frequency later if git history is added.

Why:

- Large projects need prioritization; a complete list is not an answer.

## What Not To Build Yet

- Do not build chatbot Q&A. The user goal is fast visual answers, not another question box.
- Do not add PR Impact before evidence quality improves.
- Do not add team cloud sync before local trust and packaging are release-ready.
- Do not make more graph modes without a named developer question.
- Do not confirm code-to-DB links unless a real source proves them.
- Do not spend time on 3D, animated graph spectacle, or decorative visual polish.

## Design Rules To Keep

- Default map: grouped and under 40 visible nodes.
- Detail map: 1 to 2 hop around a chosen focus.
- Every candidate needs confidence and evidence.
- Empty states must tell the next action.
- Commands stay near the region they affect.
- Dense UI is allowed, but primary answer and next action must be visually dominant.
- Minimum target size should stay at least 24 x 24 CSS pixels unless a dense visualization has an equivalent action.

## Decision Log

### Decision: Answer-first over graph-first

Context:

- Research says developers ask concrete location, reachability, and impact questions.
- Current app already has map projections but still leaves too much interpretation to the user.

Decision:

- Develop answer summary, evidence ledger, and source jump before adding more visual modes.

Consequence:

- Less flashy, more useful. The product may look simpler, but developer trust should rise.

### Decision: Keep local/private positioning

Context:

- Current app avoids row data, password persistence, and external upload.
- Backend/DB metadata is sensitive in many teams.

Decision:

- Keep local-first as a core differentiator.

Consequence:

- Team/cloud features wait until local release quality is strong.

### Decision: Improve candidate evidence before PR impact

Context:

- Current code-to-DB candidates are mostly name-based.
- Impact mode becomes dangerous if candidates look authoritative.

Decision:

- Rank and explain candidates first; only then build diff/PR impact.

Consequence:

- Slower path to flashy enterprise feature, but fewer false-confidence failures.

## Final Recommendation

The next development theme should be:

```text
From visual map to evidence-backed answer workspace.
```

If I had to choose one next phase, I would build:

```text
P0 Answer Summary + Atlas Group Drilldown + Source Jump
```

That is the smallest bundle that changes the product from "interesting visualization" into "I can actually use this while editing a backend."
