//! Coeur metier du robot.
//!
//! Cette crate ne connait ni ROS 2, ni le materiel, ni aucun runtime asynchrone. Elle
//! orchestre la machine a etats, arbitre l'autorite des commandes et delegue chaque
//! consigne a [`robot_safety`] avant de la laisser sortir.
//!
//! # Le temps entre par la porte
//!
//! Aucune methode ne lit l'horloge. L'appelant fournit un [`robot_types::Monotonic`] :
//!
//! ```
//! use core::time::Duration;
//! use robot_core::{Robot, RobotConfig, RobotState};
//! use robot_types::{CommandSource, MotionCommand, Monotonic, Velocity2d};
//!
//! let config = RobotConfig::default();
//! let mut robot = Robot::new(config).unwrap();
//!
//! // Le premier cycle etablit la reference de demarrage ; le compte a rebours part
//! // de la, pas de la construction du robot.
//! robot.tick(Monotonic::ZERO);
//!
//! let boot_done = Monotonic::ZERO + config.boot_duration;
//! robot.tick(boot_done);
//! assert_eq!(robot.state(), RobotState::Idle);
//!
//! robot.request_state(RobotState::Teleoperation, boot_done).unwrap();
//! robot
//!     .submit_command(
//!         MotionCommand::new(
//!             Velocity2d::new(0.2, 0.0),
//!             CommandSource::Teleoperation,
//!             boot_done,
//!         ),
//!         boot_done,
//!     )
//!     .unwrap();
//!
//! let output = robot.tick(boot_done + Duration::from_millis(20));
//! assert!(output.velocity.linear > 0.0);
//! ```
//!
//! # L'IA ne pilote pas les moteurs
//!
//! [`robot_types::CommandSource::Ai`] n'est habilitee dans aucun etat. L'agent IA
//! exprime des objectifs de haut niveau ; il ne soumet jamais de vitesse brute.

mod event;
mod robot;
mod state;

pub use event::{CommandRejection, RobotEvent, TransitionError, TransitionReason};
pub use robot::{ControlOutput, Robot, RobotConfig};
pub use state::RobotState;

// Types de `robot-safety` qui apparaissent dans l'API publique de cette crate : les
// reexporter evite a chaque consommateur de dependre explicitement de la couche de
// securite pour nommer ce que le coeur lui rend deja.
pub use robot_safety::{SafetyLimits, SafetyLimitsError, SafetyViolation};
