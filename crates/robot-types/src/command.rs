use core::fmt;
use core::time::Duration;

use crate::{Monotonic, Velocity2d};

/// Origine d'une commande de mouvement.
///
/// La source determine l'autorite : `robot-core` n'accepte une commande que si elle
/// provient d'une source habilitee pour l'etat courant. C'est ce mecanisme qui interdit
/// structurellement a la couche IA de piloter les moteurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CommandSource {
    /// Teleoperation par un humain (manette, clavier, dashboard).
    Teleoperation,
    /// Pile de navigation autonome (Nav2 a partir de la Milestone 6).
    Navigation,
    /// Agent IA de haut niveau.
    ///
    /// N'est habilitee dans aucun etat : l'IA exprime des objectifs, jamais des vitesses.
    Ai,
    /// Le coeur du robot lui-meme (manoeuvres internes, arret controle).
    Internal,
}

impl CommandSource {
    /// Libelle stable, utilise dans les logs structures et la telemetrie.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Teleoperation => "teleoperation",
            Self::Navigation => "navigation",
            Self::Ai => "ai",
            Self::Internal => "internal",
        }
    }
}

impl fmt::Display for CommandSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Demande de mouvement soumise au coeur du robot.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MotionCommand {
    /// Vitesse demandee, avant toute contrainte de securite.
    pub velocity: Velocity2d,
    /// Emetteur de la commande.
    pub source: CommandSource,
    /// Instant d'emission, tel que date par l'emetteur.
    pub issued_at: Monotonic,
}

impl MotionCommand {
    /// Construit une commande de mouvement.
    #[must_use]
    pub const fn new(velocity: Velocity2d, source: CommandSource, issued_at: Monotonic) -> Self {
        Self {
            velocity,
            source,
            issued_at,
        }
    }

    /// Age de la commande a l'instant `now`, sature a zero pour une commande future.
    #[must_use]
    pub fn age_at(&self, now: Monotonic) -> Duration {
        now.saturating_duration_since(self.issued_at)
    }

    /// Indique si la commande est datee dans le futur par rapport a `now`.
    ///
    /// Un horodatage futur signale une horloge desynchronisee ou un emetteur fautif :
    /// l'accepter permettrait de maintenir le watchdog nourri indefiniment.
    #[must_use]
    pub fn is_from_future(&self, now: Monotonic) -> bool {
        self.issued_at > now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_issued_at(millis: u64) -> MotionCommand {
        MotionCommand::new(
            Velocity2d::new(0.2, 0.0),
            CommandSource::Teleoperation,
            Monotonic::from_millis(millis),
        )
    }

    #[test]
    fn the_age_is_the_time_elapsed_since_issue() {
        let command = command_issued_at(1_000);

        assert_eq!(
            command.age_at(Monotonic::from_millis(1_300)),
            Duration::from_millis(300)
        );
    }

    #[test]
    fn a_command_issued_now_has_no_age() {
        let command = command_issued_at(1_000);

        assert_eq!(
            command.age_at(Monotonic::from_millis(1_000)),
            Duration::ZERO
        );
    }

    #[test]
    fn the_age_of_a_future_command_saturates_to_zero() {
        let command = command_issued_at(2_000);

        assert_eq!(
            command.age_at(Monotonic::from_millis(1_000)),
            Duration::ZERO
        );
    }

    #[test]
    fn a_command_dated_ahead_of_now_is_from_the_future() {
        let command = command_issued_at(2_000);

        assert!(command.is_from_future(Monotonic::from_millis(1_999)));
    }

    #[test]
    fn a_command_dated_now_or_earlier_is_not_from_the_future() {
        let command = command_issued_at(2_000);

        assert!(!command.is_from_future(Monotonic::from_millis(2_000)));
        assert!(!command.is_from_future(Monotonic::from_millis(2_001)));
    }

    #[test]
    fn command_sources_have_stable_labels() {
        assert_eq!(CommandSource::Ai.as_str(), "ai");
        assert_eq!(CommandSource::Teleoperation.to_string(), "teleoperation");
    }
}
