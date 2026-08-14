use core::time::Duration;

use robot_safety::SafetyViolation;
use robot_types::{CommandSource, Monotonic, Velocity2d};
use thiserror::Error;

use crate::state::RobotState;

/// Cause d'un changement d'etat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionReason {
    /// La sequence de demarrage s'est achevee.
    BootCompleted,
    /// Un operateur ou un composant amont a demande la transition.
    Requested,
    /// La couche de securite l'a imposee.
    Safety,
    /// L'arret d'urgence a ete engage.
    EmergencyStop,
    /// Sortie explicite d'un arret de securite.
    Recovered,
}

impl TransitionReason {
    /// Libelle stable pour les logs structures.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BootCompleted => "boot_completed",
            Self::Requested => "requested",
            Self::Safety => "safety",
            Self::EmergencyStop => "emergency_stop",
            Self::Recovered => "recovered",
        }
    }
}

/// Motif de refus d'une commande de mouvement.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum CommandRejection {
    /// La consigne contient un `NaN` ou un infini.
    #[error("consigne de vitesse non finie")]
    NonFiniteVelocity,
    /// La commande est datee dans le futur : horloge desynchronisee ou emetteur fautif.
    #[error("commande datee dans le futur")]
    FutureTimestamp,
    /// La commande a trop vieilli entre son emission et sa reception.
    #[error("commande perimee : {} ms, limite {} ms", age.as_millis(), limit.as_millis())]
    Stale {
        /// Age constate.
        age: Duration,
        /// Age maximal tolere.
        limit: Duration,
    },
    /// La source n'est pas habilitee dans l'etat courant.
    ///
    /// Le champ est nomme `command_source` et non `source` : `thiserror` reserve ce
    /// dernier nom a la cause chainee d'une erreur.
    #[error("la source `{command_source}` n'est pas habilitee dans l'etat `{state}`")]
    SourceNotAuthorized {
        /// Emetteur refuse.
        command_source: CommandSource,
        /// Etat au moment du refus.
        state: RobotState,
    },
}

impl CommandRejection {
    /// Libelle stable pour les logs structures.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NonFiniteVelocity => "non_finite_velocity",
            Self::FutureTimestamp => "future_timestamp",
            Self::Stale { .. } => "stale",
            Self::SourceNotAuthorized { .. } => "source_not_authorized",
        }
    }
}

/// Motif de refus d'un changement d'etat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransitionError {
    /// La machine a etats n'autorise pas cette transition.
    #[error("transition interdite de `{from}` vers `{to}`")]
    Forbidden {
        /// Etat de depart.
        from: RobotState,
        /// Etat vise.
        to: RobotState,
    },
    /// L'arret d'urgence est engage : seul `SafeStop` reste atteignable.
    #[error("arret d'urgence engage : impossible de quitter `{from}`")]
    EmergencyStopEngaged {
        /// Etat de depart.
        from: RobotState,
    },
}

/// Fait notable emis par le coeur du robot.
///
/// Les evenements sont la seule interface d'observation du coeur. `robot-telemetry` les
/// consomme pour produire logs et compteurs ; l'adaptateur ROS 2 les republiera en
/// Milestone 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RobotEvent {
    /// L'etat operationnel a change.
    StateChanged {
        /// Etat quitte.
        from: RobotState,
        /// Etat atteint.
        to: RobotState,
        /// Cause du changement.
        reason: TransitionReason,
        /// Instant du changement.
        at: Monotonic,
    },
    /// Une commande de mouvement a ete acceptee.
    CommandAccepted {
        /// Emetteur.
        source: CommandSource,
        /// Consigne demandee, avant contrainte de securite.
        velocity: Velocity2d,
        /// Instant de reception.
        at: Monotonic,
    },
    /// Une commande de mouvement a ete refusee.
    CommandRejected {
        /// Emetteur.
        source: CommandSource,
        /// Motif du refus.
        reason: CommandRejection,
        /// Instant du refus.
        at: Monotonic,
    },
    /// La couche de securite a releve un constat.
    SafetyViolationRaised {
        /// Constat releve.
        violation: SafetyViolation,
        /// Instant du constat.
        at: Monotonic,
    },
    /// Le robot est passe en arret de securite.
    SafeStopEngaged {
        /// Violation a l'origine de l'arret.
        reason: SafetyViolation,
        /// Instant de l'arret.
        at: Monotonic,
    },
    /// L'arret de securite a ete leve explicitement.
    SafeStopCleared {
        /// Instant de la levee.
        at: Monotonic,
    },
    /// L'arret d'urgence logiciel a ete engage.
    EmergencyStopEngaged {
        /// Instant de l'engagement.
        at: Monotonic,
    },
    /// L'arret d'urgence logiciel a ete leve.
    EmergencyStopCleared {
        /// Instant de la levee.
        at: Monotonic,
    },
}

impl RobotEvent {
    /// Libelle stable, utilise pour le nommage des logs et des compteurs.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::StateChanged { .. } => "state_changed",
            Self::CommandAccepted { .. } => "command_accepted",
            Self::CommandRejected { .. } => "command_rejected",
            Self::SafetyViolationRaised { .. } => "safety_violation",
            Self::SafeStopEngaged { .. } => "safe_stop_engaged",
            Self::SafeStopCleared { .. } => "safe_stop_cleared",
            Self::EmergencyStopEngaged { .. } => "emergency_stop_engaged",
            Self::EmergencyStopCleared { .. } => "emergency_stop_cleared",
        }
    }

    /// Instant auquel l'evenement s'est produit.
    #[must_use]
    pub const fn at(&self) -> Monotonic {
        match self {
            Self::StateChanged { at, .. }
            | Self::CommandAccepted { at, .. }
            | Self::CommandRejected { at, .. }
            | Self::SafetyViolationRaised { at, .. }
            | Self::SafeStopEngaged { at, .. }
            | Self::SafeStopCleared { at }
            | Self::EmergencyStopEngaged { at }
            | Self::EmergencyStopCleared { at } => *at,
        }
    }
}
