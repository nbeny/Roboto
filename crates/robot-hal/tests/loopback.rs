//! Chaine complete calculateur vers microcontroleur, sans materiel.
//!
//! ```text
//! Robot ──► robot-safety ──► cinematique ──► protocole ──► [liaison] ──► MCU simule
//!                                                                            │
//!                                        telemetrie ◄───────────────────────┘
//! ```
//!
//! Le microcontroleur simule ici n'est **pas une doublure de test** : il branche le vrai
//! [`Supervisor`] de `robot-firmware`, celui-la meme qui tournera sur la carte. Seules la
//! liaison serie et l'electronique de puissance sont remplacees. Ce qui est verifie ici
//! est donc du code de production, watchdog compris.

use core::time::Duration;

use robot_core::{Robot, RobotConfig, RobotState};
use robot_firmware::Supervisor;
use robot_hal::{DifferentialDrive, WheelSpeeds};
use robot_mcu::{Decoder, MAX_ENCODED_LEN, Message, StatusFlags, StopReason, Telemetry, encode};
use robot_types::{CommandSource, Monotonic, MotionCommand, Velocity2d};

const TOLERANCE: f64 = 1e-6;

/// Delai du watchdog du microcontroleur, en millisecondes.
const MCU_WATCHDOG_MS: u32 = Supervisor::DEFAULT_TIMEOUT_MS;

/// Geometrie de la nomenclature : roues de 85 mm, empattement 25 cm.
fn drive() -> DifferentialDrive {
    DifferentialDrive::new(0.0425, 0.25).expect("geometrie de la nomenclature")
}

/// Le microcontroleur, reduit a sa liaison serie et a sa supervision.
struct SimulatedMcu {
    decoder: Decoder,
    supervisor: Supervisor,
    rejected: usize,
}

impl SimulatedMcu {
    fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            supervisor: Supervisor::default(),
            rejected: 0,
        }
    }

    /// Consomme des octets recus sur la liaison.
    fn receive(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let Some(result) = self.decoder.push(byte) else {
                continue;
            };

            match result {
                Ok((_, message)) => self.supervisor.handle(&message),
                Err(_) => self.rejected += 1,
            }
        }
    }

    /// Fait avancer l'horloge du microcontroleur et applique son watchdog.
    fn tick(&mut self, now_ms: u32) {
        let _ = self.supervisor.tick(now_ms);
    }

    /// Consigne courante, ramenee dans les unites du calculateur.
    fn setpoint(&self) -> WheelSpeeds {
        let setpoint = self.supervisor.setpoint();
        WheelSpeeds::new(f64::from(setpoint.left), f64::from(setpoint.right))
    }

    fn flags(&self) -> StatusFlags {
        self.supervisor.flags()
    }

    /// Produit la trame de telemetrie que le microcontroleur emettrait.
    fn telemetry(&self, now_ms: u32, sequence: u8) -> ([u8; MAX_ENCODED_LEN], usize) {
        let setpoint = self.supervisor.setpoint();
        let message = Message::Telemetry(Telemetry {
            left: setpoint.left,
            right: setpoint.right,
            left_ticks: 0,
            right_ticks: 0,
            timestamp_ms: now_ms,
            flags: self.supervisor.flags(),
        });

        let mut buffer = [0_u8; MAX_ENCODED_LEN];
        let len = encode(&message, sequence, &mut buffer).expect("telemetrie encodable");
        (buffer, len)
    }
}

/// Encode une consigne de roues telle que l'enverrait le calculateur.
fn wire_setpoint(wheels: WheelSpeeds, now_ms: u32, sequence: u8) -> ([u8; MAX_ENCODED_LEN], usize) {
    let message = Message::WheelVelocity {
        left: wheels.left as f32,
        right: wheels.right as f32,
        timestamp_ms: now_ms,
    };

    let mut buffer = [0_u8; MAX_ENCODED_LEN];
    let len = encode(&message, sequence, &mut buffer).expect("consigne encodable");
    (buffer, len)
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "{what} : obtenu {actual}, attendu {expected}"
    );
}

// --- La chaine complete ---------------------------------------------------------------

#[test]
fn a_core_decision_reaches_the_microcontroller_intact() {
    let drive = drive();
    let mut mcu = SimulatedMcu::new();

    // Le coeur decide une vitesse de chassis.
    let decided = Velocity2d::new(0.3, 0.4);

    // Le calculateur la traduit, l'encode, l'envoie.
    let wheels = drive.to_wheel_speeds(decided);
    let (bytes, len) = wire_setpoint(wheels, 1_000, 0);
    mcu.receive(&bytes[..len]);

    // Le microcontroleur a recu exactement la consigne, et le retour cinematique
    // redonne la decision du coeur.
    let restored = drive.to_body_velocity(mcu.setpoint());

    assert_close(restored.linear, decided.linear, "vitesse lineaire");
    assert_close(restored.angular, decided.angular, "vitesse angulaire");
    assert!(mcu.flags().contains(StatusFlags::MOTORS_ENABLED));
}

#[test]
fn the_full_stack_drives_the_microcontroller_from_a_teleoperation_command() {
    let drive = drive();
    let mut robot = Robot::new(RobotConfig::default()).expect("configuration valide");
    let mut mcu = SimulatedMcu::new();

    let boot = RobotConfig::default().boot_duration;
    robot.tick(Monotonic::ZERO);
    robot.tick(Monotonic::from_since_boot(boot));
    robot
        .request_state(RobotState::Teleoperation, Monotonic::from_since_boot(boot))
        .expect("IDLE autorise la teleoperation");

    // Cent cycles de 20 ms, soit 2 s, avec une consigne excessive de 5 m/s.
    let mut now = boot;
    let mut after_400ms = None;

    for step in 0..100_u32 {
        now += Duration::from_millis(20);
        let at = Monotonic::from_since_boot(now);

        robot
            .submit_command(
                MotionCommand::new(Velocity2d::new(5.0, 0.0), CommandSource::Teleoperation, at),
                at,
            )
            .expect("commande valide");

        let output = robot.tick(at);

        let wheels = drive.to_wheel_speeds(output.velocity);
        let now_ms = u32::try_from(now.as_millis()).expect("horodatage tenant sur 32 bits");
        let (bytes, len) = wire_setpoint(wheels, now_ms, (step % 256) as u8);
        mcu.receive(&bytes[..len]);
        mcu.tick(now_ms);

        if step == 19 {
            after_400ms = Some(drive.to_body_velocity(mcu.setpoint()).linear);
        }
    }

    // La rampe d'acceleration se voit jusqu'aux roues : 0,5 m/s^2 pendant 400 ms ne
    // donne que 0,2 m/s, quelle que soit l'insistance de l'operateur.
    assert_close(
        after_400ms.expect("mesure a 400 ms"),
        0.2,
        "rampe d'acceleration arrivee au MCU",
    );

    // Deux secondes suffisent a atteindre le plafond, ou la saturation de vitesse prend
    // le relais de la rampe.
    let at_the_wheels = drive.to_body_velocity(mcu.setpoint());

    assert_close(at_the_wheels.linear, 0.5, "vitesse bornee arrivee au MCU");
    assert_eq!(mcu.rejected, 0, "aucune trame perdue ou rejetee");
}

#[test]
fn telemetry_travels_back_and_reconstructs_the_body_velocity() {
    let drive = drive();
    let mut mcu = SimulatedMcu::new();
    let mut sbc = Decoder::new();

    let wheels = drive.to_wheel_speeds(Velocity2d::new(0.25, -0.5));
    let (bytes, len) = wire_setpoint(wheels, 500, 0);
    mcu.receive(&bytes[..len]);

    let (bytes, len) = mcu.telemetry(510, 0);

    let mut received = None;
    for &byte in &bytes[..len] {
        if let Some(result) = sbc.push(byte) {
            received = Some(result.expect("telemetrie valide"));
        }
    }

    let Some((_, Message::Telemetry(measured))) = received else {
        panic!("telemetrie non recue");
    };

    let observed = drive.to_body_velocity(WheelSpeeds::new(
        f64::from(measured.left),
        f64::from(measured.right),
    ));

    // Aller-retour a travers des `f32` : la tolerance est celle de la simple precision.
    assert!((observed.linear - 0.25).abs() < 1e-5, "{observed:?}");
    assert!((observed.angular + 0.5).abs() < 1e-5, "{observed:?}");
}

// --- Securite -------------------------------------------------------------------------

#[test]
fn the_microcontroller_watchdog_fires_before_the_core_one() {
    // Hierarchie voulue : 200 ms cote microcontroleur, 500 ms cote coeur. Chaque etage
    // protege contre la defaillance de celui au-dessus.
    let mut mcu = SimulatedMcu::new();
    let (bytes, len) = wire_setpoint(WheelSpeeds::new(10.0, 10.0), 1_000, 0);
    mcu.receive(&bytes[..len]);

    mcu.tick(1_000 + MCU_WATCHDOG_MS - 1);
    assert_ne!(mcu.setpoint(), WheelSpeeds::ZERO, "arret premature");

    mcu.tick(1_000 + MCU_WATCHDOG_MS);

    assert_eq!(
        mcu.setpoint(),
        WheelSpeeds::ZERO,
        "les moteurs doivent etre coupes"
    );
    assert!(mcu.flags().contains(StatusFlags::WATCHDOG_EXPIRED));

    let core_timeout = RobotConfig::default().limits.command_timeout;
    assert!(
        u128::from(MCU_WATCHDOG_MS) < core_timeout.as_millis(),
        "le watchdog du microcontroleur doit se declencher avant celui du coeur"
    );
}

#[test]
fn a_microcontroller_without_any_command_keeps_its_motors_off() {
    let mut mcu = SimulatedMcu::new();

    mcu.tick(5_000);

    assert_eq!(mcu.setpoint(), WheelSpeeds::ZERO);
    assert!(!mcu.flags().contains(StatusFlags::MOTORS_ENABLED));
}

#[test]
fn a_stop_message_cuts_the_motors_immediately() {
    let mut mcu = SimulatedMcu::new();
    let (bytes, len) = wire_setpoint(WheelSpeeds::new(8.0, 8.0), 1_000, 0);
    mcu.receive(&bytes[..len]);

    let mut buffer = [0_u8; MAX_ENCODED_LEN];
    let stop = Message::Stop {
        reason: StopReason::SafeStop,
        timestamp_ms: 1_010,
    };
    let len = encode(&stop, 1, &mut buffer).expect("arret encodable");
    mcu.receive(&buffer[..len]);

    assert_eq!(mcu.setpoint(), WheelSpeeds::ZERO);
    assert!(!mcu.flags().contains(StatusFlags::MOTORS_ENABLED));
}

#[test]
fn line_noise_between_two_frames_costs_nothing_but_a_rejection() {
    let drive = drive();
    let mut mcu = SimulatedMcu::new();

    mcu.receive(&[0x42, 0x13, 0x37, 0x00]);
    assert_eq!(mcu.rejected, 1, "le bruit doit etre rejete");

    let wheels = drive.to_wheel_speeds(Velocity2d::new(0.2, 0.0));
    let (bytes, len) = wire_setpoint(wheels, 2_000, 1);
    mcu.receive(&bytes[..len]);

    assert_close(
        drive.to_body_velocity(mcu.setpoint()).linear,
        0.2,
        "la trame suivant le bruit doit passer",
    );
    assert_eq!(mcu.rejected, 1, "aucun rejet supplementaire");
}

#[test]
fn a_corrupted_setpoint_never_reaches_the_motors() {
    let drive = drive();
    let mut mcu = SimulatedMcu::new();

    let wheels = drive.to_wheel_speeds(Velocity2d::new(0.2, 0.0));
    let (mut bytes, len) = wire_setpoint(wheels, 3_000, 0);
    mcu.receive(&bytes[..len]);
    let accepted = mcu.setpoint();

    // Un bit bascule dans la charge utile.
    bytes[4] ^= 0x01;
    mcu.receive(&bytes[..len]);

    assert_eq!(
        mcu.setpoint(),
        accepted,
        "une trame corrompue ne doit pas modifier la consigne"
    );
    assert_eq!(mcu.rejected, 1);
}

#[test]
fn a_non_finite_velocity_cannot_be_put_on_the_wire() {
    // La couche de securite garantit deja des consignes finies. La frontiere du
    // protocole le verifie une seconde fois : les deux barrieres sont independantes.
    let drive = drive();

    let wheels = drive.to_wheel_speeds(Velocity2d::new(f64::NAN, 0.0));
    assert!(!wheels.is_finite());

    let mut buffer = [0_u8; MAX_ENCODED_LEN];
    let result = encode(
        &Message::WheelVelocity {
            left: wheels.left as f32,
            right: wheels.right as f32,
            timestamp_ms: 0,
        },
        0,
        &mut buffer,
    );

    assert!(result.is_err(), "un NaN ne doit pas atteindre la liaison");
}
