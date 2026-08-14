/// Pose planaire du robot dans un repere fixe.
///
/// Convention ROS : `x` vers l'avant, `y` vers la gauche, `theta` en radians dans le
/// sens trigonometrique. Utilisee par l'odometrie a partir de la Milestone 2.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pose2d {
    /// Abscisse en metres.
    pub x: f64,
    /// Ordonnee en metres.
    pub y: f64,
    /// Cap en radians.
    pub theta: f64,
}

impl Pose2d {
    /// Origine du repere, cap nul.
    pub const ORIGIN: Self = Self {
        x: 0.0,
        y: 0.0,
        theta: 0.0,
    };

    /// Construit une pose.
    #[must_use]
    pub const fn new(x: f64, y: f64, theta: f64) -> Self {
        Self { x, y, theta }
    }

    /// Indique si les trois composantes sont finies.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.theta.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_origin_is_the_default_pose() {
        assert_eq!(Pose2d::default(), Pose2d::ORIGIN);
    }

    #[test]
    fn an_ordinary_pose_is_finite() {
        assert!(Pose2d::new(1.5, -2.0, 0.78).is_finite());
    }

    #[test]
    fn a_pose_with_a_non_finite_component_is_not_finite() {
        assert!(!Pose2d::new(f64::NAN, 0.0, 0.0).is_finite());
        assert!(!Pose2d::new(0.0, f64::INFINITY, 0.0).is_finite());
        assert!(!Pose2d::new(0.0, 0.0, f64::NAN).is_finite());
    }
}
