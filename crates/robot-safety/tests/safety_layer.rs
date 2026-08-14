//! Contrat comportemental de la couche de securite.
//!
//! Ces tests decrivent la couche vue de l'exterieur, comme le fait `robot-core`.
//! L'ordre d'evaluation documente dans la crate y est verifie explicitement.

use core::time::Duration;

use robot_safety::{MotionAuthority, SafetyLayer, SafetyLimits, SafetyViolation};
use robot_types::{Monotonic, Velocity2d};

/// Tolerance des comparaisons de vitesse : on compare des grandeurs physiques,
/// pas des motifs binaires.
const TOLERANCE: f64 = 1e-9;

fn layer() -> SafetyLayer {
    SafetyLayer::new(SafetyLimits::default()).expect("les limites par defaut sont valides")
}

fn t(millis: u64) -> Monotonic {
    Monotonic::from_millis(millis)
}

fn v(linear: f64, angular: f64) -> Velocity2d {
    Velocity2d::new(linear, angular)
}

fn assert_velocity_close(actual: Velocity2d, expected: Velocity2d) {
    assert!(
        (actual.linear - expected.linear).abs() < TOLERANCE
            && (actual.angular - expected.angular).abs() < TOLERANCE,
        "vitesse obtenue {actual:?}, attendue proche de {expected:?}"
    );
}

fn contains_kind(violations: &[SafetyViolation], kind: &str) -> bool {
    violations.iter().any(|violation| violation.kind() == kind)
}

/// Fait avancer la couche pour installer un etat de depart, sans examiner le verdict.
///
/// Ignorer une [`robot_safety::SafetyDecision`] est un bug en production, d'ou son
/// `#[must_use]` ; ce detour rend l'intention explicite dans les phases d'arrangement.
fn settle(layer: &mut SafetyLayer, authority: MotionAuthority, now: Monotonic) {
    let _ = layer.evaluate(authority, now);
}

// --- Fonctionnement nominal ---------------------------------------------------------

#[test]
fn a_command_within_the_envelope_passes_through_unchanged() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.3, 0.4)), t(0));

    assert_velocity_close(decision.velocity, v(0.3, 0.4));
    assert!(decision.violations.is_empty());
    assert!(!decision.safe_stop_required);
}

#[test]
fn the_first_evaluation_has_no_acceleration_reference() {
    // Documente un comportement voulu : sans evaluation precedente, il n'existe aucun
    // `dt` sur lequel raisonner. En pratique le coeur evalue des l'etat BOOTING, donc
    // une reference existe toujours avant que le mouvement ne soit autorise.
    let mut layer = layer();
    layer.notify_command_activity(t(1_000));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.5, 0.0)), t(1_000));

    assert_velocity_close(decision.velocity, v(0.5, 0.0));
    assert!(!contains_kind(
        &decision.violations,
        "linear_acceleration_clamped"
    ));
}

#[test]
fn a_denied_authority_produces_zero_velocity_without_violation() {
    let mut layer = layer();

    let decision = layer.evaluate(MotionAuthority::Denied, t(0));

    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(decision.violations.is_empty());
    assert!(!decision.safe_stop_required);
}

// --- Saturation ---------------------------------------------------------------------

#[test]
fn an_excessive_linear_speed_is_clamped_rather_than_rejected() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(2.0, 0.0)), t(0));

    assert_velocity_close(decision.velocity, v(0.5, 0.0));
    assert!(contains_kind(&decision.violations, "linear_speed_clamped"));
    assert!(!decision.safe_stop_required);
}

#[test]
fn an_excessive_angular_speed_is_clamped_in_both_directions() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.0, -4.0)), t(0));

    assert_velocity_close(decision.velocity, v(0.0, -1.0));
    assert!(contains_kind(&decision.violations, "angular_speed_clamped"));
}

#[test]
fn acceleration_is_limited_between_consecutive_evaluations() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(Velocity2d::ZERO), t(0));

    layer.notify_command_activity(t(100));
    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.5, 0.0)), t(100));

    // 0,5 m/s^2 pendant 100 ms autorise une variation de 0,05 m/s.
    assert_velocity_close(decision.velocity, v(0.05, 0.0));
    assert!(contains_kind(
        &decision.violations,
        "linear_acceleration_clamped"
    ));
    assert!(!decision.safe_stop_required);
}

#[test]
fn speed_is_clamped_before_acceleration() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(Velocity2d::ZERO), t(0));

    layer.notify_command_activity(t(100));
    let decision = layer.evaluate(MotionAuthority::Allowed(v(10.0, 0.0)), t(100));

    assert_velocity_close(decision.velocity, v(0.05, 0.0));
    assert!(contains_kind(&decision.violations, "linear_speed_clamped"));
    assert!(contains_kind(
        &decision.violations,
        "linear_acceleration_clamped"
    ));
}

#[test]
fn deceleration_is_limited_symmetrically() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(v(0.5, 0.0)), t(0));

    layer.notify_command_activity(t(100));
    let decision = layer.evaluate(MotionAuthority::Allowed(Velocity2d::ZERO), t(100));

    assert_velocity_close(decision.velocity, v(0.45, 0.0));
}

// --- Arret d'urgence ----------------------------------------------------------------

#[test]
fn an_engaged_emergency_stop_forces_zero_velocity_and_a_safe_stop() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(v(0.3, 0.0)), t(0));

    layer.engage_emergency_stop();
    layer.notify_command_activity(t(50));
    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.3, 0.0)), t(50));

    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(contains_kind(
        &decision.violations,
        "emergency_stop_engaged"
    ));
    assert!(decision.safe_stop_required);
}

#[test]
fn an_emergency_stop_engaged_at_rest_still_requires_a_safe_stop() {
    // Regression : le controle de l'arret d'urgence doit preceder celui de l'autorite,
    // sinon un arret d'urgence declenche a l'arret resterait sans effet sur l'etat.
    let mut layer = layer();

    layer.engage_emergency_stop();
    let decision = layer.evaluate(MotionAuthority::Denied, t(0));

    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(contains_kind(
        &decision.violations,
        "emergency_stop_engaged"
    ));
    assert!(decision.safe_stop_required);
}

#[test]
fn an_emergency_stop_bypasses_the_acceleration_limiter() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(v(0.5, 0.0)), t(0));

    layer.engage_emergency_stop();
    // 1 ms plus tard : le limiteur d'acceleration n'autoriserait que 0,0005 m/s de variation.
    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.5, 0.0)), t(1));

    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
}

#[test]
fn clearing_the_emergency_stop_restores_normal_operation() {
    let mut layer = layer();
    layer.engage_emergency_stop();
    settle(&mut layer, MotionAuthority::Allowed(v(0.2, 0.0)), t(0));

    layer.clear_emergency_stop();
    layer.notify_command_activity(t(1_000));
    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.2, 0.0)), t(1_000));

    assert!(!layer.is_emergency_stopped());
    assert_velocity_close(decision.velocity, v(0.2, 0.0));
    assert!(!decision.safe_stop_required);
}

#[test]
fn a_safety_stop_resets_the_acceleration_baseline_to_zero() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(v(0.5, 0.0)), t(0));

    layer.engage_emergency_stop();
    settle(&mut layer, MotionAuthority::Allowed(v(0.5, 0.0)), t(100));
    layer.clear_emergency_stop();

    layer.notify_command_activity(t(200));
    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.5, 0.0)), t(200));

    // La rampe repart de zero, pas de la vitesse d'avant l'arret.
    assert_velocity_close(decision.velocity, v(0.05, 0.0));
}

// --- Watchdog de commande -----------------------------------------------------------

#[test]
fn a_command_watchdog_expiry_requires_a_safe_stop() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));
    settle(&mut layer, MotionAuthority::Allowed(v(0.3, 0.0)), t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.3, 0.0)), t(500));

    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(contains_kind(&decision.violations, "command_timeout"));
    assert!(decision.safe_stop_required);
}

#[test]
fn a_steady_command_stream_keeps_the_watchdog_satisfied() {
    let mut layer = layer();

    for step in 0..10 {
        let now = t(step * 100);
        layer.notify_command_activity(now);
        let decision = layer.evaluate(MotionAuthority::Allowed(v(0.1, 0.0)), now);
        assert!(
            !decision.safe_stop_required,
            "arret inattendu a l'etape {step}"
        );
    }
}

#[test]
fn a_disarmed_watchdog_cannot_trigger_a_safe_stop() {
    let mut layer = layer();

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.1, 0.0)), t(10_000));

    assert!(!contains_kind(&decision.violations, "command_timeout"));
    assert!(!decision.safe_stop_required);
}

#[test]
fn the_command_activity_age_tracks_the_last_signal() {
    let mut layer = layer();
    assert_eq!(layer.command_activity_age(t(1_000)), None);

    layer.notify_command_activity(t(1_000));

    assert_eq!(
        layer.command_activity_age(t(1_250)),
        Some(Duration::from_millis(250))
    );
}

// --- Consignes aberrantes -----------------------------------------------------------

#[test]
fn a_non_finite_command_never_reaches_the_output() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(f64::NAN, 0.0)), t(0));

    assert!(decision.velocity.is_finite());
    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(contains_kind(&decision.violations, "non_finite_command"));
    assert!(decision.safe_stop_required);
}

#[test]
fn an_infinite_angular_command_never_reaches_the_output() {
    let mut layer = layer();
    layer.notify_command_activity(t(0));

    let decision = layer.evaluate(MotionAuthority::Allowed(v(0.0, f64::INFINITY)), t(0));

    assert!(decision.velocity.is_finite());
    assert_velocity_close(decision.velocity, Velocity2d::ZERO);
    assert!(decision.safe_stop_required);
}
