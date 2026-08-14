use core::time::Duration;

use thiserror::Error;

/// Enveloppe operationnelle du robot.
///
/// Les valeurs par defaut sont deliberement conservatrices : un robot d'interieur qui
/// depasse 0,5 m/s dans un couloir est deja dangereux. Elles sont faites pour etre
/// relevees consciemment, apres validation en simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafetyLimits {
    /// Vitesse lineaire maximale en m/s, en valeur absolue.
    pub max_linear_speed: f64,
    /// Vitesse angulaire maximale en rad/s, en valeur absolue.
    pub max_angular_speed: f64,
    /// Acceleration lineaire maximale en m/s^2, appliquee symetriquement.
    pub max_linear_acceleration: f64,
    /// Acceleration angulaire maximale en rad/s^2, appliquee symetriquement.
    pub max_angular_acceleration: f64,
    /// Delai au-dela duquel l'absence de commande declenche un arret de securite.
    pub command_timeout: Duration,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        Self {
            max_linear_speed: 0.5,
            max_angular_speed: 1.0,
            max_linear_acceleration: 0.5,
            max_angular_acceleration: 1.5,
            command_timeout: Duration::from_millis(500),
        }
    }
}

impl SafetyLimits {
    /// Verifie que toutes les limites sont exploitables.
    ///
    /// # Errors
    ///
    /// Renvoie [`SafetyLimitsError`] si une limite n'est pas finie et strictement
    /// positive, ou si le timeout de commande est nul.
    pub fn validate(&self) -> Result<(), SafetyLimitsError> {
        let numeric_limits = [
            ("max_linear_speed", self.max_linear_speed),
            ("max_angular_speed", self.max_angular_speed),
            ("max_linear_acceleration", self.max_linear_acceleration),
            ("max_angular_acceleration", self.max_angular_acceleration),
        ];

        for (field, value) in numeric_limits {
            // `is_finite` d'abord : toute comparaison impliquant un NaN est fausse,
            // y compris `NaN <= 0.0`, et laisserait passer la valeur.
            if !value.is_finite() || value <= 0.0 {
                return Err(SafetyLimitsError::InvalidLimit { field, value });
            }
        }

        if self.command_timeout.is_zero() {
            return Err(SafetyLimitsError::ZeroCommandTimeout);
        }

        Ok(())
    }
}

/// Configuration de securite inexploitable.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum SafetyLimitsError {
    /// Une limite numerique n'est pas finie et strictement positive.
    #[error("la limite `{field}` doit etre finie et strictement positive (valeur : {value})")]
    InvalidLimit {
        /// Nom du champ fautif.
        field: &'static str,
        /// Valeur refusee.
        value: f64,
    },
    /// Le timeout de commande est nul, ce qui rendrait tout mouvement impossible.
    #[error("`command_timeout` doit etre strictement positif")]
    ZeroCommandTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_limits_are_valid() {
        assert_eq!(SafetyLimits::default().validate(), Ok(()));
    }

    #[test]
    fn a_zero_maximum_speed_is_rejected() {
        let limits = SafetyLimits {
            max_linear_speed: 0.0,
            ..SafetyLimits::default()
        };

        assert_eq!(
            limits.validate(),
            Err(SafetyLimitsError::InvalidLimit {
                field: "max_linear_speed",
                value: 0.0
            })
        );
    }

    #[test]
    fn a_negative_maximum_acceleration_is_rejected() {
        let limits = SafetyLimits {
            max_angular_acceleration: -1.0,
            ..SafetyLimits::default()
        };

        assert_eq!(
            limits.validate(),
            Err(SafetyLimitsError::InvalidLimit {
                field: "max_angular_acceleration",
                value: -1.0
            })
        );
    }

    #[test]
    fn a_non_finite_limit_is_rejected() {
        let limits = SafetyLimits {
            max_angular_speed: f64::NAN,
            ..SafetyLimits::default()
        };

        assert!(matches!(
            limits.validate(),
            Err(SafetyLimitsError::InvalidLimit {
                field: "max_angular_speed",
                ..
            })
        ));

        let limits = SafetyLimits {
            max_linear_acceleration: f64::INFINITY,
            ..SafetyLimits::default()
        };

        assert!(matches!(
            limits.validate(),
            Err(SafetyLimitsError::InvalidLimit {
                field: "max_linear_acceleration",
                ..
            })
        ));
    }

    #[test]
    fn a_zero_command_timeout_is_rejected() {
        let limits = SafetyLimits {
            command_timeout: Duration::ZERO,
            ..SafetyLimits::default()
        };

        assert_eq!(
            limits.validate(),
            Err(SafetyLimitsError::ZeroCommandTimeout)
        );
    }
}
