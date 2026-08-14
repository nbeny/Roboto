//! Traductions entre le vocabulaire ROS 2 et celui du coeur.
//!
//! Ce module ne contient aucune decision. Il convertit, il nomme, il analyse — et c'est
//! tout. Toute question de « le robot a-t-il le droit de faire ceci » revient a
//! `robot-core` et `robot-safety`.

use geometry_msgs::msg::Twist;
use robot_core::RobotState;
use robot_types::Velocity2d;

/// Topics et services exposes par le noeud.
pub mod names {
    /// Consignes de teleoperation. Nom standard, ce que publient `teleop_twist_keyboard`
    /// et `teleop_twist_joy` sans remappage.
    pub const TELEOP_CMD_VEL: &str = "/cmd_vel";

    /// Consignes de la pile de navigation. Nav2 y sera remappe en Milestone 6.
    ///
    /// Topic distinct de la teleoperation a dessein : c'est le topic d'arrivee qui
    /// determine la source, donc l'autorite. Un unique `/cmd_vel` partage rendrait la
    /// table d'habilitation du coeur inoperante.
    pub const NAVIGATION_CMD_VEL: &str = "/nav/cmd_vel";

    /// Demande de transition d'etat, par nom (`IDLE`, `TELEOPERATION`...).
    ///
    /// Un topic plutot qu'un service : un service demanderait un type `.srv` sur mesure,
    /// donc un paquet d'interfaces supplementaire a generer. A remplacer par un service
    /// propre quand l'API de la Milestone 7 fixera le contrat.
    pub const REQUEST_STATE: &str = "~/request_state";

    /// Etat operationnel courant, publie a chaque cycle.
    pub const STATE: &str = "~/state";

    /// Consigne de vitesse effectivement autorisee, apres la couche de securite.
    pub const SAFE_CMD_VEL: &str = "~/cmd_vel_safe";

    /// Engage l'arret d'urgence logiciel.
    pub const EMERGENCY_STOP: &str = "~/emergency_stop";

    /// Leve l'arret d'urgence logiciel.
    pub const CLEAR_EMERGENCY_STOP: &str = "~/clear_emergency_stop";

    /// Quitte `SAFE_STOP` pour revenir en `IDLE`.
    pub const CLEAR_SAFE_STOP: &str = "~/clear_safe_stop";
}

/// Extrait une consigne planaire d'un `Twist`.
///
/// Une base a entrainement differentiel n'a que deux degres de liberte : la composante
/// lineaire en x et la composante angulaire en z. Les quatre autres sont ignorees — un
/// emetteur qui les remplirait se trompe de type de robot.
#[must_use]
pub fn velocity_from_twist(twist: &Twist) -> Velocity2d {
    Velocity2d::new(twist.linear.x, twist.angular.z)
}

/// Construit un `Twist` a partir d'une consigne planaire.
#[must_use]
pub fn twist_from_velocity(velocity: Velocity2d) -> Twist {
    let mut twist = Twist::default();
    twist.linear.x = velocity.linear;
    twist.angular.z = velocity.angular;
    twist
}

/// Analyse un nom d'etat tel que publie sur [`names::REQUEST_STATE`].
///
/// Accepte la casse indifferemment et tolere les espaces autour.
#[must_use]
pub fn parse_state(raw: &str) -> Option<RobotState> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "BOOTING" => Some(RobotState::Booting),
        "IDLE" => Some(RobotState::Idle),
        "TELEOPERATION" => Some(RobotState::Teleoperation),
        "NAVIGATING" => Some(RobotState::Navigating),
        "PAUSED" => Some(RobotState::Paused),
        "ERROR" => Some(RobotState::Error),
        "SAFE_STOP" => Some(RobotState::SafeStop),
        "CHARGING" => Some(RobotState::Charging),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_twist_yields_its_planar_components() {
        let mut twist = Twist::default();
        twist.linear.x = 0.4;
        twist.angular.z = -1.2;

        let velocity = velocity_from_twist(&twist);

        assert_eq!(velocity.linear, 0.4);
        assert_eq!(velocity.angular, -1.2);
    }

    #[test]
    fn the_out_of_plane_components_are_ignored() {
        let mut twist = Twist::default();
        twist.linear.x = 0.4;
        twist.linear.y = 9.0;
        twist.linear.z = 9.0;
        twist.angular.x = 9.0;
        twist.angular.y = 9.0;
        twist.angular.z = 0.1;

        let velocity = velocity_from_twist(&twist);

        assert_eq!(velocity.linear, 0.4);
        assert_eq!(velocity.angular, 0.1);
    }

    #[test]
    fn a_velocity_round_trips_through_a_twist() {
        let velocity = Velocity2d::new(0.25, -0.75);

        let restored = velocity_from_twist(&twist_from_velocity(velocity));

        assert_eq!(restored, velocity);
    }

    #[test]
    fn a_twist_built_from_a_velocity_leaves_the_other_axes_at_zero() {
        let twist = twist_from_velocity(Velocity2d::new(0.3, 0.2));

        assert_eq!(twist.linear.y, 0.0);
        assert_eq!(twist.linear.z, 0.0);
        assert_eq!(twist.angular.x, 0.0);
        assert_eq!(twist.angular.y, 0.0);
    }

    #[test]
    fn state_names_are_parsed_case_insensitively() {
        assert_eq!(parse_state("IDLE"), Some(RobotState::Idle));
        assert_eq!(parse_state("idle"), Some(RobotState::Idle));
        assert_eq!(parse_state("  Safe_Stop \n"), Some(RobotState::SafeStop));
        assert_eq!(parse_state("TELEOPERATION"), Some(RobotState::Teleoperation));
    }

    #[test]
    fn an_unknown_state_name_is_rejected() {
        assert_eq!(parse_state(""), None);
        assert_eq!(parse_state("FLYING"), None);
        assert_eq!(parse_state("SAFESTOP"), None);
    }

    #[test]
    fn every_state_name_round_trips_through_its_label() {
        for state in [
            RobotState::Booting,
            RobotState::Idle,
            RobotState::Teleoperation,
            RobotState::Navigating,
            RobotState::Paused,
            RobotState::Error,
            RobotState::SafeStop,
            RobotState::Charging,
        ] {
            assert_eq!(
                parse_state(state.as_str()),
                Some(state),
                "le libelle de {state} n'est pas reanalysable"
            );
        }
    }
}
