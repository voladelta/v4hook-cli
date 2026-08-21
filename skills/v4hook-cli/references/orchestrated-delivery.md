# Chief-led delivery

Use this workflow for a complete hook implementation or material adaptation when multi-agent
delegation is available. The chief owns the parent task contract, local convergence ledger,
architecture, integration order, accepted evidence, and terminal status. Children receive bounded
task contracts and never inherit parent completion authority.

For ordinary one-agent changes, keep the direct `inspect → implement → verify` path.

## Dispatch exact role profiles

Every delegation call explicitly passes `model`, `reasoning_effort`, and `fork_turns`. Set
`fork_turns` to `"none"` or a bounded positive turn count compatible with the requested profile.
Full-history inheritance, `fork_turns: "all"`, or omitting any of the three arguments makes the
dispatch invalid, even when inheritance would happen to select the same model and reasoning.

| Role | Model | Reasoning | Responsibility |
| --- | --- | --- | --- |
| Chief | `gpt-5.6-sol` | high | Parent contract, architecture, ledger, integration, terminal decision |
| Scout | `gpt-5.6-luna` | xhigh | Narrow documentation or pinned-API research; read-only |
| Implementor | `gpt-5.6-sol` | medium | Bounded production slices and focused proof |
| Reviewer | `gpt-5.6-sol` | high | Fresh adversarial review; tracked source read-only |
| Fixer | `gpt-5.6-sol` | medium | Accepted root causes and focused regression proof |
| Verifier | `gpt-5.6-sol` | high | Independent unchanged gates; read-only |

Record the requested profile in the ledger before dispatch and confirm it from the tool result when
the runtime exposes the resolved profile. If an exact profile is unavailable, do not silently
downgrade or inherit another profile; preserve the frontier and classify the need for a user-approved
alternative as Escalated. Profile availability never weakens the parent gate.

At the delegation callsite, copy `model` and `reasoning_effort` from the table into explicit tool
arguments and supply a compatible non-inheriting `fork_turns`. A result that does not echo its
resolved profile is acceptable because the recorded call arguments are the evidence; it is not a
reason to omit them. If any required argument was omitted or incompatible, interrupt that child
before accepting work, record the invalid dispatch, confirm it is inactive, and redispatch
correctly.

## Keep one chief

After project-local inspection, the chief reads `task-contracts`, writes the smallest sufficient
parent contract, and creates the ignored ledger from [local-workflow.md](local-workflow.md). The
chief crafts every child contract, inspects each returned claim, integrates accepted work, and alone
classifies the parent as Complete, Escalated, or Blocked.

Preparation records requirements and verification ownership; it does not pre-solve the
implementation. Keep one compact specification for a fresh build, keep command inventory and
frontier state in the ignored ledger, and create a separate protected-invariant document only when
adapting existing behavior. Implementation mechanics that do not change the parent contract belong
to the implementor.

The chief may initialize the scaffold and author the parent contract, specification, frozen
verification inputs, ledger, and chief adjudication. In delegated delivery, production contracts,
tests, scripts, implementation configuration, and candidate documentation do not begin until an
explicitly profiled implementor or fixer is recorded as their candidate owner. The chief never
authors candidate changes. It remains a coordinator that inspects state, dispatches roles, accepts
evidence, integrates clean commits, and runs parent controls. Every candidate change, including a
trivial repair, belongs to an explicitly profiled implementor or fixer.

Use one writer at a time in a shared worktree. Parallel writers require isolated worktrees. Every
child that produces candidate changes has explicit Git working-tree authority and materializes
those changes in its assigned worktree. Its contract separately grants or withholds index and
commit authority. A child without working-tree authority is read-only and returns findings,
evidence, or design only; it never returns a candidate patch for someone else to apply.

If an external or otherwise unmaterialized candidate patch exists, the chief records it as input
and dispatches a fresh explicitly profiled implementor or fixer with working-tree authority to
apply and validate it. The chief never applies, authors, or alters candidate patches. When its
integration authority allows, the chief may inspect and accept already-materialized child work,
then stage and commit it without changing the candidate contents.

Route each child to only the domain skill and reference needed for its owned boundary. The chief
already encodes orchestration in the child contract, so children do not reload orchestration skills
unless one is explicitly appointed as a coordinator.

Children do not spawn descendants unless their child contract grants that authority for one named,
independent frontier action. Otherwise they return the newly discovered need to the chief.

Every dispatch uses the smallest task-contract envelope that makes ownership and proof unambiguous:

```text
Destination:
Anchors and supplied context:
Owned files or read-only surface:
Dependencies and invariants:
Authority:
Git working-tree, index, and commit authority:
Proof and evidence to return:
Handoff point and child gate:
```

Omit fields that do not change execution. Each child gate advances one named parent requirement and
ends before parent completion.

Immediately before dispatch, update the ledger with the child role, explicit model, reasoning,
`fork_turns`, contract, owned surface, Git authority, and expected verifier. Immediately after
dispatch, record the returned child identity and status. Update the same entry on every checkpoint,
interruption, or completion; do not leave a running writer absent from the authoritative ledger.

## Recover a non-progressing writer

Elapsed waits and an unchanged worktree are observations, not proof that a writer is stuck. Recover
an implementor or fixer through this ordered state machine:

1. Inspect the active child status, relevant live processes, and actual shared-worktree state.
2. Request a checkpoint containing its current action, produced artifacts, latest command, and
   blocker or next verifier. If no checkpoint arrives after the bounded request, record
   `checkpoint unavailable`; do not invent one.
3. Preserve the actual partial patch and evidence already present, including the absence of either.
4. Update the ledger with the inspection, checkpoint or `checkpoint unavailable`, preserved patch
   and evidence, and the decision to interrupt.
5. Interrupt the writer.
6. Confirm from child status that the interrupted writer is inactive. If it is still active, keep
   waiting or stop at that frontier; do not dispatch another writer.
7. Update the ledger with the stop confirmation and the preserved handoff state.
8. Only after that ledger update, redispatch a fresh child with the exact role profile, explicit
   non-inheriting `fork_turns`, and a narrower or changed-hypothesis contract.

Never dispatch a replacement writer while the prior writer remains active. The chief remains
non-writing. If two correctly profiled writer attempts produce no evidence delta, change
decomposition or classify Escalated/Blocked under the parent rules; do not convert the chief into
the implementor or fixer.

## Scout unresolved boundaries

The chief's research lane ends after the generated project map, the selected installed domain
reference, and narrow symbol lookup in the pinned project tree. At that point, classify every
architecture-critical question as locally answered or unresolved. An unresolved external or
version-sensitive fact becomes a scout contract; it does not become an open-ended chief research
task. External documentation lookup belongs to that scout: the chief does not issue product or API
web searches in this delegated workflow.

Dispatch one or two scouts in parallel only for unresolved questions. Split by independent
boundary, such as pinned PoolManager settlement and an external oracle lifecycle. A scout starts
with the selected installed domain reference and pinned project sources. Only a specifically named
live fact absent from both permits one targeted official URL lookup. The scout returns after that
lookup even if uncertainty remains; broad searches, documentation indexes, and fallback-query
cascades are outside the scout gate. The child contract supplies the exact local reference path and
the one missing question so the scout does not rediscover the domain. A scout owns no tracked files
and returns:

```text
Question and parent requirement:
Exact source anchors:
Constraints and traps:
Recommended decision:
Remaining uncertainty:
```

The chief accepts or rejects each recommendation and records the result before it becomes an
implementation assumption. Do not dispatch a scout for facts available in the generated project
map, owned source, configuration, selected embedded reference, installed `--help`, or a narrow
pinned symbol lookup.

Preparation ends at a clean frozen baseline. Once it is frozen and no scout result is outstanding,
the next autonomous action is implementor dispatch. The chief performs no additional research
between freeze and dispatch unless the freeze itself exposes a new, recorded blocker.

## Implement vertical slices

Use one implementor by default. Add a second writer only when interfaces are frozen, owned files do
not overlap, and each workstream has independent proof and a defined integration order. Hook
callbacks, settlement, token accounting, and shared custody normally stay with one owner.

The implementor works in vertical slices:

```text
parent requirement → failing behavioral proof → smallest production change → focused green
```

Its child contract names the accepted architecture, owned files, external interfaces, frozen
verification constraints, and integration handoff. It authorizes only that production and test
surface plus non-destructive local checks, and explicitly states its Git working-tree, index, and
commit authority.

Start with compiling interfaces and one real production-path slice. Add its rollback or hostile
case before moving to the next slice. Run formatting, compilation, focused tests, structured static
analysis, and small fuzz or invariant smoke workloads as soon as each becomes relevant. Do not use
the complete configured check as a development loop; smoke workloads supplement rather than replace
the frozen full counts.

The implementation handoff is a clean candidate commit whose production path, focused integration
suite, static policy, and smoke properties are green. The implementor reports exact commands,
counts, changed files, assumptions, and gaps; it does not claim parent completion.

## Review before the expensive gate

Dispatch a fresh reviewer for the exact focused-green candidate before full configured fuzz and
invariant workloads. Give it the parent contract, protected-invariant ledger, frozen workload,
baseline and candidate identities, and candidate diff—not the implementor's narrative.

The reviewer treats tracked source as read-only but may write its ignored preliminary findings to
`.v4hook/adversarial-findings.json`:

```text
Destination: classified preliminary findings for the exact candidate commit.
Proof: every finding names the violated parent requirement, exact anchor, impact, and reproducer or
missing evidence; compare verification scope and production causality with the frozen baseline.
Child gate: every material requirement is preserved, explicitly approved, or a must-fix. Never
claim parent completion.
```

The chief treats the findings as claims. Accepted must-fix findings move to a fresh fixer. Rejected
findings retain a rationale in the ledger. The reviewer does not adjudicate on the chief's behalf or
guess the `candidateSource.treeDigest` that the first complete check will expose.

## Repair accepted findings

Give one fresh fixer only the accepted findings, anchors and reproducers, frozen verification
constraints, and exclusive file ownership:

```text
Destination: repair the accepted root causes in one clean candidate commit.
Authority: edit only the owned project surface; preserve the parent contract, frozen workload, and
external-action boundaries.
Proof: make the closest production-causal proof red for the expected reason, repair it, rerun the
focused gate, and report the diff and exact results.
Child gate: a clean candidate ready for fresh review. Never mark the parent Complete.
```

Every source change invalidates the prior review. Dispatch a fresh reviewer for the repaired commit
and repeat `review → accepted repair → focused proof` until the exact candidate has no accepted
must-fix finding.

## Verify the reviewed candidate

Give a fresh verifier the parent contract, frozen manifest, reviewer-clean preliminary findings,
exact candidate identity, and retained installed CLI path. Source, tests, and configuration are
read-only; running gates and updating ignored CLI verification state are allowed.

The verifier runs the closest accepted reproducers, then follows the exact command sequence in
[local-workflow.md](local-workflow.md). After first green exposes `candidateSource`, the chief
inspects that identity, the final reviewer evidence, and the verification state and authors the
strict v1 JSON adjudication. The verifier treats that report as read-only, binds it, and runs the
unchanged second check.

It inspects exact test names and counts, skips, settings, static-analysis policy, sizes, gas, source
identity, report digest, and lifecycle state. It returns verified evidence or one precise mismatch;
it never repairs the source or claims parent completion.

A verifier mismatch returns to the chief. Any source repair goes through a fixer, focused proof,
fresh review, and fresh verification. Repeat an action only with a progress delta or changed
hypothesis. Run the expensive complete gate only when every cheaper relevant gate is green.

## Finish at the parent

The chief independently inspects the verifier's evidence, clean source identity, bound review
digest, and lifecycle state. Only the chief may classify:

- **Complete:** every parent deliverable is integrated and the required lifecycle is green.
- **Escalated:** a user decision, approval, or grant of new authority can unblock the work. This
  classification takes precedence over Blocked.
- **Blocked:** a technical, environmental, access, or external-dependency limit that user input or
  authority cannot resolve leaves no autonomous next action.

Local green, a child report, or an unbound review is never parent green.
