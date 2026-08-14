/// Seuil en deca duquel une composante de vitesse est consideree comme nulle.
///
/// Exprime en m/s pour la composante lineaire et en rad/s pour l'angulaire.
pub const STATIONARY_EPSILON: f64 = 1e-6;

/// Consigne de vitesse dans le plan, pour une base a entrainement differentiel.
///
/// Convention ROS : `linear` positif fait avancer le robot, `angular` positif le fait
/// tourner dans le sens trigonometrique (vers la gauche).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Velocity2d {
    /// Vitesse lineaire en m/s, positive vers l'avant.
    pub linear: f64,
    /// Vitesse angulaire en rad/s, positive dans le sens trigonometrique.
    pub angular: f64,
}

impl Velocity2d {
    /// Robot a l'arret.
    pub const ZERO: Self = Self {
        linear: 0.0,
        angular: 0.0,
    };

    /// Construit une consigne de vitesse.
    #[must_use]
    pub const fn new(linear: f64, angular: f64) -> Self {
        Self { linear, angular }
    }

    /// Indique si les deux composantes sont finies.
    ///
    /// Une consigne contenant un `NaN` ou un infini est un defaut logiciel, jamais une
    /// commande legitime. La couche de securite la rejette et declenche un arret.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.linear.is_finite() && self.angular.is_finite()
    }

    /// Indique si les deux composantes sont sous [`STATIONARY_EPSILON`] en valeur absolue.
    ///
    /// Une consigne non finie n'est pas consideree comme immobile.
    #[must_use]
    pub fn is_stationary(self) -> bool {
        self.is_finite()
            && self.linear.abs() < STATIONARY_EPSILON
            && self.angular.abs() < STATIONARY_EPSILON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_stationary() {
        assert!(Velocity2d::ZERO.is_stationary());
    }

    #[test]
    fn a_velocity_below_the_epsilon_is_stationary() {
        let creeping = Velocity2d::new(STATIONARY_EPSILON / 2.0, -STATIONARY_EPSILON / 2.0);

        assert!(creeping.is_stationary());
    }

    #[test]
    fn a_moving_velocity_is_not_stationary() {
        assert!(!Velocity2d::new(0.3, 0.0).is_stationary());
        assert!(!Velocity2d::new(0.0, 0.3).is_stationary());
    }

    #[test]
    fn a_nan_velocity_is_not_finite() {
        assert!(!Velocity2d::new(f64::NAN, 0.0).is_finite());
        assert!(!Velocity2d::new(0.0, f64::NAN).is_finite());
    }

    #[test]
    fn an_infinite_velocity_is_not_finite() {
        assert!(!Velocity2d::new(f64::INFINITY, 0.0).is_finite());
        assert!(!Velocity2d::new(0.0, f64::NEG_INFINITY).is_finite());
    }

    #[test]
    fn a_nan_velocity_is_not_reported_as_stationary() {
        assert!(!Velocity2d::new(f64::NAN, f64::NAN).is_stationary());
    }

    #[test]
    fn an_ordinary_velocity_is_finite() {
        assert!(Velocity2d::new(0.42, -1.2).is_finite());
        assert!(Velocity2d::ZERO.is_finite());
    }

    #[test]
    fn the_default_velocity_is_zero() {
        assert_eq!(Velocity2d::default(), Velocity2d::ZERO);
    }
}
