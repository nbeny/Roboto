//! Table des transitions et table d'habilitation, verifiees isolement.

use robot_core::RobotState;
use robot_types::CommandSource;

const ALL_STATES: [RobotState; 8] = [
    RobotState::Booting,
    RobotState::Idle,
    RobotState::Teleoperation,
    RobotState::Navigating,
    RobotState::Paused,
    RobotState::Error,
    RobotState::SafeStop,
    RobotState::Charging,
];

const ALL_SOURCES: [CommandSource; 4] = [
    CommandSource::Teleoperation,
    CommandSource::Navigation,
    CommandSource::Ai,
    CommandSource::Internal,
];

// --- Mouvement ----------------------------------------------------------------------

#[test]
fn only_teleoperation_and_navigation_allow_motion() {
    for state in ALL_STATES {
        let expected = matches!(state, RobotState::Teleoperation | RobotState::Navigating);

        assert_eq!(
            state.allows_motion(),
            expected,
            "l'etat {state} n'a pas la bonne autorisation de mouvement"
        );
    }
}

// --- Transitions --------------------------------------------------------------------

#[test]
fn a_safe_stop_is_reachable_from_every_other_state() {
    for state in ALL_STATES {
        if state == RobotState::SafeStop {
            continue;
        }

        assert!(
            state.can_transition_to(RobotState::SafeStop),
            "l'etat {state} ne peut pas atteindre SAFE_STOP"
        );
    }
}

#[test]
fn an_error_is_reachable_from_every_other_state() {
    for state in ALL_STATES {
        if state == RobotState::Error {
            continue;
        }

        assert!(
            state.can_transition_to(RobotState::Error),
            "l'etat {state} ne peut pas atteindre ERROR"
        );
    }
}

#[test]
fn a_state_never_transitions_to_itself() {
    for state in ALL_STATES {
        assert!(
            !state.can_transition_to(state),
            "l'etat {state} se declare une transition vers lui-meme"
        );
    }
}

#[test]
fn booting_only_leads_to_idle() {
    assert!(RobotState::Booting.can_transition_to(RobotState::Idle));
    assert!(!RobotState::Booting.can_transition_to(RobotState::Teleoperation));
    assert!(!RobotState::Booting.can_transition_to(RobotState::Navigating));
    assert!(!RobotState::Booting.can_transition_to(RobotState::Charging));
}

#[test]
fn idle_opens_the_operational_states() {
    assert!(RobotState::Idle.can_transition_to(RobotState::Teleoperation));
    assert!(RobotState::Idle.can_transition_to(RobotState::Navigating));
    assert!(RobotState::Idle.can_transition_to(RobotState::Charging));
    assert!(!RobotState::Idle.can_transition_to(RobotState::Booting));
    assert!(!RobotState::Idle.can_transition_to(RobotState::Paused));
}

#[test]
fn a_mission_can_be_paused_and_resumed() {
    assert!(RobotState::Navigating.can_transition_to(RobotState::Paused));
    assert!(RobotState::Paused.can_transition_to(RobotState::Navigating));
    assert!(RobotState::Teleoperation.can_transition_to(RobotState::Paused));
    assert!(RobotState::Paused.can_transition_to(RobotState::Teleoperation));
    assert!(RobotState::Paused.can_transition_to(RobotState::Idle));
}

#[test]
fn charging_must_pass_through_idle_before_moving() {
    assert!(RobotState::Charging.can_transition_to(RobotState::Idle));
    assert!(!RobotState::Charging.can_transition_to(RobotState::Navigating));
    assert!(!RobotState::Charging.can_transition_to(RobotState::Teleoperation));
}

#[test]
fn recovery_states_only_return_to_idle() {
    for state in [RobotState::SafeStop, RobotState::Error] {
        assert!(state.can_transition_to(RobotState::Idle));
        assert!(!state.can_transition_to(RobotState::Teleoperation));
        assert!(!state.can_transition_to(RobotState::Navigating));
        assert!(!state.can_transition_to(RobotState::Paused));
        assert!(!state.can_transition_to(RobotState::Charging));
    }
}

#[test]
fn booting_is_never_reachable_again() {
    for state in ALL_STATES {
        assert!(
            !state.can_transition_to(RobotState::Booting),
            "l'etat {state} pretend pouvoir revenir en BOOTING"
        );
    }
}

// --- Habilitation des commandes -----------------------------------------------------

#[test]
fn the_ai_is_never_authorised_to_command_motion() {
    for state in ALL_STATES {
        assert!(
            !state.accepts_command_from(CommandSource::Ai),
            "l'etat {state} accepte une consigne brute de l'IA"
        );
    }
}

#[test]
fn teleoperation_only_accepts_the_operator_and_the_core() {
    let state = RobotState::Teleoperation;

    assert!(state.accepts_command_from(CommandSource::Teleoperation));
    assert!(state.accepts_command_from(CommandSource::Internal));
    assert!(!state.accepts_command_from(CommandSource::Navigation));
}

#[test]
fn navigation_only_accepts_the_navigation_stack_and_the_core() {
    let state = RobotState::Navigating;

    assert!(state.accepts_command_from(CommandSource::Navigation));
    assert!(state.accepts_command_from(CommandSource::Internal));
    assert!(!state.accepts_command_from(CommandSource::Teleoperation));
}

#[test]
fn a_state_that_forbids_motion_accepts_no_command_at_all() {
    for state in ALL_STATES {
        if state.allows_motion() {
            continue;
        }

        for source in ALL_SOURCES {
            assert!(
                !state.accepts_command_from(source),
                "l'etat {state} accepte une commande de `{source}` alors qu'il interdit le mouvement"
            );
        }
    }
}

// --- Libelles -----------------------------------------------------------------------

#[test]
fn states_expose_stable_labels() {
    assert_eq!(RobotState::SafeStop.as_str(), "SAFE_STOP");
    assert_eq!(RobotState::Teleoperation.to_string(), "TELEOPERATION");
}
