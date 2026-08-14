//! Modele physique du robot.
//!
//! Aujourd'hui, la cinematique differentielle : la traduction entre ce que le coeur
//! decide — une vitesse de chassis — et ce que les moteurs comprennent — des vitesses de
//! roues.
//!
//! # Pourquoi cote calculateur
//!
//! Le rayon de roue et l'empattement sont deja connus ici : ils viennent du modele du
//! robot, celui-la meme qui decrit la simulation. Les dupliquer dans le firmware du
//! microcontroleur garantirait qu'un jour les deux ne concordent plus — et une erreur
//! d'empattement ne se voit pas, elle se traduit par une odometrie qui derive lentement.
//!
//! Le microcontroleur reste donc sans modele : il recoit des consignes par roue, il rend
//! des mesures par roue.
//!
//! # Ce qui viendra
//!
//! Les traits d'abstraction materielle arriveront en Milestone 5, quand il y aura du
//! materiel a abstraire. Les ecrire maintenant, avec une unique implementation
//! simulee, ne serait qu'une supposition sur ce dont le robot reel aura besoin.

mod kinematics;

pub use kinematics::{DifferentialDrive, GeometryError, WheelSpeeds};
