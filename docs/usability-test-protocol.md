# Backend Developer Usability Test Protocol

Status: Ready to run; independent participant evidence pending
Last updated: 2026-07-22

## Purpose

Verify that a backend developer can answer codebase questions faster and more
accurately without learning the engine vocabulary or trusting unsupported
relationships.

This is a task test, not a visual-preference interview. Participants receive a
working repository and database fixture but no product walkthrough beyond the
one-sentence goal: "Use this app to find the answer and show its evidence."

## Participants

- Minimum release sample: 9 backend developers.
- Include at least 3 junior, 3 mid-level, and 3 senior developers.
- Include at least 2 developers unfamiliar with each test repository.
- Record language/framework familiarity, but do not exclude a participant for
  preferring code-first or visual-first investigation.

The facilitator must not explain which menu or control to press during a timed
task. A requested hint is recorded as one assistance event.

## Test Fixtures

Use one medium repository with a real API-to-static-SQL path and one deliberately
disconnected API/table pair. Keep the repository commit, DB schema, expected
answers, and product build checksum fixed for the whole round.

Before each session:

1. reset application data;
2. verify the code and DB fixtures independently;
3. start screen and interaction recording with participant consent;
4. confirm that no row data or production credentials are present.

## Tasks

| ID | Developer question | Required observable answer | Target time |
| --- | --- | --- | ---: |
| T1 | Open the project and database so they are ready to inspect. | Code and DB snapshots are current; no secret is persisted. | 4 min |
| T2 | Where is this API handled, and what calls does it make? | Route, handler, downstream code, source location, and truth class. | 3 min |
| T3 | Does this API read or write the named table and columns? | `READS`/`WRITES`, explicit columns, SQL source line, or an honest candidate/unknown answer. | 4 min |
| T4 | Show only the relationship among one API, one service/repository function, and two DB objects. | All four subjects remain selected; the chosen HOW view shows only bounded connecting context. | 3 min |
| T5 | What may be affected if this column changes? | Direct, candidate, and unknown impact are distinguished; source evidence is reachable. | 4 min |
| T6 | Are these two selected items connected? | A disconnected result is stated plainly without fabricating a path. | 2 min |
| T7 | Return to the earlier API and verify the evidence in source. | Stable navigation restores the target and opens the correct file/line. | 2 min |

After every task, ask the participant for a 1-7 Single Ease Question score and
one sentence explaining what they expected to happen.

## Measurements

Record for each task:

- success without help, success with help, or failure;
- time to first correct answer;
- wrong menu choices, backtracks, and accidental target changes;
- whether the participant opened evidence before answering;
- false-confidence events: accepting a candidate as confirmed, or claiming a
  relationship the product did not prove;
- SEQ score from 1 (very difficult) to 7 (very easy).

Also record crashes, stale-state confusion, layout overflow, hidden controls,
keyboard/accessibility blockers, and any moment where labels required engine
terminology to interpret.

## Release Gate

The round passes only when all conditions hold:

- at least 90% of T2-T7 attempts succeed without facilitator help;
- every task's median completion time is at or below its target;
- median SEQ is at least 5.5/7;
- zero false confirmed relationships are reported by the product;
- zero participants mistake a candidate or unknown edge for confirmed after
  opening its evidence;
- no critical flow requires a moving control, hidden hover action, or restart;
- every observed P0/P1 issue is fixed and rerun with the affected task.

T1 may fail because of an environment prerequisite only when the product names
that prerequisite and recovery action. Credential leakage, fabricated data, or
silent partial indexing is always a failed round.

## Evidence Record

Create one dated report under `docs/reports/` containing:

- build commit and checksum;
- fixture commits and schema checksum;
- anonymized participant experience bands;
- per-task raw measurements and aggregate medians;
- every assistance event and false-confidence event;
- issue links, fixes, and rerun results;
- explicit pass/fail decision.

Automated tests and native smoke runs may verify mechanics, but they do not
count as independent participant evidence.
