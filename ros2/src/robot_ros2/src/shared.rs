//! Etat partage entre les callbacks ROS 2 et la boucle de controle.
//!
//! Un seul `Mutex` protege l'ensemble, et c'est voulu : le coeur, l'horloge et la
//! telemetrie forment un tout coherent. Les verrouiller separement autoriserait un
//! `tick` a s'intercaler entre l'horodatage d'une commande et son enregistrement.

use std::time::Duration;

use robot_core::{CommandRejection, ControlOutput, Robot, RobotConfig, RobotState, TransitionError};
use robot_telemetry::{TelemetryCollector, TelemetrySnapshot};
use robot_types::{CommandSource, Monotonic, MonotonicClock, MotionCommand, Velocity2d};

/// Le coeur du robot et ses satellites d'observation.
#[derive(Debug)]
pub struct Shared {
    robot: Robot,
    clock: MonotonicClock,
    telemetry: TelemetryCollector,
}

impl Shared {
    /// Construit l'etat partage a partir d'une configuration de coeur.
    ///
    /// # Errors
    ///
    /// Renvoie une erreur si les limites de securite sont inexploitables.
    pub fn new(config: RobotConfig) -> Result<Self, robot_core::SafetyLimitsError> {
        Ok(Self {
            robot: Robot::new(config)?,
            clock: MonotonicClock::new(),
            telemetry: TelemetryCollector::new(),
        })
    }

    /// Convertit un instant de l'horloge ROS 2 en temps interne.
    ///
    /// Un recul de l'horloge source — relance de Gazebo, resynchronisation — signifie
    /// qu'on ne sait plus combien de temps s'est ecoule depuis la derniere lecture. Le
    /// robot passe donc en `SAFE_STOP` : c'est exactement la situation ou le watchdog
    /// ne peut plus rien garantir.
    pub fn stamp(&mut self, source_nanos: u64) -> Monotonic {
        let reading = self.clock.stamp(source_nanos);

        if reading.rewound {
            tracing::error!(
                event = "clock_rewound",
                backward_step_ms = reading.backward_step.as_millis() as u64,
                internal_ms = reading.now.since_boot().as_millis() as u64,
                "recul de l'horloge source : passage en SAFE_STOP, le temps ecoule n'est plus mesurable"
            );
            let _ = self.robot.request_state(RobotState::SafeStop, reading.now);
        } else if reading.backward_step > Duration::ZERO {
            // Recul absorbe : recalage NTP ou resynchronisation d'une machine virtuelle.
            // Observable sans etre alarmant, mais on veut pouvoir le mesurer.
            tracing::debug!(
                event = "clock_skew_absorbed",
                backward_step_us = reading.backward_step.as_micros() as u64,
                "recul d'horloge sous la tolerance"
            );
        }

        reading.now
    }

    /// Soumet une commande de mouvement recue sur un topic.
    ///
    /// # Errors
    ///
    /// Renvoie [`CommandRejection`] si la commande est invalide ou non habilitee.
    pub fn submit(
        &mut self,
        velocity: Velocity2d,
        source: CommandSource,
        now: Monotonic,
    ) -> Result<(), CommandRejection> {
        self.robot
            .submit_command(MotionCommand::new(velocity, source, now), now)
    }

    /// Demande une transition d'etat.
    ///
    /// # Errors
    ///
    /// Renvoie [`TransitionError`] si la transition est interdite.
    pub fn request_state(
        &mut self,
        target: RobotState,
        now: Monotonic,
    ) -> Result<(), TransitionError> {
        self.robot.request_state(target, now)
    }

    /// Engage l'arret d'urgence logiciel.
    pub fn engage_emergency_stop(&mut self, now: Monotonic) {
        self.robot.engage_emergency_stop(now);
    }

    /// Leve l'arret d'urgence logiciel.
    pub fn clear_emergency_stop(&mut self, now: Monotonic) {
        self.robot.clear_emergency_stop(now);
    }

    /// Quitte `SAFE_STOP`.
    ///
    /// # Errors
    ///
    /// Renvoie [`TransitionError`] si le robot n'est pas en `SAFE_STOP` ou si l'arret
    /// d'urgence est toujours engage.
    pub fn clear_safe_stop(&mut self, now: Monotonic) -> Result<(), TransitionError> {
        self.robot.clear_safe_stop(now)
    }

    /// Execute un cycle de controle et draine les evenements vers la telemetrie.
    pub fn tick(&mut self, now: Monotonic) -> ControlOutput {
        let output = self.robot.tick(now);
        self.telemetry.collect_from(&mut self.robot);
        output
    }

    /// Instantane observable, destine a la publication.
    #[must_use]
    pub fn snapshot(&self, now: Monotonic) -> TelemetrySnapshot {
        self.telemetry.snapshot(&self.robot, now)
    }

    /// Etat operationnel courant.
    #[must_use]
    pub fn state(&self) -> RobotState {
        self.robot.state()
    }

    /// Dernier instant interne produit par l'horloge.
    #[must_use]
    pub fn last_instant(&self) -> Monotonic {
        self.clock.last()
    }
}
