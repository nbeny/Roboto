use robot_types::Velocity2d;
use thiserror::Error;

/// Vitesses angulaires des roues motrices, en rad/s.
///
/// Positives vers l'avant, pour les deux roues. C'est bien une convention : le montage
/// physique des moteurs est symetrique, donc l'un des deux tourne en sens inverse de
/// l'autre pour avancer. Ce retournement appartient au cablage et au firmware, pas au
/// modele cinematique.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WheelSpeeds {
    /// Roue gauche, en rad/s.
    pub left: f64,
    /// Roue droite, en rad/s.
    pub right: f64,
}

impl WheelSpeeds {
    /// Les deux roues a l'arret.
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
    };

    /// Construit un couple de vitesses de roues.
    #[must_use]
    pub const fn new(left: f64, right: f64) -> Self {
        Self { left, right }
    }

    /// Indique si les deux vitesses sont finies.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.left.is_finite() && self.right.is_finite()
    }
}

/// Geometrie inexploitable.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum GeometryError {
    /// Le rayon de roue n'est pas fini et strictement positif.
    #[error("rayon de roue invalide : {0} m")]
    InvalidWheelRadius(f64),
    /// L'empattement n'est pas fini et strictement positif.
    #[error("empattement invalide : {0} m")]
    InvalidWheelSeparation(f64),
}

/// Cinematique d'une base a entrainement differentiel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DifferentialDrive {
    wheel_radius: f64,
    wheel_separation: f64,
}

impl DifferentialDrive {
    /// Construit une cinematique a partir du rayon de roue et de l'empattement, en metres.
    ///
    /// # Errors
    ///
    /// Renvoie [`GeometryError`] si l'une des deux grandeurs n'est pas finie et
    /// strictement positive. Un rayon nul rendrait la conversion infinie, un empattement
    /// nul rendrait toute rotation impossible a exprimer.
    pub fn new(wheel_radius: f64, wheel_separation: f64) -> Result<Self, GeometryError> {
        // `is_finite` d'abord : toute comparaison impliquant un NaN est fausse, y compris
        // `NaN <= 0.0`, et laisserait passer la valeur.
        if !wheel_radius.is_finite() || wheel_radius <= 0.0 {
            return Err(GeometryError::InvalidWheelRadius(wheel_radius));
        }
        if !wheel_separation.is_finite() || wheel_separation <= 0.0 {
            return Err(GeometryError::InvalidWheelSeparation(wheel_separation));
        }

        Ok(Self {
            wheel_radius,
            wheel_separation,
        })
    }

    /// Rayon de roue, en metres.
    #[must_use]
    pub const fn wheel_radius(&self) -> f64 {
        self.wheel_radius
    }

    /// Empattement, en metres.
    #[must_use]
    pub const fn wheel_separation(&self) -> f64 {
        self.wheel_separation
    }

    /// Traduit une vitesse de chassis en vitesses de roues.
    ///
    /// ```text
    /// omega_gauche = (v - omega * L / 2) / r
    /// omega_droite = (v + omega * L / 2) / r
    /// ```
    #[must_use]
    pub fn to_wheel_speeds(&self, velocity: Velocity2d) -> WheelSpeeds {
        let half_track = velocity.angular * self.wheel_separation / 2.0;

        WheelSpeeds {
            left: (velocity.linear - half_track) / self.wheel_radius,
            right: (velocity.linear + half_track) / self.wheel_radius,
        }
    }

    /// Reconstruit la vitesse du chassis a partir des vitesses de roues mesurees.
    ///
    /// C'est cette direction qui produira l'odometrie a partir des encodeurs.
    ///
    /// ```text
    /// v     = r * (omega_droite + omega_gauche) / 2
    /// omega = r * (omega_droite - omega_gauche) / L
    /// ```
    #[must_use]
    pub fn to_body_velocity(&self, wheels: WheelSpeeds) -> Velocity2d {
        Velocity2d {
            linear: self.wheel_radius * (wheels.right + wheels.left) / 2.0,
            angular: self.wheel_radius * (wheels.right - wheels.left) / self.wheel_separation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Geometrie du robot decrit par la nomenclature : roues de 85 mm, empattement 25 cm.
    fn roboto() -> DifferentialDrive {
        DifferentialDrive::new(0.0425, 0.25).expect("geometrie de la nomenclature")
    }

    const TOLERANCE: f64 = 1e-9;

    fn assert_close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < TOLERANCE,
            "{what} : obtenu {actual}, attendu {expected}"
        );
    }

    // --- Construction -------------------------------------------------------------

    #[test]
    fn the_bom_geometry_is_accepted() {
        let drive = roboto();

        assert_close(drive.wheel_radius(), 0.0425, "rayon");
        assert_close(drive.wheel_separation(), 0.25, "empattement");
    }

    #[test]
    fn a_zero_wheel_radius_is_rejected() {
        assert_eq!(
            DifferentialDrive::new(0.0, 0.25),
            Err(GeometryError::InvalidWheelRadius(0.0))
        );
    }

    #[test]
    fn a_negative_wheel_separation_is_rejected() {
        assert_eq!(
            DifferentialDrive::new(0.0425, -0.25),
            Err(GeometryError::InvalidWheelSeparation(-0.25))
        );
    }

    #[test]
    fn a_non_finite_geometry_is_rejected() {
        assert!(DifferentialDrive::new(f64::NAN, 0.25).is_err());
        assert!(DifferentialDrive::new(0.0425, f64::INFINITY).is_err());
    }

    // --- Chassis vers roues ---------------------------------------------------------

    #[test]
    fn a_robot_at_rest_turns_no_wheel() {
        assert_eq!(
            roboto().to_wheel_speeds(Velocity2d::ZERO),
            WheelSpeeds::ZERO
        );
    }

    #[test]
    fn moving_straight_turns_both_wheels_alike() {
        let drive = roboto();

        let wheels = drive.to_wheel_speeds(Velocity2d::new(0.5, 0.0));

        assert_close(wheels.left, wheels.right, "symetrie en ligne droite");
        // 0,5 m/s sur une roue de 0,0425 m de rayon.
        assert_close(wheels.left, 0.5 / 0.0425, "vitesse de roue");
    }

    #[test]
    fn turning_in_place_turns_the_wheels_in_opposite_directions() {
        let drive = roboto();

        let wheels = drive.to_wheel_speeds(Velocity2d::new(0.0, 1.0));

        assert_close(wheels.left, -wheels.right, "antisymetrie en rotation pure");
        // omega * L / 2 / r = 1,0 * 0,125 / 0,0425
        assert_close(wheels.right, 0.125 / 0.0425, "vitesse de roue");
    }

    #[test]
    fn a_positive_rotation_turns_left() {
        // Convention ROS : omega positif tourne dans le sens trigonometrique, donc vers
        // la gauche. La roue droite doit alors aller plus vite que la gauche.
        let wheels = roboto().to_wheel_speeds(Velocity2d::new(0.3, 0.5));

        assert!(
            wheels.right > wheels.left,
            "une rotation trigonometrique doit accelerer la roue droite"
        );
    }

    #[test]
    fn reversing_reverses_both_wheels() {
        let drive = roboto();

        let forward = drive.to_wheel_speeds(Velocity2d::new(0.4, 0.0));
        let backward = drive.to_wheel_speeds(Velocity2d::new(-0.4, 0.0));

        assert_close(backward.left, -forward.left, "roue gauche");
        assert_close(backward.right, -forward.right, "roue droite");
    }

    // --- Roues vers chassis ---------------------------------------------------------

    #[test]
    fn stationary_wheels_yield_a_stationary_body() {
        assert_eq!(
            roboto().to_body_velocity(WheelSpeeds::ZERO),
            Velocity2d::ZERO
        );
    }

    #[test]
    fn the_conversion_round_trips() {
        let drive = roboto();

        for velocity in [
            Velocity2d::new(0.5, 0.0),
            Velocity2d::new(0.0, 1.2),
            Velocity2d::new(0.32, -0.75),
            Velocity2d::new(-0.18, 0.4),
        ] {
            let restored = drive.to_body_velocity(drive.to_wheel_speeds(velocity));

            assert_close(restored.linear, velocity.linear, "vitesse lineaire");
            assert_close(restored.angular, velocity.angular, "vitesse angulaire");
        }
    }

    #[test]
    fn a_wider_track_needs_faster_wheels_for_the_same_rotation() {
        let narrow = DifferentialDrive::new(0.0425, 0.20).expect("geometrie valide");
        let wide = DifferentialDrive::new(0.0425, 0.40).expect("geometrie valide");

        let rotation = Velocity2d::new(0.0, 1.0);

        assert!(
            wide.to_wheel_speeds(rotation).right > narrow.to_wheel_speeds(rotation).right,
            "un empattement plus large exige des roues plus rapides"
        );
    }

    #[test]
    fn a_bigger_wheel_needs_fewer_turns_for_the_same_speed() {
        let small = DifferentialDrive::new(0.03, 0.25).expect("geometrie valide");
        let big = DifferentialDrive::new(0.06, 0.25).expect("geometrie valide");

        let forward = Velocity2d::new(0.5, 0.0);

        assert!(
            big.to_wheel_speeds(forward).left < small.to_wheel_speeds(forward).left,
            "une roue plus grande tourne moins vite a vitesse egale"
        );
    }

    #[test]
    fn wheel_speeds_report_whether_they_are_finite() {
        assert!(WheelSpeeds::ZERO.is_finite());
        assert!(!WheelSpeeds::new(f64::NAN, 0.0).is_finite());
        assert!(!WheelSpeeds::new(0.0, f64::INFINITY).is_finite());
    }
}
