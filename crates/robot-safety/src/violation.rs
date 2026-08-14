use core::fmt;
use core::time::Duration;

/// Constat produit par la couche de securite lors d'une evaluation.
///
/// Toutes les violations ne se valent pas : une saturation de vitesse est un
/// fonctionnement nominal de la couche (la consigne est bornee, le robot continue),
/// alors qu'un watchdog expire impose un arret. [`SafetyViolation::requires_safe_stop`]
/// tranche.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SafetyViolation {
    /// La consigne contient un `NaN` ou un infini. Defaut logiciel.
    NonFiniteCommand,
    /// L'arret d'urgence est engage.
    EmergencyStopEngaged,
    /// Aucune commande recue dans le delai imparti.
    CommandTimeout {
        /// Temps ecoule depuis la derniere commande.
        elapsed: Duration,
        /// Delai configure.
        timeout: Duration,
    },
    /// Vitesse lineaire bornee a la limite.
    LinearSpeedClamped {
        /// Valeur demandee.
        requested: f64,
        /// Valeur appliquee.
        applied: f64,
    },
    /// Vitesse angulaire bornee a la limite.
    AngularSpeedClamped {
        /// Valeur demandee.
        requested: f64,
        /// Valeur appliquee.
        applied: f64,
    },
    /// Variation de vitesse lineaire bornee par l'acceleration maximale.
    LinearAccelerationClamped {
        /// Valeur demandee.
        requested: f64,
        /// Valeur appliquee.
        applied: f64,
    },
    /// Variation de vitesse angulaire bornee par l'acceleration maximale.
    AngularAccelerationClamped {
        /// Valeur demandee.
        requested: f64,
        /// Valeur appliquee.
        applied: f64,
    },
}

impl SafetyViolation {
    /// Indique si cette violation impose le passage en `SAFE_STOP`.
    ///
    /// Les saturations sont le travail normal de la couche et n'interrompent pas la
    /// mission. Les trois autres cas signalent une perte de controle.
    #[must_use]
    pub const fn requires_safe_stop(self) -> bool {
        matches!(
            self,
            Self::NonFiniteCommand | Self::EmergencyStopEngaged | Self::CommandTimeout { .. }
        )
    }

    /// Libelle stable, destine aux logs structures et aux compteurs de telemetrie.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NonFiniteCommand => "non_finite_command",
            Self::EmergencyStopEngaged => "emergency_stop_engaged",
            Self::CommandTimeout { .. } => "command_timeout",
            Self::LinearSpeedClamped { .. } => "linear_speed_clamped",
            Self::AngularSpeedClamped { .. } => "angular_speed_clamped",
            Self::LinearAccelerationClamped { .. } => "linear_acceleration_clamped",
            Self::AngularAccelerationClamped { .. } => "angular_acceleration_clamped",
        }
    }
}

impl fmt::Display for SafetyViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteCommand => f.write_str("consigne de vitesse non finie"),
            Self::EmergencyStopEngaged => f.write_str("arret d'urgence engage"),
            Self::CommandTimeout { elapsed, timeout } => write!(
                f,
                "aucune commande depuis {:.3}s (limite {:.3}s)",
                elapsed.as_secs_f64(),
                timeout.as_secs_f64()
            ),
            Self::LinearSpeedClamped { requested, applied } => write!(
                f,
                "vitesse lineaire bornee de {requested:.3} a {applied:.3} m/s"
            ),
            Self::AngularSpeedClamped { requested, applied } => write!(
                f,
                "vitesse angulaire bornee de {requested:.3} a {applied:.3} rad/s"
            ),
            Self::LinearAccelerationClamped { requested, applied } => write!(
                f,
                "acceleration lineaire bornee : {requested:.3} ramene a {applied:.3} m/s"
            ),
            Self::AngularAccelerationClamped { requested, applied } => write!(
                f,
                "acceleration angulaire bornee : {requested:.3} ramene a {applied:.3} rad/s"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_control_requires_a_safe_stop() {
        assert!(SafetyViolation::NonFiniteCommand.requires_safe_stop());
        assert!(SafetyViolation::EmergencyStopEngaged.requires_safe_stop());
        assert!(
            SafetyViolation::CommandTimeout {
                elapsed: Duration::from_millis(600),
                timeout: Duration::from_millis(500),
            }
            .requires_safe_stop()
        );
    }

    #[test]
    fn clamping_is_nominal_and_does_not_require_a_safe_stop() {
        assert!(
            !SafetyViolation::LinearSpeedClamped {
                requested: 2.0,
                applied: 0.5
            }
            .requires_safe_stop()
        );
        assert!(
            !SafetyViolation::AngularSpeedClamped {
                requested: 4.0,
                applied: 1.0
            }
            .requires_safe_stop()
        );
        assert!(
            !SafetyViolation::LinearAccelerationClamped {
                requested: 0.5,
                applied: 0.05
            }
            .requires_safe_stop()
        );
        assert!(
            !SafetyViolation::AngularAccelerationClamped {
                requested: 1.0,
                applied: 0.15
            }
            .requires_safe_stop()
        );
    }

    #[test]
    fn violations_expose_a_stable_kind_label() {
        assert_eq!(
            SafetyViolation::EmergencyStopEngaged.kind(),
            "emergency_stop_engaged"
        );
        assert_eq!(
            SafetyViolation::CommandTimeout {
                elapsed: Duration::ZERO,
                timeout: Duration::ZERO
            }
            .kind(),
            "command_timeout"
        );
    }
}
