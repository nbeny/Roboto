use core::fmt;

use robot_types::CommandSource;

/// Etat operationnel du robot.
///
/// Les transitions sont explicites et verifiees : voir [`RobotState::can_transition_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RobotState {
    /// Sequence de demarrage en cours.
    Booting,
    /// Pret, immobile, aucune mission en cours.
    Idle,
    /// Pilote par un operateur humain.
    Teleoperation,
    /// Mission de navigation autonome en cours.
    Navigating,
    /// Mission suspendue, reprise possible.
    Paused,
    /// Faute non liee a la securite (capteur absent, configuration invalide).
    Error,
    /// Arret de securite. Sortie toujours explicite.
    SafeStop,
    /// En charge, mouvement interdit.
    Charging,
}

impl RobotState {
    /// Libelle stable, utilise dans les logs structures et la telemetrie.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Booting => "BOOTING",
            Self::Idle => "IDLE",
            Self::Teleoperation => "TELEOPERATION",
            Self::Navigating => "NAVIGATING",
            Self::Paused => "PAUSED",
            Self::Error => "ERROR",
            Self::SafeStop => "SAFE_STOP",
            Self::Charging => "CHARGING",
        }
    }

    /// Indique si l'etat autorise une consigne de mouvement non nulle.
    #[must_use]
    pub fn allows_motion(self) -> bool {
        matches!(self, Self::Teleoperation | Self::Navigating)
    }

    /// Indique si la transition vers `next` est autorisee.
    ///
    /// `SafeStop` et `Error` sont atteignables depuis n'importe quel etat. Une
    /// transition vers l'etat courant n'en est pas une et est refusee.
    #[must_use]
    pub fn can_transition_to(self, next: Self) -> bool {
        // Rester dans le meme etat n'est pas une transition : l'accepter ferait emettre
        // un evenement de changement a chaque cycle tant qu'une condition persiste.
        if self == next {
            return false;
        }

        match (self, next) {
            // La securite et la faute sont toujours atteignables.
            (_, Self::SafeStop | Self::Error) => true,
            (Self::Booting, Self::Idle) => true,
            (Self::Idle, Self::Teleoperation | Self::Navigating | Self::Charging) => true,
            (Self::Teleoperation | Self::Navigating, Self::Idle | Self::Paused) => true,
            (Self::Paused, Self::Teleoperation | Self::Navigating | Self::Idle) => true,
            // La sortie de charge, de faute ou d'arret repasse toujours par IDLE.
            (Self::Charging | Self::Error | Self::SafeStop, Self::Idle) => true,
            _ => false,
        }
    }

    /// Indique si une commande emise par `source` fait autorite dans cet etat.
    ///
    /// C'est la table d'habilitation du robot. Elle est exhaustive sur
    /// [`CommandSource`] : ajouter une source cassera la compilation ici, ce qui est
    /// voulu — une nouvelle source ne doit jamais heriter d'une autorisation par defaut.
    #[must_use]
    pub fn accepts_command_from(self, source: CommandSource) -> bool {
        match source {
            // L'IA orchestre par objectifs. Elle ne soumet jamais de vitesse brute,
            // dans aucun etat.
            CommandSource::Ai => false,
            CommandSource::Teleoperation => matches!(self, Self::Teleoperation),
            CommandSource::Navigation => matches!(self, Self::Navigating),
            CommandSource::Internal => self.allows_motion(),
        }
    }
}

impl fmt::Display for RobotState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
