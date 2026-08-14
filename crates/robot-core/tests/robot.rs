//! Contrat comportemental du coeur du robot, vu comme le verra l'adaptateur ROS 2.

use core::time::Duration;

use robot_core::{CommandRejection, Robot, RobotConfig, RobotEvent, RobotState};
use robot_types::{CommandSource, Monotonic, MotionCommand, Velocity2d};

/// Duree de demarrage par defaut, en millisecondes.
const BOOT: u64 = 500;

const TOLERANCE: f64 = 1e-9;

fn t(millis: u64) -> Monotonic {
    Monotonic::from_millis(millis)
}

fn cmd(linear: f64, source: CommandSource, issued_at: Monotonic) -> MotionCommand {
    MotionCommand::new(Velocity2d::new(linear, 0.0), source, issued_at)
}

fn robot_with(config: RobotConfig) -> Robot {
    Robot::new(config).expect("la configuration par defaut est valide")
}

/// Robot en `IDLE`, tampon d'evenements vide, pret pour la phase d'action du test.
fn booted() -> Robot {
    let mut robot = robot_with(RobotConfig::default());
    robot.tick(t(0));
    robot.tick(t(BOOT));
    robot.drain_events().for_each(drop);
    robot
}

/// Amene le robot en teleoperation avec une commande en attente.
fn teleoperating(linear: f64) -> Robot {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("IDLE autorise la teleoperation");
    robot
        .submit_command(cmd(linear, CommandSource::Teleoperation, t(BOOT)), t(BOOT))
        .expect("l'operateur fait autorite en teleoperation");
    robot
}

fn assert_stopped(velocity: Velocity2d) {
    assert!(
        velocity.is_stationary(),
        "vitesse {velocity:?} attendue nulle"
    );
}

// --- Demarrage ----------------------------------------------------------------------

#[test]
fn a_new_robot_starts_in_booting() {
    let robot = robot_with(RobotConfig::default());

    assert_eq!(robot.state(), RobotState::Booting);
}

#[test]
fn the_robot_stays_in_booting_until_the_boot_duration_elapses() {
    let mut robot = robot_with(RobotConfig::default());

    robot.tick(t(0));
    let output = robot.tick(t(BOOT - 1));

    assert_eq!(output.state, RobotState::Booting);
}

#[test]
fn the_robot_reaches_idle_once_the_boot_duration_elapses() {
    let mut robot = robot_with(RobotConfig::default());

    robot.tick(t(0));
    let output = robot.tick(t(BOOT));

    assert_eq!(output.state, RobotState::Idle);
}

#[test]
fn no_motion_is_produced_while_booting() {
    let mut robot = robot_with(RobotConfig::default());

    let output = robot.tick(t(0));

    assert_stopped(output.velocity);
}

#[test]
fn the_uptime_counts_from_the_first_control_cycle() {
    let mut robot = robot_with(RobotConfig::default());
    robot.tick(t(1_000));

    assert_eq!(robot.uptime(t(1_250)), Duration::from_millis(250));
}

// --- Chemin nominal -----------------------------------------------------------------

#[test]
fn an_authorised_command_reaches_the_output() {
    let mut robot = teleoperating(0.2);

    let output = robot.tick(t(BOOT + 20));

    assert_eq!(output.state, RobotState::Teleoperation);
    assert!(output.velocity.linear > 0.0);
}

#[test]
fn the_output_ramps_up_instead_of_jumping_to_the_setpoint() {
    let mut robot = teleoperating(0.5);

    // 0,5 m/s^2 pendant 20 ms n'autorise que 0,01 m/s.
    let output = robot.tick(t(BOOT + 20));

    assert!(
        (output.velocity.linear - 0.01).abs() < TOLERANCE,
        "vitesse {:?} attendue proche de 0,01 m/s",
        output.velocity
    );
}

#[test]
fn an_excessive_command_is_clamped_rather_than_rejected() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    let mut now = BOOT;
    for _ in 0..30 {
        now += 100;
        robot
            .submit_command(cmd(5.0, CommandSource::Teleoperation, t(now)), t(now))
            .expect("une consigne excessive reste une commande valide");
        robot.tick(t(now));
    }

    assert_eq!(robot.state(), RobotState::Teleoperation);
    assert!(
        (robot.velocity_output().linear - 0.5).abs() < TOLERANCE,
        "vitesse {:?} attendue bornee a 0,5 m/s",
        robot.velocity_output()
    );
}

// --- Autorite des commandes ---------------------------------------------------------

#[test]
fn an_ai_command_is_rejected_even_in_teleoperation() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    let rejection = robot
        .submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT))
        .expect_err("l'IA ne pilote jamais les moteurs");

    assert!(matches!(
        rejection,
        CommandRejection::SourceNotAuthorized {
            command_source: CommandSource::Ai,
            ..
        }
    ));
}

#[test]
fn a_teleoperation_command_is_rejected_while_idle() {
    let mut robot = booted();

    let rejection = robot
        .submit_command(cmd(0.2, CommandSource::Teleoperation, t(BOOT)), t(BOOT))
        .expect_err("IDLE n'autorise aucun mouvement");

    assert!(matches!(
        rejection,
        CommandRejection::SourceNotAuthorized {
            state: RobotState::Idle,
            ..
        }
    ));
}

#[test]
fn a_navigation_command_is_rejected_in_teleoperation() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    assert!(
        robot
            .submit_command(cmd(0.2, CommandSource::Navigation, t(BOOT)), t(BOOT))
            .is_err()
    );
}

#[test]
fn a_rejected_command_never_reaches_the_output() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");
    let _ = robot.submit_command(cmd(0.4, CommandSource::Ai, t(BOOT)), t(BOOT));

    let output = robot.tick(t(BOOT + 100));

    assert_stopped(output.velocity);
}

// --- Validation des commandes -------------------------------------------------------

#[test]
fn a_non_finite_command_is_rejected() {
    let mut robot = teleoperating(0.1);

    let rejection = robot
        .submit_command(
            MotionCommand::new(
                Velocity2d::new(f64::NAN, 0.0),
                CommandSource::Teleoperation,
                t(BOOT),
            ),
            t(BOOT),
        )
        .expect_err("une consigne NaN est un defaut logiciel");

    assert_eq!(rejection, CommandRejection::NonFiniteVelocity);
}

#[test]
fn a_command_dated_in_the_future_is_rejected() {
    let mut robot = teleoperating(0.1);

    let rejection = robot
        .submit_command(
            cmd(0.2, CommandSource::Teleoperation, t(BOOT + 100)),
            t(BOOT + 50),
        )
        .expect_err("un horodatage futur signale une horloge desynchronisee");

    assert_eq!(rejection, CommandRejection::FutureTimestamp);
}

#[test]
fn a_stale_command_is_rejected() {
    let mut robot = teleoperating(0.1);

    let rejection = robot
        .submit_command(
            cmd(0.2, CommandSource::Teleoperation, t(BOOT)),
            t(BOOT + 600),
        )
        .expect_err("une commande plus vieille que le timeout est perimee");

    assert!(matches!(rejection, CommandRejection::Stale { .. }));
}

#[test]
fn a_rejected_command_does_not_feed_the_watchdog() {
    let mut robot = teleoperating(0.2);
    robot.tick(t(BOOT + 100));

    // Refusee : elle ne doit pas prolonger la duree de vie du watchdog.
    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT + 400)), t(BOOT + 400));
    let output = robot.tick(t(BOOT + 500));

    assert_eq!(output.state, RobotState::SafeStop);
}

// --- Arret de securite --------------------------------------------------------------

#[test]
fn a_command_timeout_drives_the_robot_to_safe_stop() {
    let mut robot = teleoperating(0.2);
    robot.tick(t(BOOT + 100));

    let output = robot.tick(t(BOOT + 500));

    assert_eq!(output.state, RobotState::SafeStop);
    assert_stopped(output.velocity);
}

#[test]
fn a_steady_command_stream_never_triggers_a_safe_stop() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    let mut now = BOOT;
    for step in 0..40 {
        now += 100;
        robot
            .submit_command(cmd(0.2, CommandSource::Teleoperation, t(now)), t(now))
            .expect("commande valide");
        let output = robot.tick(t(now));
        assert_eq!(
            output.state,
            RobotState::Teleoperation,
            "arret inattendu a l'etape {step}"
        );
    }
}

#[test]
fn no_velocity_is_produced_in_safe_stop() {
    let mut robot = teleoperating(0.5);
    robot.tick(t(BOOT + 100));
    robot.tick(t(BOOT + 500));
    assert_eq!(robot.state(), RobotState::SafeStop);

    let output = robot.tick(t(BOOT + 600));

    assert_stopped(output.velocity);
}

#[test]
fn clearing_the_safe_stop_returns_to_idle() {
    let mut robot = teleoperating(0.2);
    robot.tick(t(BOOT + 500));
    assert_eq!(robot.state(), RobotState::SafeStop);

    robot
        .clear_safe_stop(t(BOOT + 600))
        .expect("aucun arret d'urgence engage");

    assert_eq!(robot.state(), RobotState::Idle);
}

#[test]
fn the_safe_stop_cannot_be_cleared_from_another_state() {
    let mut robot = booted();

    assert!(robot.clear_safe_stop(t(BOOT + 10)).is_err());
    assert_eq!(robot.state(), RobotState::Idle);
}

// --- Arret d'urgence ----------------------------------------------------------------

#[test]
fn the_emergency_stop_drives_the_robot_to_safe_stop_immediately() {
    let mut robot = teleoperating(0.4);
    robot.tick(t(BOOT + 100));

    robot.engage_emergency_stop(t(BOOT + 150));

    assert_eq!(robot.state(), RobotState::SafeStop);
    assert!(robot.is_emergency_stopped());
}

#[test]
fn the_emergency_stop_zeroes_the_output_on_the_next_cycle() {
    let mut robot = teleoperating(0.4);
    robot.tick(t(BOOT + 100));
    robot.engage_emergency_stop(t(BOOT + 150));

    let output = robot.tick(t(BOOT + 160));

    assert_stopped(output.velocity);
}

#[test]
fn the_robot_cannot_leave_safe_stop_while_the_emergency_stop_is_engaged() {
    let mut robot = teleoperating(0.2);
    robot.engage_emergency_stop(t(BOOT + 100));

    assert!(robot.clear_safe_stop(t(BOOT + 200)).is_err());
    assert_eq!(robot.state(), RobotState::SafeStop);
}

#[test]
fn releasing_the_emergency_stop_alone_does_not_resume_the_robot() {
    let mut robot = teleoperating(0.2);
    robot.engage_emergency_stop(t(BOOT + 100));

    robot.clear_emergency_stop(t(BOOT + 200));

    assert!(!robot.is_emergency_stopped());
    assert_eq!(
        robot.state(),
        RobotState::SafeStop,
        "la reprise doit rester explicite"
    );
}

#[test]
fn the_robot_resumes_only_after_both_releases() {
    let mut robot = teleoperating(0.2);
    robot.engage_emergency_stop(t(BOOT + 100));
    robot.clear_emergency_stop(t(BOOT + 200));

    robot
        .clear_safe_stop(t(BOOT + 300))
        .expect("arret d'urgence leve");

    assert_eq!(robot.state(), RobotState::Idle);
}

#[test]
fn no_transition_is_possible_while_the_emergency_stop_is_engaged() {
    let mut robot = booted();
    robot.engage_emergency_stop(t(BOOT + 10));

    assert!(
        robot
            .request_state(RobotState::Teleoperation, t(BOOT + 20))
            .is_err()
    );
    assert_eq!(robot.state(), RobotState::SafeStop);
}

// --- Transitions --------------------------------------------------------------------

#[test]
fn a_forbidden_transition_leaves_the_state_untouched() {
    let mut robot = booted();

    let error = robot
        .request_state(RobotState::Paused, t(BOOT))
        .expect_err("IDLE ne mene pas directement a PAUSED");

    assert!(matches!(
        error,
        robot_core::TransitionError::Forbidden {
            from: RobotState::Idle,
            to: RobotState::Paused
        }
    ));
    assert_eq!(robot.state(), RobotState::Idle);
}

#[test]
fn pausing_a_mission_stops_the_robot() {
    let mut robot = teleoperating(0.3);
    robot.tick(t(BOOT + 100));

    robot
        .request_state(RobotState::Paused, t(BOOT + 110))
        .expect("la teleoperation peut etre suspendue");
    let output = robot.tick(t(BOOT + 120));

    assert_eq!(output.state, RobotState::Paused);
    assert_stopped(output.velocity);
}

#[test]
fn leaving_a_motion_state_discards_the_pending_command() {
    let mut robot = teleoperating(0.3);
    robot.tick(t(BOOT + 100));
    robot
        .request_state(RobotState::Paused, t(BOOT + 110))
        .expect("transition autorisee");

    robot
        .request_state(RobotState::Teleoperation, t(BOOT + 120))
        .expect("reprise autorisee");
    let output = robot.tick(t(BOOT + 130));

    assert_stopped(output.velocity);
}

// --- Evenements ---------------------------------------------------------------------

#[test]
fn a_state_change_emits_an_event() {
    let mut robot = booted();

    robot
        .request_state(RobotState::Navigating, t(BOOT))
        .expect("transition autorisee");

    let events: Vec<_> = robot.drain_events().collect();
    assert!(events.iter().any(|event| matches!(
        event,
        RobotEvent::StateChanged {
            from: RobotState::Idle,
            to: RobotState::Navigating,
            ..
        }
    )));
}

#[test]
fn an_accepted_command_emits_an_event() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");
    robot.drain_events().for_each(drop);

    robot
        .submit_command(cmd(0.2, CommandSource::Teleoperation, t(BOOT)), t(BOOT))
        .expect("commande valide");

    let events: Vec<_> = robot.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|event| event.kind() == "command_accepted")
    );
}

#[test]
fn a_rejected_command_emits_an_event() {
    let mut robot = booted();

    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));

    let events: Vec<_> = robot.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|event| event.kind() == "command_rejected")
    );
}

#[test]
fn a_safe_stop_emits_both_a_violation_and_an_engagement() {
    let mut robot = teleoperating(0.2);
    robot.tick(t(BOOT + 100));
    robot.drain_events().for_each(drop);

    robot.tick(t(BOOT + 500));

    let events: Vec<_> = robot.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|event| event.kind() == "safety_violation")
    );
    assert!(
        events
            .iter()
            .any(|event| event.kind() == "safe_stop_engaged")
    );
}

#[test]
fn a_persistent_violation_is_not_re_reported_every_cycle() {
    // Une condition qui dure — arret d'urgence engage — a deja ete signalee et traitee.
    // La re-emettre a chaque cycle noierait les journaux a 50 evenements par seconde
    // sans rien apprendre a personne.
    let mut robot = teleoperating(0.2);
    robot.engage_emergency_stop(t(BOOT + 100));
    robot.drain_events().for_each(drop);

    for step in 1..=50 {
        robot.tick(t(BOOT + 100 + step * 20));
    }

    let violations = robot
        .drain_events()
        .filter(|event| event.kind() == "safety_violation")
        .count();

    assert!(
        violations <= 1,
        "{violations} constats emis pour une seule condition persistante"
    );
}

#[test]
fn the_violation_that_triggers_the_safe_stop_is_still_reported() {
    let mut robot = teleoperating(0.2);
    robot.tick(t(BOOT + 100));
    robot.drain_events().for_each(drop);

    robot.tick(t(BOOT + 500));

    let events: Vec<_> = robot.drain_events().collect();
    assert!(
        events
            .iter()
            .any(|event| event.kind() == "safety_violation"),
        "le constat a l'origine de l'arret doit etre emis"
    );
}

#[test]
fn draining_the_events_empties_the_buffer() {
    let mut robot = booted();
    robot
        .request_state(RobotState::Navigating, t(BOOT))
        .expect("transition autorisee");

    assert!(robot.drain_events().count() > 0);
    assert_eq!(robot.drain_events().count(), 0);
}

#[test]
fn the_event_buffer_is_bounded_and_counts_what_it_drops() {
    let mut robot = robot_with(RobotConfig {
        event_buffer_capacity: 4,
        ..RobotConfig::default()
    });
    robot.tick(t(0));
    robot.tick(t(BOOT));

    for _ in 0..50 {
        let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    }

    assert!(
        robot.dropped_events() > 0,
        "les abandons doivent etre comptabilises"
    );
    assert!(robot.drain_events().count() <= 4);
}
