//! Observabilite du coeur du robot.
//!
//! Le coeur n'ecrit pas de logs lui-meme : il emet des [`robot_core::RobotEvent`].
//! Cette crate les consomme pour produire deux choses complementaires :
//!
//! - des **logs structures** via [`tracing`], destines au diagnostic ;
//! - des **compteurs** et un **instantane**, destines a l'API et au dashboard en
//!   Milestone 7.
//!
//! Cette separation permet de tester le coeur sans souscripteur de logs, et de changer
//! de backend d'observabilite (OpenTelemetry, Prometheus) sans toucher a la logique
//! robotique.
//!
//! ```
//! use robot_core::{Robot, RobotConfig};
//! use robot_telemetry::TelemetryCollector;
//! use robot_types::Monotonic;
//!
//! let mut robot = Robot::new(RobotConfig::default()).unwrap();
//! let mut telemetry = TelemetryCollector::new();
//!
//! robot.tick(Monotonic::ZERO);
//! telemetry.collect_from(&mut robot);
//!
//! let snapshot = telemetry.snapshot(&robot, Monotonic::ZERO);
//! assert_eq!(snapshot.state, robot.state());
//! ```

mod collector;
mod counters;
mod snapshot;

pub use collector::TelemetryCollector;
pub use counters::EventCounters;
pub use snapshot::TelemetrySnapshot;
