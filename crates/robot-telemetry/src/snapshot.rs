use core::time::Duration;

use robot_core::RobotState;
use robot_types::{Monotonic, Velocity2d};

use crate::counters::EventCounters;

/// Instantane de l'etat observable du robot.
///
/// C'est la structure que l'API TypeScript exposera en Milestone 7, sur
/// `GET /api/robot/status` et sur le canal WebSocket `robot.state`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetrySnapshot {
    /// Etat operationnel courant.
    pub state: RobotState,
    /// Derniere consigne de vitesse autorisee.
    pub velocity: Velocity2d,
    /// Etat de l'arret d'urgence logiciel.
    pub emergency_stop: bool,
    /// Temps ecoule depuis le premier cycle de controle.
    pub uptime: Duration,
    /// Age du dernier signe de vie du flux de commandes, si le watchdog est arme.
    pub command_activity_age: Option<Duration>,
    /// Evenements abandonnes par saturation du tampon du coeur.
    pub dropped_events: u64,
    /// Compteurs cumules.
    pub counters: EventCounters,
    /// Instant de l'instantane.
    pub at: Monotonic,
}
