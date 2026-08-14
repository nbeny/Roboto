//! Logique du firmware du microcontroleur, sans une ligne de code materiel.
//!
//! # Pourquoi cette separation
//!
//! Un firmware melange deux natures de code. D'un cote l'asservissement et la
//! supervision : des decisions, qui se raisonnent, se testent et ou les erreurs sont
//! dangereuses. De l'autre la glu materielle — configuration des PIO, USB, pont en H —
//! qui ne se verifie que sur la carte, oscilloscope a la main.
//!
//! Cette crate contient la premiere, et rien de la seconde. Elle se teste sur un PC en
//! quelques microsecondes, et se compile pour Cortex-M33.
//!
//! Le binaire du firmware vivra dans `hardware/firmware/` en Milestone 5, quand il y aura
//! une carte pour l'executer. Il se contentera de brancher des broches sur ce qui est ici.
//!
//! # Ce que le microcontroleur sait, et ce qu'il ignore
//!
//! Il sait asservir deux roues a une vitesse, et couper les moteurs quand plus personne
//! ne lui parle. Il ignore tout du reste : ni empattement, ni rayon de roue, ni etat du
//! robot. La cinematique vit cote calculateur, dans `robot-hal`.

#![no_std]

mod pid;
mod supervisor;

pub use pid::{GainsError, PidGains, VelocityPid};
pub use supervisor::{Supervisor, WheelSetpoint};
