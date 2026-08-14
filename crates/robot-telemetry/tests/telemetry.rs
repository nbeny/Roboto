//! Contrat de la couche d'observabilite, exercee contre un vrai `Robot`.

use robot_core::{Robot, RobotConfig, RobotState};
use robot_telemetry::TelemetryCollector;
use robot_types::{CommandSource, Monotonic, MotionCommand, Velocity2d};

const BOOT: u64 = 500;

fn t(millis: u64) -> Monotonic {
    Monotonic::from_millis(millis)
}

fn cmd(linear: f64, source: CommandSource, issued_at: Monotonic) -> MotionCommand {
    MotionCommand::new(Velocity2d::new(linear, 0.0), source, issued_at)
}

/// Robot en `IDLE` et collecteur neuf.
///
/// Les evenements du demarrage sont purges sans etre comptabilises, pour que chaque
/// test ne compte que ce qu'il declenche lui-meme.
fn booted() -> (Robot, TelemetryCollector) {
    let mut robot = Robot::new(RobotConfig::default()).expect("configuration valide");

    robot.tick(t(0));
    robot.tick(t(BOOT));
    robot.drain_events().for_each(drop);

    (robot, TelemetryCollector::new())
}

// --- Compteurs ----------------------------------------------------------------------

#[test]
fn a_new_collector_starts_at_zero() {
    let telemetry = TelemetryCollector::new();

    assert_eq!(
        telemetry.counters(),
        robot_telemetry::EventCounters::default()
    );
    assert_eq!(telemetry.counters().total(), 0);
}

#[test]
fn a_state_change_is_counted() {
    let (mut robot, mut telemetry) = booted();

    robot
        .request_state(RobotState::Navigating, t(BOOT))
        .expect("transition autorisee");
    telemetry.collect_from(&mut robot);

    assert_eq!(telemetry.counters().state_transitions, 1);
}

#[test]
fn an_accepted_command_is_counted() {
    let (mut robot, mut telemetry) = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    robot
        .submit_command(cmd(0.2, CommandSource::Teleoperation, t(BOOT)), t(BOOT))
        .expect("commande valide");
    telemetry.collect_from(&mut robot);

    assert_eq!(telemetry.counters().commands_accepted, 1);
    assert_eq!(telemetry.counters().commands_rejected, 0);
}

#[test]
fn a_rejected_command_is_counted() {
    let (mut robot, mut telemetry) = booted();

    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    telemetry.collect_from(&mut robot);

    assert_eq!(telemetry.counters().commands_rejected, 1);
    assert_eq!(telemetry.counters().commands_accepted, 0);
}

#[test]
fn a_safety_violation_and_its_safe_stop_are_counted_separately() {
    let (mut robot, mut telemetry) = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");
    robot
        .submit_command(cmd(0.2, CommandSource::Teleoperation, t(BOOT)), t(BOOT))
        .expect("commande valide");
    robot.tick(t(BOOT + 100));
    telemetry.collect_from(&mut robot);

    // Plus aucune commande : le watchdog expire.
    robot.tick(t(BOOT + 500));
    telemetry.collect_from(&mut robot);

    assert_eq!(robot.state(), RobotState::SafeStop);
    assert_eq!(telemetry.counters().safe_stops, 1);
    assert!(telemetry.counters().safety_violations >= 1);
}

#[test]
fn an_emergency_stop_is_counted() {
    let (mut robot, mut telemetry) = booted();

    robot.engage_emergency_stop(t(BOOT + 10));
    telemetry.collect_from(&mut robot);

    assert_eq!(telemetry.counters().emergency_stops, 1);
}

#[test]
fn counters_accumulate_across_collections() {
    let (mut robot, mut telemetry) = booted();

    for _ in 0..3 {
        let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
        telemetry.collect_from(&mut robot);
    }

    assert_eq!(telemetry.counters().commands_rejected, 3);
}

// --- Etouffement du bruit ------------------------------------------------------------

#[test]
fn a_repeated_rejection_is_not_logged_over_and_over() {
    // Cas concret : le robot passe en SAFE_STOP pendant que Nav2 commande encore. La
    // pile continue de publier a 20 Hz, chaque consigne est refusee, et sans
    // etouffement le journal se remplit de la meme ligne au moment precis ou on a
    // besoin de le lire.
    let (mut robot, mut telemetry) = booted();

    for _ in 0..50 {
        let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
        telemetry.collect_from(&mut robot);
    }

    assert_eq!(
        telemetry.counters().commands_rejected,
        50,
        "le comptage doit rester exact, seul le journal s'allege"
    );
    assert!(
        telemetry.counters().suppressed_logs >= 49,
        "seulement {} lignes etouffees sur 50 refus identiques",
        telemetry.counters().suppressed_logs
    );
}

#[test]
fn a_rejection_of_another_kind_is_reported_again() {
    let (mut robot, mut telemetry) = booted();

    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    telemetry.collect_from(&mut robot);
    let after_first = telemetry.counters().suppressed_logs;

    // Motif different : consigne non finie, et non plus source non habilitee.
    let _ = robot.submit_command(
        MotionCommand::new(Velocity2d::new(f64::NAN, 0.0), CommandSource::Ai, t(BOOT)),
        t(BOOT),
    );
    telemetry.collect_from(&mut robot);

    assert_eq!(
        telemetry.counters().suppressed_logs,
        after_first,
        "un refus de nature differente ne doit pas etre etouffe"
    );
}

#[test]
fn a_state_change_makes_a_rejection_worth_reporting_again() {
    // Apres un changement d'etat, le meme refus n'a plus la meme signification : il
    // decrit desormais une nouvelle situation.
    let (mut robot, mut telemetry) = booted();

    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    telemetry.collect_from(&mut robot);

    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");
    telemetry.collect_from(&mut robot);

    let before = telemetry.counters().suppressed_logs;
    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    telemetry.collect_from(&mut robot);

    assert_eq!(
        telemetry.counters().suppressed_logs,
        before,
        "le premier refus apres un changement d'etat doit etre journalise"
    );
}

// --- Collecte -----------------------------------------------------------------------

#[test]
fn collecting_drains_the_robot_event_buffer() {
    let (mut robot, mut telemetry) = booted();
    robot
        .request_state(RobotState::Navigating, t(BOOT))
        .expect("transition autorisee");

    let collected = telemetry.collect_from(&mut robot);

    assert!(collected > 0);
    assert_eq!(telemetry.collect_from(&mut robot), 0);
}

// --- Instantane ---------------------------------------------------------------------

#[test]
fn the_snapshot_mirrors_the_robot_state() {
    let (mut robot, telemetry) = booted();
    robot
        .request_state(RobotState::Teleoperation, t(BOOT))
        .expect("transition autorisee");

    let snapshot = telemetry.snapshot(&robot, t(BOOT + 10));

    assert_eq!(snapshot.state, RobotState::Teleoperation);
    assert_eq!(snapshot.velocity, robot.velocity_output());
    assert!(!snapshot.emergency_stop);
    assert_eq!(snapshot.at, t(BOOT + 10));
}

#[test]
fn the_snapshot_reports_the_uptime() {
    let (robot, telemetry) = booted();

    let snapshot = telemetry.snapshot(&robot, t(2_000));

    assert_eq!(snapshot.uptime, core::time::Duration::from_millis(2_000));
}

#[test]
fn the_snapshot_reports_the_emergency_stop() {
    let (mut robot, telemetry) = booted();
    robot.engage_emergency_stop(t(BOOT + 10));

    let snapshot = telemetry.snapshot(&robot, t(BOOT + 20));

    assert!(snapshot.emergency_stop);
    assert_eq!(snapshot.state, RobotState::SafeStop);
}

#[test]
fn the_snapshot_carries_the_current_counters() {
    let (mut robot, mut telemetry) = booted();
    let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    telemetry.collect_from(&mut robot);

    let snapshot = telemetry.snapshot(&robot, t(BOOT));

    assert_eq!(snapshot.counters, telemetry.counters());
    assert_eq!(snapshot.counters.commands_rejected, 1);
}

#[test]
fn the_snapshot_surfaces_events_dropped_by_the_core() {
    let mut robot = Robot::new(RobotConfig {
        event_buffer_capacity: 2,
        ..RobotConfig::default()
    })
    .expect("configuration valide");
    let telemetry = TelemetryCollector::new();
    robot.tick(t(0));
    robot.tick(t(BOOT));

    for _ in 0..20 {
        let _ = robot.submit_command(cmd(0.2, CommandSource::Ai, t(BOOT)), t(BOOT));
    }

    let snapshot = telemetry.snapshot(&robot, t(BOOT));

    assert!(snapshot.dropped_events > 0);
}
