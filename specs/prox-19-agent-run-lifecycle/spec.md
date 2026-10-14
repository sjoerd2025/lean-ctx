# Spec: agent-run lifecycle reference rail (refs PROX-19)

## Problem / Why
The rail an agent run travels — queued → planning → executing → reviewing → completed —
exists only as documentation prose and as scattered enforcement in the runtime. Nothing in the
Engine states the rail as reviewable behavior: which event moves a run, which holds pause it
without ending it, which events end it, and which moves are illegal. Approval and a missing
input are consequently easy to model as endings, a recoverable error is easy to make terminal,
and "no reply" has no state to sit in.

## Goal
`lean-ctx-protocol` publishes the frozen reference rail: five ordered phases, two holds that
pause a run without ending it and without consuming a worker, a recoverable-error loop, and two
explicit terminal exits. Every hold names the events that continue it, and the rail is
self-validating.

## Acceptance Criteria (EARS)
- The contract SHALL define exactly five ordered run phases: `queued`, `planning`, `executing`,
  `reviewing`, `completed`.
- WHEN a run is `queued` and `start` arrives, THE RAIL SHALL move it to `planning`.
- WHEN `plan ready` arrives in `planning`, THE RAIL SHALL move the run to `executing`.
- WHEN `steps done` arrives in `executing`, THE RAIL SHALL move the run to `reviewing`.
- WHEN `pass` arrives in `reviewing`, THE RAIL SHALL move the run to `completed`.
- THE CONTRACT SHALL treat `completed` as the fifth ordered phase and a final state, not as a
  third ending distinct from the phases.
- WHEN `risky tool` arrives in `executing`, THE RAIL SHALL hold the run in `needs approval` and
  SHALL NOT end it; `risky tool` SHALL be rejected from every other phase.
- WHEN `approved` arrives in `needs approval`, THE RAIL SHALL resume the run at `executing`,
  never at a later phase.
- WHEN `denied` arrives in `needs approval`, THE RAIL SHALL end the run as `cancelled`.
- WHEN `tool error` arrives in `executing`, THE RAIL SHALL move the run to `failed`, and SHALL NOT
  treat that as a terminal exit.
- WHEN `retry` arrives in `failed`, THE RAIL SHALL return the run to `executing`.
- WHEN `needs input` arrives in `reviewing`, THE RAIL SHALL hold the run in `blocked`;
  `needs input` SHALL be rejected from every other phase.
- WHEN `input given` arrives in `blocked`, THE RAIL SHALL resume the run at `reviewing`.
- WHEN `no reply` arrives in `blocked`, THE RAIL SHALL end the run as `expired`.
- THE CONTRACT SHALL expose, for every non-terminal state, the pending events that continue it,
  so a held run never needs a worker to discover how to proceed.
- WHEN an event is not on the rail for the current state, THE RUN SHALL remain unchanged and the
  caller SHALL receive a rejection error.
- WHEN a run already finished, EVERY event SHALL fail with the ended error and SHALL NOT move it.
- THE RAIL SHALL reject a transition table that holds a run with no way to continue it, holds
  approval outside `executing`, holds a missing input outside `reviewing`, completes a run
  without `reviewing`, expires a run without a hold, or leaves a final state.
- THE RAIL SHALL reject a transition table that declares one event twice in one state.

## Out of Scope
- Running a step, calling a tool, or taking a retry-budget or timeout decision. The module models
  the rail; a caller whose retry budget is exhausted has no rail edge to take. Production run
  intelligence stays Class D in `lean-ctx-enterprise`.
- A wire schema or contract-portal document. Like `circuit_breaker`, this reference carries
  behavior only; there is no payload schema to version in `docs/contracts/`.
- Persistence, timers, and worker scheduling for a held run. The hold states observe the run;
  they do not run a scheduler.

## Verification
- `cargo test -p lean-ctx-protocol --lib agent_run_lifecycle` (10 rail tests).
- `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`
- `cargo fmt -p lean-ctx-protocol -- --check`
- Drift probes: retargeting `approved` to resume at `reviewing`, no-op'ing `require_continuable`,
  and making `completed` non-terminal each make the rail tests fail.

## Links
- Tracking issue: PROX-19 (Multica); artifact: <https://archify.si/gallery/artifacts/agent-run.lifecycle>
- Plan: ./plan.md · Tasks: ./tasks.md
