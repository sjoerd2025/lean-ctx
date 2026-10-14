// SPDX-License-Identifier: Apache-2.0
//! Frozen agent-run lifecycle reference behavior.
//!
//! A run moves through five ordered phases ([`RunPhaseV1`]) from `queued` to
//! `completed`. Human approval and a missing input hold it in a [`RunWaitV1`]
//! wait state that is not an ending and consumes no worker;
//! [`run_pending_events`] names the events that continue such a run. A
//! recoverable error moves the run to `failed`, which is continuable rather than
//! terminal: its `retry` edge returns it to `executing`. Only `cancelled`
//! (approval denied) and `expired` (no reply inside the hold window) are
//! terminal exits, and neither has a return path to the rail.
//!
//! The two holds are distinct and neither skips work: approval is reachable only
//! from `executing` (a risky tool) and resumes at `executing`; missing input is
//! reachable only from `reviewing` and resumes at `reviewing`.
//!
//! This module models the public rail only. It runs no step, calls no tool, and
//! takes no retry-budget or timeout decision — a caller whose retry budget is
//! exhausted has no rail edge to take. Production run intelligence remains
//! Class D in `lean-ctx-enterprise`.

use crate::ValidationError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Ordered phase of a run that has not ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhaseV1 {
    /// Request accepted; waiting for a worker to start it.
    Queued,
    /// Building the task graph for the accepted request.
    Planning,
    /// Running the plan's tool calls.
    Executing,
    /// Quality gate over the work the run produced.
    Reviewing,
    /// Final response produced; the run is complete.
    Completed,
}

/// The five ordered phases, from acceptance to completion.
pub const RUN_PHASE_ORDER: [RunPhaseV1; 5] = [
    RunPhaseV1::Queued,
    RunPhaseV1::Planning,
    RunPhaseV1::Executing,
    RunPhaseV1::Reviewing,
    RunPhaseV1::Completed,
];

/// Wait state that holds a run without ending it or consuming a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunWaitV1 {
    /// A human must approve the risky tool the run reached for.
    NeedsApproval,
    /// The run cannot continue without input it does not have.
    Blocked,
}

impl RunWaitV1 {
    /// Events that resolve this wait; a wait never resolves by itself.
    pub fn resolution_events(self) -> &'static [RunEventV1] {
        match self {
            Self::NeedsApproval => &[RunEventV1::Approved, RunEventV1::Denied],
            Self::Blocked => &[RunEventV1::InputGiven, RunEventV1::NoReply],
        }
    }
}

/// Terminal exit of a run; a terminal run accepts no further event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEndingV1 {
    /// Approval was denied; the run stops before it produces anything.
    Cancelled,
    /// No reply arrived inside the hold window; the run stops waiting.
    Expired,
}

/// Final state of a run, reported so a caller can name how it finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTerminalV1 {
    /// The run produced its final response.
    Completed,
    /// The run was stopped by a denied approval.
    Cancelled,
    /// The run stopped waiting for input it never received.
    Expired,
}

/// Every final state a run can report, in artifact order.
pub const RUN_TERMINALS: [RunTerminalV1; 3] = [
    RunTerminalV1::Completed,
    RunTerminalV1::Cancelled,
    RunTerminalV1::Expired,
];

/// State of one run on the lifecycle rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum RunStateV1 {
    /// The run is on an ordered phase.
    Phase(RunPhaseV1),
    /// The run is held in a wait state.
    Waiting(RunWaitV1),
    /// A recoverable error; the run can be retried from `executing`.
    Failed,
    /// The run reached a terminal exit.
    Ended(RunEndingV1),
}

impl RunStateV1 {
    /// Return the phase, when the run is on the rail.
    pub fn phase(self) -> Option<RunPhaseV1> {
        match self {
            Self::Phase(phase) => Some(phase),
            Self::Waiting(_) | Self::Failed | Self::Ended(_) => None,
        }
    }

    /// Return the wait state, when the run is held.
    pub fn wait(self) -> Option<RunWaitV1> {
        match self {
            Self::Waiting(wait) => Some(wait),
            Self::Phase(_) | Self::Failed | Self::Ended(_) => None,
        }
    }

    /// Return the final state, when the run already finished.
    pub fn terminal(self) -> Option<RunTerminalV1> {
        match self {
            Self::Phase(RunPhaseV1::Completed) => Some(RunTerminalV1::Completed),
            Self::Ended(RunEndingV1::Cancelled) => Some(RunTerminalV1::Cancelled),
            Self::Ended(RunEndingV1::Expired) => Some(RunTerminalV1::Expired),
            Self::Phase(_) | Self::Waiting(_) | Self::Failed => None,
        }
    }

    /// Report whether the run reached a final state.
    pub fn is_terminal(self) -> bool {
        self.terminal().is_some()
    }
}

/// Every run state, in artifact order: five phases, two waits, the recovery
/// loop, and the two terminal exits.
pub const RUN_STATES: [RunStateV1; 10] = [
    RunStateV1::Phase(RunPhaseV1::Queued),
    RunStateV1::Phase(RunPhaseV1::Planning),
    RunStateV1::Phase(RunPhaseV1::Executing),
    RunStateV1::Phase(RunPhaseV1::Reviewing),
    RunStateV1::Phase(RunPhaseV1::Completed),
    RunStateV1::Waiting(RunWaitV1::NeedsApproval),
    RunStateV1::Waiting(RunWaitV1::Blocked),
    RunStateV1::Failed,
    RunStateV1::Ended(RunEndingV1::Cancelled),
    RunStateV1::Ended(RunEndingV1::Expired),
];

/// Event that moves a run along one rail edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEventV1 {
    /// A worker started the queued request.
    Start,
    /// The task graph is ready to execute.
    PlanReady,
    /// The plan's steps ran to completion.
    StepsDone,
    /// The quality gate accepted the work.
    Pass,
    /// The plan reached for a tool a human must approve.
    RiskyTool,
    /// The human approved the risky tool.
    Approved,
    /// The human denied the risky tool.
    Denied,
    /// A tool call failed recoverably.
    ToolError,
    /// The run retries the failed step.
    Retry,
    /// The quality gate needs input the run does not have.
    NeedsInput,
    /// The missing input arrived.
    InputGiven,
    /// No reply arrived inside the hold window.
    NoReply,
}

/// Every rail event, in artifact order.
pub const RUN_EVENTS: [RunEventV1; 12] = [
    RunEventV1::Start,
    RunEventV1::PlanReady,
    RunEventV1::StepsDone,
    RunEventV1::Pass,
    RunEventV1::RiskyTool,
    RunEventV1::Approved,
    RunEventV1::Denied,
    RunEventV1::ToolError,
    RunEventV1::Retry,
    RunEventV1::NeedsInput,
    RunEventV1::InputGiven,
    RunEventV1::NoReply,
];

/// One directed edge of the lifecycle rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunTransitionV1 {
    /// State the edge leaves.
    pub from: RunStateV1,
    /// Event that selects the edge.
    pub event: RunEventV1,
    /// State the edge enters.
    pub to: RunStateV1,
}

/// The canonical lifecycle rail: every state and event pair a run accepts.
pub const RUN_TRANSITIONS: [RunTransitionV1; 12] = [
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Queued),
        event: RunEventV1::Start,
        to: RunStateV1::Phase(RunPhaseV1::Planning),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Planning),
        event: RunEventV1::PlanReady,
        to: RunStateV1::Phase(RunPhaseV1::Executing),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Executing),
        event: RunEventV1::StepsDone,
        to: RunStateV1::Phase(RunPhaseV1::Reviewing),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Reviewing),
        event: RunEventV1::Pass,
        to: RunStateV1::Phase(RunPhaseV1::Completed),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Executing),
        event: RunEventV1::RiskyTool,
        to: RunStateV1::Waiting(RunWaitV1::NeedsApproval),
    },
    RunTransitionV1 {
        from: RunStateV1::Waiting(RunWaitV1::NeedsApproval),
        event: RunEventV1::Approved,
        to: RunStateV1::Phase(RunPhaseV1::Executing),
    },
    RunTransitionV1 {
        from: RunStateV1::Waiting(RunWaitV1::NeedsApproval),
        event: RunEventV1::Denied,
        to: RunStateV1::Ended(RunEndingV1::Cancelled),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Executing),
        event: RunEventV1::ToolError,
        to: RunStateV1::Failed,
    },
    RunTransitionV1 {
        from: RunStateV1::Failed,
        event: RunEventV1::Retry,
        to: RunStateV1::Phase(RunPhaseV1::Executing),
    },
    RunTransitionV1 {
        from: RunStateV1::Phase(RunPhaseV1::Reviewing),
        event: RunEventV1::NeedsInput,
        to: RunStateV1::Waiting(RunWaitV1::Blocked),
    },
    RunTransitionV1 {
        from: RunStateV1::Waiting(RunWaitV1::Blocked),
        event: RunEventV1::InputGiven,
        to: RunStateV1::Phase(RunPhaseV1::Reviewing),
    },
    RunTransitionV1 {
        from: RunStateV1::Waiting(RunWaitV1::Blocked),
        event: RunEventV1::NoReply,
        to: RunStateV1::Ended(RunEndingV1::Expired),
    },
];

/// Return the state entered from `from` by `event`, when the rail accepts it.
pub fn run_transition(from: RunStateV1, event: RunEventV1) -> Option<RunStateV1> {
    RUN_TRANSITIONS
        .iter()
        .find(|transition| transition.from == from && transition.event == event)
        .map(|transition| transition.to)
}

/// Events that continue or resolve a run in `state`.
///
/// A wait state returns its resolution events, so a held run never needs a
/// worker to discover what continues it. Final states return nothing.
pub fn run_pending_events(state: RunStateV1) -> &'static [RunEventV1] {
    match state {
        RunStateV1::Phase(RunPhaseV1::Queued) => &[RunEventV1::Start],
        RunStateV1::Phase(RunPhaseV1::Planning) => &[RunEventV1::PlanReady],
        RunStateV1::Phase(RunPhaseV1::Executing) => &[
            RunEventV1::StepsDone,
            RunEventV1::RiskyTool,
            RunEventV1::ToolError,
        ],
        RunStateV1::Phase(RunPhaseV1::Reviewing) => &[RunEventV1::Pass, RunEventV1::NeedsInput],
        RunStateV1::Waiting(wait) => wait.resolution_events(),
        RunStateV1::Failed => &[RunEventV1::Retry],
        RunStateV1::Phase(RunPhaseV1::Completed) | RunStateV1::Ended(_) => &[],
    }
}

/// Reason a run could not accept an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunLifecycleError {
    /// No rail edge accepts the event in the current state.
    Rejected {
        /// State the run was in.
        from: RunStateV1,
        /// Event the caller supplied.
        event: RunEventV1,
    },
    /// The run already finished; a final state absorbs no event.
    Ended {
        /// Final state the run reached.
        terminal: RunTerminalV1,
        /// Event the caller supplied.
        event: RunEventV1,
    },
}

impl fmt::Display for RunLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, event } => write!(
                formatter,
                "run event {event:?} is not accepted in state {from:?}"
            ),
            Self::Ended { terminal, event } => write!(
                formatter,
                "run finished as {terminal:?} and accepts no further {event:?}"
            ),
        }
    }
}

impl std::error::Error for RunLifecycleError {}

/// Mutable run object: exactly one lifecycle state at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunLifecycleV1 {
    /// Current state; a run starts queued when the request is accepted.
    pub state: RunStateV1,
}

impl Default for RunLifecycleV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl RunLifecycleV1 {
    /// Start a run whose request was accepted.
    pub fn new() -> Self {
        Self {
            state: RunStateV1::Phase(RunPhaseV1::Queued),
        }
    }

    /// Return the current state.
    pub fn state(&self) -> RunStateV1 {
        self.state
    }

    /// Apply `event`, moving the run one rail edge.
    ///
    /// A rejected event leaves the run unchanged, so a caller may retry with the
    /// events [`run_pending_events`] names for the current state.
    pub fn advance(&mut self, event: RunEventV1) -> Result<RunStateV1, RunLifecycleError> {
        if let Some(terminal) = self.state.terminal() {
            return Err(RunLifecycleError::Ended { terminal, event });
        }
        match run_transition(self.state, event) {
            Some(state) => {
                self.state = state;
                Ok(state)
            }
            None => Err(RunLifecycleError::Rejected {
                from: self.state,
                event,
            }),
        }
    }
}

/// Validate the canonical rail and its declared pending events.
pub fn validate_run_lifecycle() -> Result<(), ValidationError> {
    validate_run_transitions(&RUN_TRANSITIONS)?;
    for state in RUN_STATES {
        let declared = run_pending_events(state);
        let edges = RUN_TRANSITIONS
            .iter()
            .filter(|transition| transition.from == state)
            .count();
        if state.is_terminal() {
            if edges > 0 || !declared.is_empty() {
                return Err(ValidationError::new(format!(
                    "final run state {state:?} must not leave the rail"
                )));
            }
            continue;
        }
        if edges == 0 {
            return Err(ValidationError::new(format!(
                "run state {state:?} must be continuable"
            )));
        }
        if edges != declared.len() {
            return Err(ValidationError::new(format!(
                "run state {state:?} declares {} pending events for {edges} rail edges",
                declared.len()
            )));
        }
        if let Some(missing) = declared
            .iter()
            .find(|event| run_transition(state, **event).is_none())
        {
            return Err(ValidationError::new(format!(
                "run state {state:?} declares unsupported event {missing:?}"
            )));
        }
    }
    Ok(())
}

/// Validate one rail: the shape rules every lifecycle rail must satisfy.
pub fn validate_run_transitions(transitions: &[RunTransitionV1]) -> Result<(), ValidationError> {
    for transition in transitions {
        if transition.from.is_terminal() {
            return Err(ValidationError::new(format!(
                "final run state {:?} must not leave the rail",
                transition.from
            )));
        }
        if transitions
            .iter()
            .filter(|other| other.from == transition.from && other.event == transition.event)
            .count()
            > 1
        {
            return Err(ValidationError::new(format!(
                "run state {:?} declares event {:?} more than once",
                transition.from, transition.event
            )));
        }
    }
    require_sources(
        transitions,
        RunStateV1::Phase(RunPhaseV1::Planning),
        &[RunStateV1::Phase(RunPhaseV1::Queued)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Phase(RunPhaseV1::Executing),
        &[
            RunStateV1::Phase(RunPhaseV1::Planning),
            RunStateV1::Waiting(RunWaitV1::NeedsApproval),
            RunStateV1::Failed,
        ],
    )?;
    require_sources(
        transitions,
        RunStateV1::Phase(RunPhaseV1::Reviewing),
        &[
            RunStateV1::Phase(RunPhaseV1::Executing),
            RunStateV1::Waiting(RunWaitV1::Blocked),
        ],
    )?;
    require_sources(
        transitions,
        RunStateV1::Phase(RunPhaseV1::Completed),
        &[RunStateV1::Phase(RunPhaseV1::Reviewing)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Waiting(RunWaitV1::NeedsApproval),
        &[RunStateV1::Phase(RunPhaseV1::Executing)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Waiting(RunWaitV1::Blocked),
        &[RunStateV1::Phase(RunPhaseV1::Reviewing)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Failed,
        &[RunStateV1::Phase(RunPhaseV1::Executing)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Ended(RunEndingV1::Cancelled),
        &[RunStateV1::Waiting(RunWaitV1::NeedsApproval)],
    )?;
    require_sources(
        transitions,
        RunStateV1::Ended(RunEndingV1::Expired),
        &[RunStateV1::Waiting(RunWaitV1::Blocked)],
    )?;
    require_continuable(transitions, RunStateV1::Waiting(RunWaitV1::NeedsApproval))?;
    require_continuable(transitions, RunStateV1::Waiting(RunWaitV1::Blocked))?;
    require_continuable(transitions, RunStateV1::Failed)?;
    Ok(())
}

/// Reject a rail where `state` is entered from an unlisted state.
fn require_sources(
    transitions: &[RunTransitionV1],
    target: RunStateV1,
    allowed: &[RunStateV1],
) -> Result<(), ValidationError> {
    for transition in transitions.iter().filter(|edge| edge.to == target) {
        if !allowed.contains(&transition.from) {
            return Err(ValidationError::new(format!(
                "run state {:?} must only be entered from {allowed:?}, found {:?}",
                target, transition.from
            )));
        }
    }
    Ok(())
}

/// Reject a rail where `state` holds a run with no way to continue it.
///
/// A wait pauses a run without ending it, and the recovery loop returns a failed
/// run to work, so each must expose at least one event that leaves it for a
/// non-terminal state.
fn require_continuable(
    transitions: &[RunTransitionV1],
    state: RunStateV1,
) -> Result<(), ValidationError> {
    if transitions
        .iter()
        .any(|transition| transition.from == state && !transition.to.is_terminal())
    {
        return Ok(());
    }
    Err(ValidationError::new(format!(
        "run state {state:?} must expose an event that continues it"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a run through `events`, failing on the first rejected event.
    fn run(events: &[RunEventV1]) -> RunLifecycleV1 {
        let mut lifecycle = RunLifecycleV1::new();
        for event in events {
            lifecycle.advance(*event).expect("event is on the rail");
        }
        lifecycle
    }

    /// Drive a run that finishes as `terminal`, so final-state assertions start
    /// from a real rail traversal instead of a hand-built state.
    fn finished(terminal: RunTerminalV1) -> RunLifecycleV1 {
        use RunEventV1 as E;
        match terminal {
            RunTerminalV1::Completed => run(&[E::Start, E::PlanReady, E::StepsDone, E::Pass]),
            RunTerminalV1::Cancelled => run(&[E::Start, E::PlanReady, E::RiskyTool, E::Denied]),
            RunTerminalV1::Expired => run(&[
                E::Start,
                E::PlanReady,
                E::StepsDone,
                E::NeedsInput,
                E::NoReply,
            ]),
        }
    }

    #[test]
    fn run_rail_advances_through_five_ordered_phases() {
        use RunEventV1 as E;
        let events = [E::Start, E::PlanReady, E::StepsDone, E::Pass];
        let mut lifecycle = RunLifecycleV1::new();
        assert_eq!(lifecycle.state(), RunStateV1::Phase(RunPhaseV1::Queued));
        for (step, event) in events.iter().enumerate() {
            let state = lifecycle.advance(*event).expect("rail edge");
            assert_eq!(state.phase(), Some(RUN_PHASE_ORDER[step + 1]));
        }
        assert_eq!(
            lifecycle.state().terminal(),
            Some(RunTerminalV1::Completed),
            "the fifth phase is the completed final state"
        );
    }

    #[test]
    fn approval_holds_the_run_and_resumes_at_executing() {
        use RunEventV1 as E;
        let mut lifecycle = run(&[E::Start, E::PlanReady]);

        lifecycle.advance(E::RiskyTool).expect("approval hold");
        assert_eq!(
            lifecycle.state().wait(),
            Some(RunWaitV1::NeedsApproval),
            "approval is a wait state, not a lifecycle phase"
        );
        assert!(!lifecycle.state().is_terminal(), "a hold is not an ending");
        assert_eq!(
            run_pending_events(lifecycle.state()),
            [E::Approved, E::Denied],
            "the hold names the events that continue it"
        );

        lifecycle
            .advance(E::Approved)
            .expect("approval resumes the run");
        assert_eq!(
            lifecycle.state().phase(),
            Some(RunPhaseV1::Executing),
            "approval resumes the runs's tool calls, never a later phase"
        );
    }

    #[test]
    fn denied_approval_ends_the_run_as_cancelled() {
        use RunEventV1 as E;
        let mut lifecycle = run(&[E::Start, E::PlanReady, E::RiskyTool]);
        assert_eq!(
            lifecycle.advance(E::Denied).expect("denial"),
            RunStateV1::Ended(RunEndingV1::Cancelled)
        );
        assert_eq!(
            lifecycle.state().terminal(),
            Some(RunTerminalV1::Cancelled),
            "a denied approval is a terminal exit with no return path"
        );
        assert_eq!(run_pending_events(lifecycle.state()), []);
    }

    #[test]
    fn missing_input_holds_the_run_and_resumes_at_reviewing() {
        use RunEventV1 as E;
        let mut lifecycle = run(&[E::Start, E::PlanReady, E::StepsDone]);

        lifecycle.advance(E::NeedsInput).expect("input hold");
        assert_eq!(lifecycle.state().wait(), Some(RunWaitV1::Blocked));
        assert_eq!(
            run_pending_events(lifecycle.state()),
            [E::InputGiven, E::NoReply]
        );

        lifecycle.advance(E::InputGiven).expect("input arrives");
        assert_eq!(
            lifecycle.state().phase(),
            Some(RunPhaseV1::Reviewing),
            "the run resumes the quality gate that asked for input"
        );
    }

    #[test]
    fn a_hold_without_a_reply_expires_the_run() {
        use RunEventV1 as E;
        let mut lifecycle = run(&[E::Start, E::PlanReady, E::StepsDone, E::NeedsInput]);
        assert_eq!(
            lifecycle.advance(E::NoReply).expect("no reply"),
            RunStateV1::Ended(RunEndingV1::Expired)
        );
        assert_eq!(
            lifecycle.state().terminal(),
            Some(RunTerminalV1::Expired),
            "an unanswered hold is a terminal exit, not a return to the rail"
        );
    }

    #[test]
    fn a_tool_error_recovers_through_retry_rather_than_ending_the_run() {
        use RunEventV1 as E;
        let mut lifecycle = run(&[E::Start, E::PlanReady]);
        assert_eq!(
            lifecycle.advance(E::ToolError).expect("recoverable error"),
            RunStateV1::Failed
        );
        assert!(
            !lifecycle.state().is_terminal(),
            "a recoverable error is not an ending"
        );
        assert_eq!(
            run_pending_events(lifecycle.state()),
            [E::Retry],
            "the recovery loop exposes only the retry that leaves it"
        );

        lifecycle.advance(E::Retry).expect("retry");
        assert_eq!(
            lifecycle.state().phase(),
            Some(RunPhaseV1::Executing),
            "a retried run executes again"
        );
        assert_eq!(
            lifecycle.advance(E::StepsDone).expect("steps done"),
            RunStateV1::Phase(RunPhaseV1::Reviewing)
        );
    }

    #[test]
    fn final_states_reject_every_event_and_keep_their_terminal() {
        for terminal in RUN_TERMINALS {
            let mut lifecycle = finished(terminal);
            assert_eq!(lifecycle.state().terminal(), Some(terminal));
            for event in RUN_EVENTS {
                assert_eq!(
                    lifecycle.advance(event),
                    Err(RunLifecycleError::Ended { terminal, event }),
                    "{terminal:?} must absorb {event:?}"
                );
                assert_eq!(
                    lifecycle.state().terminal(),
                    Some(terminal),
                    "a rejected event must not move a finished run"
                );
            }
        }
    }

    #[test]
    fn events_outside_the_rail_are_rejected_without_moving_the_run() {
        use RunEventV1 as E;
        let cases = [
            (RunLifecycleV1::new(), E::PlanReady),
            (RunLifecycleV1::new(), E::RiskyTool),
            (run(&[E::Start]), E::StepsDone),
            (run(&[E::Start]), E::Retry),
            (run(&[E::Start, E::PlanReady]), E::Pass),
            (run(&[E::Start, E::PlanReady]), E::NeedsInput),
            (run(&[E::Start, E::PlanReady]), E::Retry),
            (run(&[E::Start, E::PlanReady, E::RiskyTool]), E::StepsDone),
            (run(&[E::Start, E::PlanReady, E::RiskyTool]), E::InputGiven),
            (
                run(&[E::Start, E::PlanReady, E::StepsDone, E::NeedsInput]),
                E::Pass,
            ),
            (run(&[E::Start, E::PlanReady, E::StepsDone]), E::Denied),
        ];
        for (mut lifecycle, event) in cases {
            let before = lifecycle.state();
            assert_eq!(
                lifecycle.advance(event),
                Err(RunLifecycleError::Rejected {
                    from: before,
                    event
                }),
                "{event:?} must not be accepted in {before:?}"
            );
            assert_eq!(
                lifecycle.state(),
                before,
                "a rejected event changes nothing"
            );
        }
    }

    /// The canonical rail with the edge carrying `event` removed.
    fn without(event: RunEventV1) -> Vec<RunTransitionV1> {
        RUN_TRANSITIONS
            .iter()
            .copied()
            .filter(|edge| edge.event != event)
            .collect()
    }

    /// The canonical rail with `from --event--> to` replaced by `retargeted`.
    fn retarget(
        from: RunStateV1,
        event: RunEventV1,
        to: RunStateV1,
        retargeted: RunTransitionV1,
    ) -> Vec<RunTransitionV1> {
        let mut transitions = RUN_TRANSITIONS.to_vec();
        let edge = transitions
            .iter_mut()
            .find(|edge| edge.from == from && edge.event == event && edge.to == to)
            .expect("the canonical rail carries the edge being retargeted");
        *edge = retargeted;
        transitions
    }

    /// Assert a rail is rejected for one named invariant, not merely rejected.
    fn rejected(transitions: &[RunTransitionV1], invariant: &str) {
        let error = validate_run_transitions(transitions).expect_err("rail must be rejected");
        assert!(
            error.to_string().contains(invariant),
            "expected a rejection naming {invariant:?}, got {error}"
        );
    }

    #[test]
    fn a_rail_that_breaks_a_run_invariant_is_rejected() {
        use RunEventV1 as E;
        let queued = RunStateV1::Phase(RunPhaseV1::Queued);
        let planning = RunStateV1::Phase(RunPhaseV1::Planning);
        let executing = RunStateV1::Phase(RunPhaseV1::Executing);
        let reviewing = RunStateV1::Phase(RunPhaseV1::Reviewing);
        let completed = RunStateV1::Phase(RunPhaseV1::Completed);
        let approval = RunStateV1::Waiting(RunWaitV1::NeedsApproval);
        let blocked = RunStateV1::Waiting(RunWaitV1::Blocked);
        let cancelled = RunStateV1::Ended(RunEndingV1::Cancelled);
        let expired = RunStateV1::Ended(RunEndingV1::Expired);
        let edge = |from, event, to| RunTransitionV1 { from, event, to };

        // Each probe differs from the canonical rail in exactly one respect, so
        // it fails for the invariant it names and no other.

        rejected(
            &without(E::Retry),
            "Failed must expose an event that continues it",
        );
        rejected(
            &without(E::Approved),
            "Waiting(NeedsApproval) must expose an event that continues it",
        );
        rejected(
            &without(E::InputGiven),
            "Waiting(Blocked) must expose an event that continues it",
        );
        rejected(
            &retarget(
                executing,
                E::RiskyTool,
                approval,
                edge(planning, E::RiskyTool, approval),
            ),
            "Waiting(NeedsApproval) must only be entered from",
        );
        rejected(
            &retarget(
                reviewing,
                E::Pass,
                completed,
                edge(executing, E::Pass, completed),
            ),
            "Phase(Completed) must only be entered from",
        );
        rejected(
            &retarget(
                blocked,
                E::NoReply,
                expired,
                edge(reviewing, E::NoReply, expired),
            ),
            "Ended(Expired) must only be entered from",
        );
        rejected(
            &retarget(
                approval,
                E::Denied,
                cancelled,
                edge(executing, E::Denied, cancelled),
            ),
            "Ended(Cancelled) must only be entered from",
        );

        let mut leaving_the_rail = RUN_TRANSITIONS.to_vec();
        leaving_the_rail.push(edge(completed, E::Start, queued));
        rejected(
            &leaving_the_rail,
            "Phase(Completed) must not leave the rail",
        );

        let mut ambiguous = RUN_TRANSITIONS.to_vec();
        ambiguous.push(edge(queued, E::Start, completed));
        rejected(&ambiguous, "declares event Start more than once");
    }

    #[test]
    fn canonical_rail_declares_the_events_that_continue_every_state() {
        validate_run_lifecycle().expect("canonical rail is consistent");
        for state in RUN_STATES {
            if state.is_terminal() {
                assert_eq!(
                    run_pending_events(state),
                    [],
                    "{state:?} is final and accepts no further event"
                );
                continue;
            }
            assert!(
                !run_pending_events(state).is_empty(),
                "{state:?} must expose how it continues"
            );
        }
    }
}
