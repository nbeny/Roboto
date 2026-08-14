//! Types fondamentaux partages par l'ensemble du coeur du robot.
//!
//! Cette crate n'a aucune dependance externe obligatoire et ne connait ni ROS 2,
//! ni le materiel, ni aucun runtime asynchrone. Elle definit le vocabulaire commun
//! a `robot-safety`, `robot-core` et `robot-telemetry`.
//!
//! # Le temps entre par la porte
//!
//! Aucun type de cette crate ne lit l'horloge systeme. Les instants sont representes
//! par [`Monotonic`], que l'appelant fournit explicitement. C'est ce qui rend le coeur
//! du robot deterministe et testable sans materiel : un test peut fabriquer une
//! seconde entiere sans dormir.

mod command;
mod pose;
mod time;
mod velocity;

pub use command::{CommandSource, MotionCommand};
pub use pose::Pose2d;
pub use time::Monotonic;
pub use velocity::{STATIONARY_EPSILON, Velocity2d};
