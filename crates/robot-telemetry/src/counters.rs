/// Compteurs cumules depuis le demarrage du collecteur.
///
/// Volontairement plats et monotones : ce sont des compteurs au sens Prometheus, faits
/// pour etre derives cote consommateur, pas des jauges a maintenir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventCounters {
    /// Commandes de mouvement acceptees.
    pub commands_accepted: u64,
    /// Commandes de mouvement refusees.
    pub commands_rejected: u64,
    /// Constats releves par la couche de securite, saturations comprises.
    pub safety_violations: u64,
    /// Entrees en arret de securite.
    pub safe_stops: u64,
    /// Engagements de l'arret d'urgence.
    pub emergency_stops: u64,
    /// Changements d'etat.
    pub state_transitions: u64,
}

impl EventCounters {
    /// Total des evenements comptabilises.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.commands_accepted
            + self.commands_rejected
            + self.safety_violations
            + self.safe_stops
            + self.emergency_stops
            + self.state_transitions
    }
}
