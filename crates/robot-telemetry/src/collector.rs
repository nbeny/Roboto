use robot_core::{Robot, RobotEvent};
use robot_types::{CommandSource, Monotonic};

use crate::counters::EventCounters;
use crate::snapshot::TelemetrySnapshot;

/// Consomme les evenements du coeur pour en tirer compteurs et logs structures.
#[derive(Debug, Clone, Default)]
pub struct TelemetryCollector {
    counters: EventCounters,
    /// Dernier refus journalise, pour ne pas repeter la meme ligne indefiniment.
    last_rejection: Option<(CommandSource, &'static str)>,
}

impl TelemetryCollector {
    /// Cree un collecteur aux compteurs remis a zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compteurs cumules.
    #[must_use]
    pub const fn counters(&self) -> EventCounters {
        self.counters
    }

    /// Comptabilise un evenement et emet le log structure correspondant.
    pub fn record(&mut self, event: &RobotEvent) {
        self.count(event);

        // Un refus qui se repete a l'identique n'apprend rien de plus au bout de la
        // deuxieme fois — et une pile de navigation qui insiste pendant un `SAFE_STOP`
        // en produit des dizaines par seconde.
        if let RobotEvent::CommandRejected { source, reason, .. } = event {
            let signature = (*source, reason.kind());

            if self.last_rejection == Some(signature) {
                self.counters.suppressed_logs = self.counters.suppressed_logs.saturating_add(1);
                tracing::debug!(
                    event = "command_rejected",
                    source = source.as_str(),
                    rejection = reason.kind(),
                    "{reason} (repetition)"
                );
                return;
            }

            self.last_rejection = Some(signature);
        }

        // Apres un changement d'etat, le meme refus decrit une situation nouvelle : il
        // redevient digne d'etre signale.
        if matches!(event, RobotEvent::StateChanged { .. }) {
            self.last_rejection = None;
        }

        log(event);
    }

    fn count(&mut self, event: &RobotEvent) {
        let counter = match event {
            RobotEvent::StateChanged { .. } => &mut self.counters.state_transitions,
            RobotEvent::CommandAccepted { .. } => &mut self.counters.commands_accepted,
            RobotEvent::CommandRejected { .. } => &mut self.counters.commands_rejected,
            RobotEvent::SafetyViolationRaised { .. } => &mut self.counters.safety_violations,
            RobotEvent::SafeStopEngaged { .. } => &mut self.counters.safe_stops,
            RobotEvent::EmergencyStopEngaged { .. } => &mut self.counters.emergency_stops,
            // Les levees ne sont pas des compteurs : elles se lisent dans l'instantane.
            RobotEvent::SafeStopCleared { .. } | RobotEvent::EmergencyStopCleared { .. } => return,
        };

        *counter = counter.saturating_add(1);
    }

    /// Vide le tampon d'evenements du robot et comptabilise le tout.
    ///
    /// Renvoie le nombre d'evenements traites.
    pub fn collect_from(&mut self, robot: &mut Robot) -> usize {
        let events: Vec<RobotEvent> = robot.drain_events().collect();
        for event in &events {
            self.record(event);
        }
        events.len()
    }

    /// Produit l'instantane observable du robot.
    #[must_use]
    pub fn snapshot(&self, robot: &Robot, now: Monotonic) -> TelemetrySnapshot {
        TelemetrySnapshot {
            state: robot.state(),
            velocity: robot.velocity_output(),
            emergency_stop: robot.is_emergency_stopped(),
            uptime: robot.uptime(now),
            command_activity_age: robot.command_activity_age(now),
            dropped_events: robot.dropped_events(),
            counters: self.counters,
            at: now,
        }
    }
}

/// Emet le log structure d'un evenement.
///
/// Les champs sont nommes de facon stable : ce sont eux qui seront requetes plus tard
/// dans un agregateur de logs, et repris comme etiquettes de metriques.
fn log(event: &RobotEvent) {
    let kind = event.kind();
    let at = event.at();

    match event {
        RobotEvent::StateChanged {
            from, to, reason, ..
        } => tracing::info!(
            event = kind,
            %at,
            from = from.as_str(),
            to = to.as_str(),
            reason = reason.as_str(),
            "changement d'etat"
        ),
        RobotEvent::CommandAccepted {
            source, velocity, ..
        } => tracing::debug!(
            event = kind,
            %at,
            source = source.as_str(),
            linear = velocity.linear,
            angular = velocity.angular,
            "commande acceptee"
        ),
        RobotEvent::CommandRejected { source, reason, .. } => tracing::warn!(
            event = kind,
            %at,
            source = source.as_str(),
            rejection = reason.kind(),
            "commande refusee : {reason}"
        ),
        RobotEvent::SafetyViolationRaised { violation, .. } => tracing::warn!(
            event = kind,
            %at,
            violation = violation.kind(),
            "constat de securite : {violation}"
        ),
        RobotEvent::SafeStopEngaged { reason, .. } => tracing::error!(
            event = kind,
            %at,
            cause = reason.kind(),
            "arret de securite : {reason}"
        ),
        RobotEvent::SafeStopCleared { .. } => {
            tracing::info!(event = kind, %at, "arret de securite leve");
        }
        RobotEvent::EmergencyStopEngaged { .. } => {
            tracing::error!(event = kind, %at, "arret d'urgence engage");
        }
        RobotEvent::EmergencyStopCleared { .. } => {
            tracing::info!(event = kind, %at, "arret d'urgence leve");
        }
    }
}
