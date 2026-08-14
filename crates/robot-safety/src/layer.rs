use core::time::Duration;

use robot_types::{Monotonic, Velocity2d};

use crate::limits::{SafetyLimits, SafetyLimitsError};
use crate::violation::SafetyViolation;
use crate::watchdog::Watchdog;

/// Ce que l'etat courant du robot autorise en matiere de mouvement.
///
/// C'est `robot-core` qui traduit son etat en autorite. La couche de securite n'a pas a
/// connaitre la machine a etats : elle sait seulement si le mouvement est permis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotionAuthority {
    /// L'etat autorise le mouvement, avec la consigne demandee.
    Allowed(Velocity2d),
    /// L'etat interdit le mouvement : la sortie sera nulle.
    Denied,
}

/// Verdict d'une evaluation de securite.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct SafetyDecision {
    /// Vitesse effectivement autorisee en sortie.
    pub velocity: Velocity2d,
    /// Constats releves pendant l'evaluation, dans l'ordre de detection.
    pub violations: Vec<SafetyViolation>,
    /// Indique si le coeur doit basculer en `SAFE_STOP`.
    pub safe_stop_required: bool,
}

/// Couche de securite : dernier maillon avant la sortie moteur.
///
/// Voir la documentation de la crate pour l'ordre d'evaluation, qui est normatif.
#[derive(Debug, Clone)]
pub struct SafetyLayer {
    limits: SafetyLimits,
    command_watchdog: Watchdog,
    emergency_stop: bool,
    last_output: Velocity2d,
    last_evaluated_at: Option<Monotonic>,
}

impl SafetyLayer {
    /// Construit une couche de securite a partir de limites validees.
    ///
    /// # Errors
    ///
    /// Renvoie [`SafetyLimitsError`] si les limites sont inexploitables.
    pub fn new(limits: SafetyLimits) -> Result<Self, SafetyLimitsError> {
        limits.validate()?;
        Ok(Self {
            limits,
            command_watchdog: Watchdog::new(limits.command_timeout),
            emergency_stop: false,
            last_output: Velocity2d::ZERO,
            last_evaluated_at: None,
        })
    }

    /// Limites en vigueur.
    #[must_use]
    pub const fn limits(&self) -> &SafetyLimits {
        &self.limits
    }

    /// Derniere vitesse effectivement autorisee.
    #[must_use]
    pub const fn last_output(&self) -> Velocity2d {
        self.last_output
    }

    /// Indique si l'arret d'urgence logiciel est engage.
    #[must_use]
    pub const fn is_emergency_stopped(&self) -> bool {
        self.emergency_stop
    }

    /// Engage l'arret d'urgence. Verrouille jusqu'a [`Self::clear_emergency_stop`].
    pub const fn engage_emergency_stop(&mut self) {
        self.emergency_stop = true;
    }

    /// Leve l'arret d'urgence. Toujours explicite, jamais automatique.
    pub const fn clear_emergency_stop(&mut self) {
        self.emergency_stop = false;
    }

    /// Enregistre un signe de vie du flux de commandes.
    ///
    /// Appele a chaque commande acceptee, et a l'entree d'un etat de mouvement pour
    /// etablir la ligne de base : a partir de la, une commande doit arriver dans
    /// `command_timeout` sous peine d'arret de securite.
    pub const fn notify_command_activity(&mut self, now: Monotonic) {
        self.command_watchdog.feed(now);
    }

    /// Desarme le watchdog de commande, a la sortie d'un etat de mouvement.
    pub const fn disarm_command_watchdog(&mut self) {
        self.command_watchdog.disarm();
    }

    /// Age du dernier signe de vie du flux de commandes.
    #[must_use]
    pub fn command_activity_age(&self, now: Monotonic) -> Option<Duration> {
        self.command_watchdog.elapsed_since_feed(now)
    }

    /// Evalue une demande de mouvement et renvoie la vitesse autorisee.
    ///
    /// L'ordre des controles est normatif : voir la documentation de la crate.
    pub fn evaluate(&mut self, authority: MotionAuthority, now: Monotonic) -> SafetyDecision {
        let mut violations = Vec::new();

        // 1. L'arret d'urgence prime sur tout, y compris sur l'autorite : un arret
        //    declenche pendant que le robot est au repos doit tout de meme forcer le
        //    passage en SAFE_STOP.
        if self.emergency_stop {
            violations.push(SafetyViolation::EmergencyStopEngaged);
            return self.halt(violations, true, now);
        }

        // 2. L'etat courant n'autorise pas le mouvement : arret nominal, sans violation.
        let MotionAuthority::Allowed(requested) = authority else {
            return self.halt(violations, false, now);
        };

        // 3. Une consigne non finie est un defaut logiciel, pas une commande.
        if !requested.is_finite() {
            violations.push(SafetyViolation::NonFiniteCommand);
        }

        // 4. Le flux de commandes s'est tari. Reste a savoir si c'est dangereux.
        if self.command_watchdog.is_expired(now) {
            // `is_expired` implique un watchdog arme, donc une valeur presente.
            let elapsed = self
                .command_watchdog
                .elapsed_since_feed(now)
                .unwrap_or_default();
            let timeout = self.command_watchdog.timeout();

            // Le robot est-il reellement immobile ? Il faut les deux conditions : qu'il
            // ne roule pas, et que la derniere consigne recue ne lui demandait pas de
            // partir. Une consigne de mouvement perimee est aussi dangereuse qu'un robot
            // deja lance.
            let at_rest = self.last_output.is_stationary() && requested.is_stationary();

            violations.push(if at_rest {
                SafetyViolation::CommandStreamIdle { elapsed, timeout }
            } else {
                SafetyViolation::CommandTimeout { elapsed, timeout }
            });
        }

        // 5. Toute perte de controle court-circuite les limiteurs.
        if violations
            .iter()
            .any(|violation| violation.requires_safe_stop())
        {
            return self.halt(violations, true, now);
        }

        // 6. puis 7. : la saturation de vitesse precede celle d'acceleration, de sorte
        //    que la rampe vise une cible deja bornee.
        let bounded = self.clamp_speed(requested, &mut violations);
        let applied = self.clamp_acceleration(bounded, now, &mut violations);

        self.last_output = applied;
        self.last_evaluated_at = Some(now);

        SafetyDecision {
            velocity: applied,
            violations,
            safe_stop_required: false,
        }
    }

    /// Arret immediat : vitesse nulle sans rampe, et remise a zero de la reference
    /// d'acceleration pour que la reprise reparte du robot reellement a l'arret.
    fn halt(
        &mut self,
        violations: Vec<SafetyViolation>,
        safe_stop_required: bool,
        now: Monotonic,
    ) -> SafetyDecision {
        self.last_output = Velocity2d::ZERO;
        self.last_evaluated_at = Some(now);

        SafetyDecision {
            velocity: Velocity2d::ZERO,
            violations,
            safe_stop_required,
        }
    }

    fn clamp_speed(
        &self,
        requested: Velocity2d,
        violations: &mut Vec<SafetyViolation>,
    ) -> Velocity2d {
        let mut applied = requested;

        let max_linear = self.limits.max_linear_speed;
        if applied.linear.abs() > max_linear {
            let bounded = applied.linear.clamp(-max_linear, max_linear);
            violations.push(SafetyViolation::LinearSpeedClamped {
                requested: applied.linear,
                applied: bounded,
            });
            applied.linear = bounded;
        }

        let max_angular = self.limits.max_angular_speed;
        if applied.angular.abs() > max_angular {
            let bounded = applied.angular.clamp(-max_angular, max_angular);
            violations.push(SafetyViolation::AngularSpeedClamped {
                requested: applied.angular,
                applied: bounded,
            });
            applied.angular = bounded;
        }

        applied
    }

    fn clamp_acceleration(
        &self,
        requested: Velocity2d,
        now: Monotonic,
        violations: &mut Vec<SafetyViolation>,
    ) -> Velocity2d {
        // Sans evaluation precedente, il n'existe aucun `dt` sur lequel raisonner.
        let Some(previous) = self.last_evaluated_at else {
            return requested;
        };

        let dt = now.saturating_duration_since(previous).as_secs_f64();
        if dt <= 0.0 {
            return requested;
        }

        let mut applied = requested;

        let max_linear_delta = self.limits.max_linear_acceleration * dt;
        let lower = self.last_output.linear - max_linear_delta;
        let upper = self.last_output.linear + max_linear_delta;
        if applied.linear < lower || applied.linear > upper {
            let bounded = applied.linear.clamp(lower, upper);
            violations.push(SafetyViolation::LinearAccelerationClamped {
                requested: applied.linear,
                applied: bounded,
            });
            applied.linear = bounded;
        }

        let max_angular_delta = self.limits.max_angular_acceleration * dt;
        let lower = self.last_output.angular - max_angular_delta;
        let upper = self.last_output.angular + max_angular_delta;
        if applied.angular < lower || applied.angular > upper {
            let bounded = applied.angular.clamp(lower, upper);
            violations.push(SafetyViolation::AngularAccelerationClamped {
                requested: applied.angular,
                applied: bounded,
            });
            applied.angular = bounded;
        }

        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_limits_are_rejected_at_construction() {
        let limits = SafetyLimits {
            max_linear_speed: 0.0,
            ..SafetyLimits::default()
        };

        assert!(SafetyLayer::new(limits).is_err());
    }

    #[test]
    fn a_fresh_layer_is_not_emergency_stopped() {
        let layer = SafetyLayer::new(SafetyLimits::default()).expect("limites par defaut valides");

        assert!(!layer.is_emergency_stopped());
        assert_eq!(layer.last_output(), Velocity2d::ZERO);
    }
}
