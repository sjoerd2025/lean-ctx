// SPDX-License-Identifier: Apache-2.0
//! Frozen deployment-release lifecycle reference behavior.
//!
//! A release object moves through five ordered promotion phases
//! ([`ReleasePhaseV1`]). Human approval and a production health regression hold
//! it in a [`ReleaseWaitV1`] wait state that is not an ending and consumes no
//! worker; [`release_pending_events`] names the events that continue such a
//! release. Denied approval, a failed rollback controller, a completed rollback,
//! and a withdrawn change end the release in an explicit terminal
//! [`ReleaseEndingV1`].
//!
//! Verification and approval are distinct states: approval is only reachable
//! after verification reports `checks pass` or `needs sign-off`. `Live` means
//! production health is currently proven, so a health regression leaves it
//! without ending the release.
//!
//! This module models the public promotion rail only. It deploys nothing,
//! observes no production system, and takes no promotion decision; production
//! rollout intelligence remains Class D in `lean-ctx-enterprise`.

use crate::ValidationError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Ordered promotion phase of a release that has not ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleasePhaseV1 {
    /// Change accepted; waiting for a builder to pick it up.
    Queued,
    /// Building the immutable release image.
    Building,
    /// Running tests and policy gates against the built image.
    Verifying,
    /// Checks passed; promotion is pending.
    Ready,
    /// Promoted, with production health currently proven.
    Live,
}

/// The five ordered promotion phases, from acceptance to live.
pub const RELEASE_PHASE_ORDER: [ReleasePhaseV1; 5] = [
    ReleasePhaseV1::Queued,
    ReleasePhaseV1::Building,
    ReleasePhaseV1::Verifying,
    ReleasePhaseV1::Ready,
    ReleasePhaseV1::Live,
];

/// Wait state that holds a release without ending it or consuming a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseWaitV1 {
    /// Release owner sign-off is required before promotion.
    NeedsApproval,
    /// A production health regression paused further rollout.
    HealthPaused,
}

impl ReleaseWaitV1 {
    /// Events that resolve this wait; an unresolved wait never ends the release.
    pub fn resolution_events(self) -> &'static [ReleaseEventV1] {
        match self {
            Self::NeedsApproval => &[ReleaseEventV1::Approved, ReleaseEventV1::Denied],
            Self::HealthPaused => &[
                ReleaseEventV1::HealthRecovered,
                ReleaseEventV1::BreachPersists,
            ],
        }
    }
}

/// Terminal ending of a release; a terminal release accepts no further event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEndingV1 {
    /// Approval denied; the release never reaches `Ready`.
    Cancelled,
    /// Rollback completed and the previous image serves production again.
    RolledBack,
    /// The rollback controller failed; production is not restored.
    Failed,
    /// The change was withdrawn before promotion.
    Aborted,
}

/// State of one release object on the promotion rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum ReleaseStateV1 {
    /// The release is on an ordered promotion phase.
    Phase(ReleasePhaseV1),
    /// The release is held in a wait state.
    Waiting(ReleaseWaitV1),
    /// Rollback to the last good image is in progress.
    RollingBack,
    /// The release reached a terminal ending.
    Ended(ReleaseEndingV1),
}

impl ReleaseStateV1 {
    /// Return the promotion phase, when the release is on the rail.
    pub fn phase(self) -> Option<ReleasePhaseV1> {
        match self {
            Self::Phase(phase) => Some(phase),
            Self::Waiting(_) | Self::RollingBack | Self::Ended(_) => None,
        }
    }

    /// Return the wait state, when the release is held.
    pub fn wait(self) -> Option<ReleaseWaitV1> {
        match self {
            Self::Waiting(wait) => Some(wait),
            Self::Phase(_) | Self::RollingBack | Self::Ended(_) => None,
        }
    }

    /// Return the ending, when the release already ended.
    pub fn ending(self) -> Option<ReleaseEndingV1> {
        match self {
            Self::Ended(ending) => Some(ending),
            Self::Phase(_) | Self::Waiting(_) | Self::RollingBack => None,
        }
    }

    /// Report whether the release reached a terminal ending.
    pub fn is_terminal(self) -> bool {
        self.ending().is_some()
    }
}

/// Event that moves a release along one rail edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEventV1 {
    /// A builder picked the queued change up.
    PickUp,
    /// The immutable image was built.
    ImageBuilt,
    /// Tests and policy gates passed.
    ChecksPass,
    /// Tests or policy gates failed; the release returns to building.
    ChecksFail,
    /// The release owner must sign off before promotion.
    NeedsSignOff,
    /// The release owner approved promotion.
    Approved,
    /// The release owner denied promotion.
    Denied,
    /// The ready image is promoted to production.
    Promote,
    /// Production health regressed against its objective.
    SloAlert,
    /// Production health recovered; rollout continues.
    HealthRecovered,
    /// The regression persists; the last good image is restored.
    BreachPersists,
    /// Rollback restored the previous image.
    Restored,
    /// The rollback controller failed.
    RollbackError,
    /// The change was withdrawn before promotion.
    Abort,
}

/// Every rail event, in artifact order.
pub const RELEASE_EVENTS: [ReleaseEventV1; 14] = [
    ReleaseEventV1::PickUp,
    ReleaseEventV1::ImageBuilt,
    ReleaseEventV1::ChecksPass,
    ReleaseEventV1::ChecksFail,
    ReleaseEventV1::NeedsSignOff,
    ReleaseEventV1::Approved,
    ReleaseEventV1::Denied,
    ReleaseEventV1::Promote,
    ReleaseEventV1::SloAlert,
    ReleaseEventV1::HealthRecovered,
    ReleaseEventV1::BreachPersists,
    ReleaseEventV1::Restored,
    ReleaseEventV1::RollbackError,
    ReleaseEventV1::Abort,
];

/// One directed edge of the promotion rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTransitionV1 {
    /// State the edge leaves.
    pub from: ReleaseStateV1,
    /// Event that selects the edge.
    pub event: ReleaseEventV1,
    /// State the edge enters.
    pub to: ReleaseStateV1,
}

/// The canonical promotion rail: every state and event pair a release accepts.
pub const RELEASE_TRANSITIONS: [ReleaseTransitionV1; 15] = [
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Queued),
        event: ReleaseEventV1::PickUp,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Building),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Building),
        event: ReleaseEventV1::ImageBuilt,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
        event: ReleaseEventV1::ChecksPass,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Ready),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Ready),
        event: ReleaseEventV1::Promote,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Live),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
        event: ReleaseEventV1::ChecksFail,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Building),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
        event: ReleaseEventV1::NeedsSignOff,
        to: ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval),
        event: ReleaseEventV1::Approved,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Ready),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval),
        event: ReleaseEventV1::Denied,
        to: ReleaseStateV1::Ended(ReleaseEndingV1::Cancelled),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Live),
        event: ReleaseEventV1::SloAlert,
        to: ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused),
        event: ReleaseEventV1::HealthRecovered,
        to: ReleaseStateV1::Phase(ReleasePhaseV1::Live),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused),
        event: ReleaseEventV1::BreachPersists,
        to: ReleaseStateV1::RollingBack,
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::RollingBack,
        event: ReleaseEventV1::Restored,
        to: ReleaseStateV1::Ended(ReleaseEndingV1::RolledBack),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::RollingBack,
        event: ReleaseEventV1::RollbackError,
        to: ReleaseStateV1::Ended(ReleaseEndingV1::Failed),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Building),
        event: ReleaseEventV1::Abort,
        to: ReleaseStateV1::Ended(ReleaseEndingV1::Aborted),
    },
    ReleaseTransitionV1 {
        from: ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
        event: ReleaseEventV1::Abort,
        to: ReleaseStateV1::Ended(ReleaseEndingV1::Aborted),
    },
];

/// Return the state entered from `from` by `event`, when the rail accepts it.
pub fn release_transition(from: ReleaseStateV1, event: ReleaseEventV1) -> Option<ReleaseStateV1> {
    RELEASE_TRANSITIONS
        .iter()
        .find(|transition| transition.from == from && transition.event == event)
        .map(|transition| transition.to)
}

/// Events that continue or resolve a release in `state`.
///
/// A wait state returns its resolution events, so a paused release never needs
/// a worker to discover what continues it. Terminal endings return nothing.
pub fn release_pending_events(state: ReleaseStateV1) -> &'static [ReleaseEventV1] {
    match state {
        ReleaseStateV1::Phase(ReleasePhaseV1::Queued) => &[ReleaseEventV1::PickUp],
        ReleaseStateV1::Phase(ReleasePhaseV1::Building) => {
            &[ReleaseEventV1::ImageBuilt, ReleaseEventV1::Abort]
        }
        ReleaseStateV1::Phase(ReleasePhaseV1::Verifying) => &[
            ReleaseEventV1::ChecksPass,
            ReleaseEventV1::ChecksFail,
            ReleaseEventV1::NeedsSignOff,
            ReleaseEventV1::Abort,
        ],
        ReleaseStateV1::Phase(ReleasePhaseV1::Ready) => &[ReleaseEventV1::Promote],
        ReleaseStateV1::Phase(ReleasePhaseV1::Live) => &[ReleaseEventV1::SloAlert],
        ReleaseStateV1::Waiting(wait) => wait.resolution_events(),
        ReleaseStateV1::RollingBack => &[ReleaseEventV1::Restored, ReleaseEventV1::RollbackError],
        ReleaseStateV1::Ended(_) => &[],
    }
}

/// Reason a release could not accept an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseLifecycleError {
    /// No rail edge accepts the event in the current state.
    Rejected {
        /// State the release was in.
        from: ReleaseStateV1,
        /// Event the caller supplied.
        event: ReleaseEventV1,
    },
    /// The release already ended; terminal endings absorb no event.
    Ended {
        /// Ending the release reached.
        ending: ReleaseEndingV1,
        /// Event the caller supplied.
        event: ReleaseEventV1,
    },
}

impl fmt::Display for ReleaseLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, event } => write!(
                formatter,
                "release event {event:?} is not accepted in state {from:?}"
            ),
            Self::Ended { ending, event } => write!(
                formatter,
                "release ended as {ending:?} and accepts no further {event:?}"
            ),
        }
    }
}

impl std::error::Error for ReleaseLifecycleError {}

/// Mutable release object: exactly one promotion state at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLifecycleV1 {
    /// Current state; a release starts queued when the change is accepted.
    pub state: ReleaseStateV1,
}

impl Default for ReleaseLifecycleV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl ReleaseLifecycleV1 {
    /// Start a release whose change was accepted.
    pub fn new() -> Self {
        Self {
            state: ReleaseStateV1::Phase(ReleasePhaseV1::Queued),
        }
    }

    /// Return the current state.
    pub fn state(&self) -> ReleaseStateV1 {
        self.state
    }

    /// Apply `event`, moving the release one rail edge.
    ///
    /// A rejected event leaves the release unchanged, so a caller may retry with
    /// the events [`release_pending_events`] names for the current state.
    pub fn advance(
        &mut self,
        event: ReleaseEventV1,
    ) -> Result<ReleaseStateV1, ReleaseLifecycleError> {
        if let ReleaseStateV1::Ended(ending) = self.state {
            return Err(ReleaseLifecycleError::Ended { ending, event });
        }
        match release_transition(self.state, event) {
            Some(state) => {
                self.state = state;
                Ok(state)
            }
            None => Err(ReleaseLifecycleError::Rejected {
                from: self.state,
                event,
            }),
        }
    }
}

/// Validate the canonical rail and its declared pending events.
pub fn validate_release_lifecycle() -> Result<(), ValidationError> {
    validate_release_transitions(&RELEASE_TRANSITIONS)?;
    let held = [
        ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval),
        ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused),
        ReleaseStateV1::RollingBack,
        ReleaseStateV1::Ended(ReleaseEndingV1::Cancelled),
        ReleaseStateV1::Ended(ReleaseEndingV1::RolledBack),
        ReleaseStateV1::Ended(ReleaseEndingV1::Failed),
        ReleaseStateV1::Ended(ReleaseEndingV1::Aborted),
    ];
    let states = RELEASE_PHASE_ORDER
        .iter()
        .map(|phase| ReleaseStateV1::Phase(*phase))
        .chain(held);
    for state in states {
        let declared = release_pending_events(state);
        let edges = RELEASE_TRANSITIONS
            .iter()
            .filter(|transition| transition.from == state)
            .count();
        if state.is_terminal() {
            if edges > 0 {
                return Err(ValidationError::new(format!(
                    "terminal release state {state:?} must not leave the rail"
                )));
            }
            continue;
        }
        if edges == 0 {
            return Err(ValidationError::new(format!(
                "release state {state:?} must be continuable"
            )));
        }
        if edges != declared.len() {
            return Err(ValidationError::new(format!(
                "release state {state:?} declares {} pending events for {edges} rail edges",
                declared.len()
            )));
        }
        if let Some(missing) = declared
            .iter()
            .find(|event| release_transition(state, **event).is_none())
        {
            return Err(ValidationError::new(format!(
                "release state {state:?} declares unsupported event {missing:?}"
            )));
        }
    }
    Ok(())
}

/// Validate one rail: the shape rules every promotion rail must satisfy.
pub fn validate_release_transitions(
    transitions: &[ReleaseTransitionV1],
) -> Result<(), ValidationError> {
    for transition in transitions {
        if transition.from.is_terminal() {
            return Err(ValidationError::new(format!(
                "terminal release state {:?} must not leave the rail",
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
                "release state {:?} declares event {:?} more than once",
                transition.from, transition.event
            )));
        }
    }
    require_sources(
        transitions,
        ReleaseStateV1::Phase(ReleasePhaseV1::Live),
        &[
            ReleaseStateV1::Phase(ReleasePhaseV1::Ready),
            ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused),
        ],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval),
        &[ReleaseStateV1::Phase(ReleasePhaseV1::Verifying)],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::RollingBack,
        &[ReleaseStateV1::Waiting(ReleaseWaitV1::HealthPaused)],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::Ended(ReleaseEndingV1::Cancelled),
        &[ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval)],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::Ended(ReleaseEndingV1::RolledBack),
        &[ReleaseStateV1::RollingBack],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::Ended(ReleaseEndingV1::Failed),
        &[ReleaseStateV1::RollingBack],
    )?;
    require_sources(
        transitions,
        ReleaseStateV1::Ended(ReleaseEndingV1::Aborted),
        &[
            ReleaseStateV1::Phase(ReleasePhaseV1::Building),
            ReleaseStateV1::Phase(ReleasePhaseV1::Verifying),
        ],
    )?;
    Ok(())
}

/// Reject a rail where `target` is entered from an unlisted state.
fn require_sources(
    transitions: &[ReleaseTransitionV1],
    target: ReleaseStateV1,
    allowed: &[ReleaseStateV1],
) -> Result<(), ValidationError> {
    for transition in transitions.iter().filter(|edge| edge.to == target) {
        if !allowed.contains(&transition.from) {
            return Err(ValidationError::new(format!(
                "release state {:?} must only be entered from {allowed:?}, found {:?}",
                target, transition.from
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a release through `events`, failing on the first rejected event.
    fn run(events: &[ReleaseEventV1]) -> ReleaseLifecycleV1 {
        let mut release = ReleaseLifecycleV1::new();
        for event in events {
            release.advance(*event).expect("event is on the rail");
        }
        release
    }

    /// Drive a release that ends as `ending`, so terminal assertions can start
    /// from a real rail traversal instead of a hand-built state.
    fn ended(ending: ReleaseEndingV1) -> ReleaseLifecycleV1 {
        use ReleaseEventV1 as E;
        match ending {
            ReleaseEndingV1::Aborted => run(&[E::PickUp, E::Abort]),
            ReleaseEndingV1::Cancelled => {
                run(&[E::PickUp, E::ImageBuilt, E::NeedsSignOff, E::Denied])
            }
            ReleaseEndingV1::RolledBack => run(&[
                E::PickUp,
                E::ImageBuilt,
                E::ChecksPass,
                E::Promote,
                E::SloAlert,
                E::BreachPersists,
                E::Restored,
            ]),
            ReleaseEndingV1::Failed => run(&[
                E::PickUp,
                E::ImageBuilt,
                E::ChecksPass,
                E::Promote,
                E::SloAlert,
                E::BreachPersists,
                E::RollbackError,
            ]),
        }
    }

    #[test]
    fn promotion_rail_advances_through_five_ordered_phases() {
        use ReleaseEventV1 as E;
        let events = [E::PickUp, E::ImageBuilt, E::ChecksPass, E::Promote];
        let mut release = ReleaseLifecycleV1::new();
        assert_eq!(
            release.state(),
            ReleaseStateV1::Phase(ReleasePhaseV1::Queued)
        );
        for (step, event) in events.iter().enumerate() {
            let state = release.advance(*event).expect("rail edge");
            assert_eq!(state.phase(), Some(RELEASE_PHASE_ORDER[step + 1]));
        }
        assert_eq!(release.state().phase(), Some(ReleasePhaseV1::Live));
        assert!(
            !release.state().is_terminal(),
            "a live release is not an ending"
        );
    }

    #[test]
    fn approval_waits_after_verification_and_promotes_through_ready() {
        use ReleaseEventV1 as E;
        let mut release = run(&[E::PickUp, E::ImageBuilt]);

        release.advance(E::NeedsSignOff).expect("sign-off hold");
        assert_eq!(
            release.state().wait(),
            Some(ReleaseWaitV1::NeedsApproval),
            "approval is a wait state, not a promotion phase"
        );
        assert_eq!(
            release_pending_events(release.state()),
            [E::Approved, E::Denied],
            "the hold names the events that continue it"
        );

        release
            .advance(E::Approved)
            .expect("approval resumes the rail");
        assert_eq!(
            release.state().phase(),
            Some(ReleasePhaseV1::Ready),
            "approval resumes at promotion pending, never straight to live"
        );
        release.advance(E::Promote).expect("promotion");
        assert_eq!(release.state().phase(), Some(ReleasePhaseV1::Live));
    }

    #[test]
    fn denied_approval_ends_as_cancelled() {
        use ReleaseEventV1 as E;
        let mut release = run(&[E::PickUp, E::ImageBuilt, E::NeedsSignOff]);
        assert_eq!(
            release.advance(E::Denied).expect("denial"),
            ReleaseStateV1::Ended(ReleaseEndingV1::Cancelled)
        );
        assert_eq!(
            release.state().ending(),
            Some(ReleaseEndingV1::Cancelled),
            "denied approval is terminal and never reaches ready"
        );
    }

    #[test]
    fn failed_checks_rebuild_the_image_before_promotion() {
        use ReleaseEventV1 as E;
        let mut release = run(&[E::PickUp, E::ImageBuilt]);
        release.advance(E::ChecksFail).expect("recovery loop");
        assert_eq!(
            release.state().phase(),
            Some(ReleasePhaseV1::Building),
            "tests fail returns the release to building"
        );
        release.advance(E::ImageBuilt).expect("rebuild");
        assert_eq!(
            release.state().phase(),
            Some(ReleasePhaseV1::Verifying),
            "the rebuilt image is verified again"
        );
    }

    #[test]
    fn health_regression_pauses_a_live_release_without_ending_it() {
        use ReleaseEventV1 as E;
        let mut release = run(&[E::PickUp, E::ImageBuilt, E::ChecksPass, E::Promote]);
        release.advance(E::SloAlert).expect("health pause");
        assert_eq!(release.state().wait(), Some(ReleaseWaitV1::HealthPaused));
        assert!(!release.state().is_terminal());
        assert_eq!(
            release_pending_events(release.state()),
            [E::HealthRecovered, E::BreachPersists]
        );
        release
            .advance(E::HealthRecovered)
            .expect("rollout resumes");
        assert_eq!(release.state().phase(), Some(ReleasePhaseV1::Live));
    }

    #[test]
    fn rollback_ends_as_restored_or_failed() {
        use ReleaseEventV1 as E;
        let rollback = [
            E::PickUp,
            E::ImageBuilt,
            E::ChecksPass,
            E::Promote,
            E::SloAlert,
            E::BreachPersists,
        ];

        let mut restored = run(&rollback);
        assert_eq!(
            restored.advance(E::Restored).expect("restore"),
            ReleaseStateV1::Ended(ReleaseEndingV1::RolledBack),
            "a successful rollback is a terminal restored outcome"
        );

        let mut failed = run(&rollback);
        assert_eq!(
            failed.advance(E::RollbackError).expect("rollback error"),
            ReleaseStateV1::Ended(ReleaseEndingV1::Failed),
            "a rollback controller failure ends as failed"
        );
        assert_eq!(
            release_pending_events(restored.state()),
            [],
            "terminal endings accept no further event"
        );
    }

    #[test]
    fn terminal_endings_reject_every_event_and_keep_their_ending() {
        for ending in [
            ReleaseEndingV1::Cancelled,
            ReleaseEndingV1::RolledBack,
            ReleaseEndingV1::Failed,
            ReleaseEndingV1::Aborted,
        ] {
            let mut release = ended(ending);
            assert!(release.state().is_terminal());
            for event in RELEASE_EVENTS {
                assert_eq!(
                    release.advance(event),
                    Err(ReleaseLifecycleError::Ended { ending, event }),
                    "{ending:?} must absorb {event:?}"
                );
                assert_eq!(
                    release.state().ending(),
                    Some(ending),
                    "a rejected event must not move a terminal release"
                );
            }
        }
    }

    #[test]
    fn events_outside_the_rail_are_rejected_without_moving_the_release() {
        use ReleaseEventV1 as E;
        let cases = [
            (ReleaseLifecycleV1::new(), E::Promote),
            (ReleaseLifecycleV1::new(), E::Denied),
            (run(&[E::PickUp]), E::ChecksPass),
            (run(&[E::PickUp]), E::Restored),
            (run(&[E::PickUp, E::ImageBuilt, E::ChecksPass]), E::Abort),
            (
                run(&[E::PickUp, E::ImageBuilt, E::ChecksPass, E::Promote]),
                E::Abort,
            ),
            (
                run(&[E::PickUp, E::ImageBuilt, E::ChecksPass, E::Promote]),
                E::Denied,
            ),
        ];
        for (mut release, event) in cases {
            let before = release.state();
            assert_eq!(
                release.advance(event),
                Err(ReleaseLifecycleError::Rejected {
                    from: before,
                    event
                }),
                "{event:?} must not be accepted in {before:?}"
            );
            assert_eq!(release.state(), before, "a rejected event changes nothing");
        }
    }

    #[test]
    fn a_rail_that_breaks_a_promotion_invariant_is_rejected() {
        let queued = ReleaseStateV1::Phase(ReleasePhaseV1::Queued);
        let verifying = ReleaseStateV1::Phase(ReleasePhaseV1::Verifying);
        let approval = ReleaseStateV1::Waiting(ReleaseWaitV1::NeedsApproval);
        let live = ReleaseStateV1::Phase(ReleasePhaseV1::Live);
        let cancelled = ReleaseStateV1::Ended(ReleaseEndingV1::Cancelled);
        let edge = |from, event, to| ReleaseTransitionV1 { from, event, to };

        let approval_without_verification = [
            edge(queued, ReleaseEventV1::PickUp, approval),
            edge(approval, ReleaseEventV1::Approved, live),
        ];
        assert!(validate_release_transitions(&approval_without_verification).is_err());

        let promotion_bypassing_ready = [
            edge(queued, ReleaseEventV1::PickUp, live),
            edge(live, ReleaseEventV1::SloAlert, approval),
        ];
        assert!(validate_release_transitions(&promotion_bypassing_ready).is_err());

        let cancellation_without_denial = [
            edge(verifying, ReleaseEventV1::ChecksFail, cancelled),
            edge(cancelled, ReleaseEventV1::Denied, queued),
        ];
        assert!(validate_release_transitions(&cancellation_without_denial).is_err());

        let ambiguous_rail = [
            edge(queued, ReleaseEventV1::PickUp, verifying),
            edge(queued, ReleaseEventV1::PickUp, live),
        ];
        assert!(validate_release_transitions(&ambiguous_rail).is_err());
    }

    #[test]
    fn canonical_rail_declares_the_events_that_continue_every_state() {
        validate_release_lifecycle().expect("canonical rail is consistent");
        for state in RELEASE_PHASE_ORDER.map(ReleaseStateV1::Phase) {
            if state == ReleaseStateV1::Phase(ReleasePhaseV1::Live) {
                continue;
            }
            assert!(
                !release_pending_events(state).is_empty(),
                "{state:?} must expose how it continues"
            );
        }
    }
}
