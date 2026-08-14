use core::time::Duration;
use std::collections::VecDeque;
use std::collections::vec_deque::Drain;

use robot_safety::{
    MotionAuthority, SafetyLayer, SafetyLimits, SafetyLimitsError, SafetyViolation,
};
use robot_types::{Monotonic, MotionCommand, Velocity2d};

use crate::event::{CommandRejection, RobotEvent, TransitionError, TransitionReason};
use crate::state::RobotState;

/// Reglage du coeur du robot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RobotConfig {
    /// Enveloppe operationnelle appliquee par la couche de securite.
    pub limits: SafetyLimits,
    /// Duree de la sequence de demarrage avant le passage automatique en `IDLE`.
    pub boot_duration: Duration,
    /// Taille du tampon d'evenements en attente de consommation.
    ///
    /// Le tampon est borne : un consommateur absent ou lent ne doit pas faire enfler la
    /// memoire d'un robot qui tourne des heures. Les evenements les plus anciens sont
    /// abandonnes et comptabilises par [`Robot::dropped_events`].
    pub event_buffer_capacity: usize,
}

impl Default for RobotConfig {
    fn default() -> Self {
        Self {
            limits: SafetyLimits::default(),
            boot_duration: Duration::from_millis(500),
            event_buffer_capacity: 256,
        }
    }
}

/// Resultat d'un cycle de controle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlOutput {
    /// Etat du robot a l'issue du cycle.
    pub state: RobotState,
    /// Consigne de vitesse autorisee, destinee a l'abstraction materielle.
    pub velocity: Velocity2d,
    /// Instant du cycle.
    pub at: Monotonic,
}

/// Coeur du robot : machine a etats, autorite des commandes, boucle de controle.
#[derive(Debug)]
pub struct Robot {
    config: RobotConfig,
    state: RobotState,
    safety: SafetyLayer,
    pending_command: Option<MotionCommand>,
    boot_started_at: Option<Monotonic>,
    events: VecDeque<RobotEvent>,
    dropped_events: u64,
}

impl Robot {
    /// Construit un robot en etat `BOOTING`.
    ///
    /// # Errors
    ///
    /// Renvoie [`SafetyLimitsError`] si les limites de securite sont inexploitables.
    pub fn new(config: RobotConfig) -> Result<Self, SafetyLimitsError> {
        Ok(Self {
            safety: SafetyLayer::new(config.limits)?,
            config,
            state: RobotState::Booting,
            pending_command: None,
            boot_started_at: None,
            events: VecDeque::new(),
            dropped_events: 0,
        })
    }

    /// Etat operationnel courant.
    #[must_use]
    pub const fn state(&self) -> RobotState {
        self.state
    }

    /// Derniere consigne de vitesse autorisee en sortie.
    #[must_use]
    pub fn velocity_output(&self) -> Velocity2d {
        self.safety.last_output()
    }

    /// Indique si l'arret d'urgence logiciel est engage.
    #[must_use]
    pub fn is_emergency_stopped(&self) -> bool {
        self.safety.is_emergency_stopped()
    }

    /// Nombre d'evenements abandonnes faute de place dans le tampon.
    #[must_use]
    pub const fn dropped_events(&self) -> u64 {
        self.dropped_events
    }

    /// Temps ecoule depuis le premier cycle de controle.
    #[must_use]
    pub fn uptime(&self, now: Monotonic) -> Duration {
        self.boot_started_at.map_or(Duration::ZERO, |started| {
            now.saturating_duration_since(started)
        })
    }

    /// Age du dernier signe de vie du flux de commandes.
    #[must_use]
    pub fn command_activity_age(&self, now: Monotonic) -> Option<Duration> {
        self.safety.command_activity_age(now)
    }

    /// Vide le tampon d'evenements.
    pub fn drain_events(&mut self) -> Drain<'_, RobotEvent> {
        self.events.drain(..)
    }

    /// Soumet une commande de mouvement.
    ///
    /// La commande est d'abord validee sur son contenu (consigne finie, horodatage
    /// coherent), puis sur l'autorite de sa source dans l'etat courant. Une commande
    /// acceptee nourrit le watchdog ; une commande refusee ne le nourrit pas.
    ///
    /// # Errors
    ///
    /// Renvoie [`CommandRejection`] si la commande est invalide ou non habilitee.
    pub fn submit_command(
        &mut self,
        command: MotionCommand,
        now: Monotonic,
    ) -> Result<(), CommandRejection> {
        if let Err(reason) = self.validate_command(command, now) {
            self.emit(RobotEvent::CommandRejected {
                source: command.source,
                reason,
                at: now,
            });
            return Err(reason);
        }

        self.pending_command = Some(command);
        self.safety.notify_command_activity(now);
        self.emit(RobotEvent::CommandAccepted {
            source: command.source,
            velocity: command.velocity,
            at: now,
        });

        Ok(())
    }

    /// Valide le contenu de la commande avant son autorite.
    ///
    /// L'ordre compte : une consigne aberrante ou perimee doit etre signalee comme telle
    /// meme lorsqu'elle emane d'une source par ailleurs habilitee.
    fn validate_command(
        &self,
        command: MotionCommand,
        now: Monotonic,
    ) -> Result<(), CommandRejection> {
        if !command.velocity.is_finite() {
            return Err(CommandRejection::NonFiniteVelocity);
        }

        if command.is_from_future(now) {
            return Err(CommandRejection::FutureTimestamp);
        }

        let age = command.age_at(now);
        let limit = self.safety.limits().command_timeout;
        if age >= limit {
            return Err(CommandRejection::Stale { age, limit });
        }

        if !self.state.accepts_command_from(command.source) {
            return Err(CommandRejection::SourceNotAuthorized {
                command_source: command.source,
                state: self.state,
            });
        }

        Ok(())
    }

    /// Demande une transition d'etat explicite.
    ///
    /// # Errors
    ///
    /// Renvoie [`TransitionError`] si la transition est interdite, ou si l'arret
    /// d'urgence est engage.
    pub fn request_state(
        &mut self,
        target: RobotState,
        now: Monotonic,
    ) -> Result<(), TransitionError> {
        if self.safety.is_emergency_stopped() && target != RobotState::SafeStop {
            return Err(TransitionError::EmergencyStopEngaged { from: self.state });
        }

        self.transition(target, TransitionReason::Requested, now)
    }

    /// Execute un cycle de controle et renvoie la consigne autorisee.
    pub fn tick(&mut self, now: Monotonic) -> ControlOutput {
        if self.boot_started_at.is_none() {
            self.boot_started_at = Some(now);
        }

        self.advance_boot(now);

        let authority = match self.pending_command {
            Some(command) if self.state.allows_motion() => {
                MotionAuthority::Allowed(command.velocity)
            }
            _ => MotionAuthority::Denied,
        };

        let decision = self.safety.evaluate(authority, now);

        // Une condition qui impose l'arret et qui persiste a deja ete signalee au cycle
        // ou elle est apparue, et le robot est deja arrete. La re-emettre a chaque cycle
        // produirait des dizaines d'evenements par seconde sans rien apprendre.
        let already_stopped = self.state == RobotState::SafeStop;

        for violation in &decision.violations {
            if already_stopped && violation.requires_safe_stop() {
                continue;
            }

            self.emit(RobotEvent::SafetyViolationRaised {
                violation: *violation,
                at: now,
            });
        }

        if decision.safe_stop_required && self.state != RobotState::SafeStop {
            let cause = decision
                .violations
                .iter()
                .copied()
                .find(|violation| violation.requires_safe_stop());

            // `SafeStop` est atteignable depuis tout autre etat : la transition ne peut
            // pas echouer ici, mais on ne s'autorise pas a l'affirmer par un `unwrap`.
            let transitioned = self
                .transition(RobotState::SafeStop, TransitionReason::Safety, now)
                .is_ok();

            if let Some(reason) = cause {
                if transitioned {
                    self.emit(RobotEvent::SafeStopEngaged { reason, at: now });
                }
            }
        }

        ControlOutput {
            state: self.state,
            velocity: decision.velocity,
            at: now,
        }
    }

    fn advance_boot(&mut self, now: Monotonic) {
        if self.state != RobotState::Booting {
            return;
        }

        let Some(started_at) = self.boot_started_at else {
            return;
        };

        if now.saturating_duration_since(started_at) >= self.config.boot_duration {
            let _ = self.transition(RobotState::Idle, TransitionReason::BootCompleted, now);
        }
    }

    /// Engage l'arret d'urgence logiciel et bascule immediatement en `SAFE_STOP`.
    ///
    /// Complement du bouton champignon materiel, qui coupe physiquement la puissance :
    /// ce verrou logiciel garantit que l'etat interne reflete la realite.
    pub fn engage_emergency_stop(&mut self, now: Monotonic) {
        if self.safety.is_emergency_stopped() {
            return;
        }

        self.safety.engage_emergency_stop();
        self.emit(RobotEvent::EmergencyStopEngaged { at: now });

        if self.state != RobotState::SafeStop
            && self
                .transition(RobotState::SafeStop, TransitionReason::EmergencyStop, now)
                .is_ok()
        {
            self.emit(RobotEvent::SafeStopEngaged {
                reason: SafetyViolation::EmergencyStopEngaged,
                at: now,
            });
        }
    }

    /// Leve l'arret d'urgence. Le robot reste en `SAFE_STOP` jusqu'a
    /// [`Robot::clear_safe_stop`].
    pub fn clear_emergency_stop(&mut self, now: Monotonic) {
        if !self.safety.is_emergency_stopped() {
            return;
        }

        self.safety.clear_emergency_stop();
        self.emit(RobotEvent::EmergencyStopCleared { at: now });
    }

    /// Quitte `SAFE_STOP` pour revenir en `IDLE`.
    ///
    /// # Errors
    ///
    /// Renvoie [`TransitionError`] si le robot n'est pas en `SAFE_STOP`, ou si l'arret
    /// d'urgence est toujours engage.
    pub fn clear_safe_stop(&mut self, now: Monotonic) -> Result<(), TransitionError> {
        if self.state != RobotState::SafeStop {
            return Err(TransitionError::Forbidden {
                from: self.state,
                to: RobotState::Idle,
            });
        }

        if self.safety.is_emergency_stopped() {
            return Err(TransitionError::EmergencyStopEngaged { from: self.state });
        }

        self.transition(RobotState::Idle, TransitionReason::Recovered, now)?;
        self.emit(RobotEvent::SafeStopCleared { at: now });

        Ok(())
    }

    fn emit(&mut self, event: RobotEvent) {
        if self.events.len() >= self.config.event_buffer_capacity {
            self.events.pop_front();
            self.dropped_events += 1;
        }
        self.events.push_back(event);
    }

    fn transition(
        &mut self,
        target: RobotState,
        reason: TransitionReason,
        now: Monotonic,
    ) -> Result<(), TransitionError> {
        if !self.state.can_transition_to(target) {
            return Err(TransitionError::Forbidden {
                from: self.state,
                to: target,
            });
        }

        let from = self.state;
        self.state = target;

        if target.allows_motion() {
            // Entrer dans un etat de mouvement etablit la ligne de base du watchdog :
            // a partir d'ici, une commande doit arriver dans le delai imparti.
            self.safety.notify_command_activity(now);
        } else {
            // Quitter un etat de mouvement invalide la commande en attente, faute de
            // quoi une reprise ferait repartir le robot sur une consigne perimee.
            self.safety.disarm_command_watchdog();
            self.pending_command = None;
        }

        self.emit(RobotEvent::StateChanged {
            from,
            to: target,
            reason,
            at: now,
        });

        Ok(())
    }
}
