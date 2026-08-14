//! Couche de securite du robot.
//!
//! Cette crate est le dernier maillon avant la sortie moteur. Aucune consigne de vitesse
//! ne parvient au materiel sans avoir traverse [`SafetyLayer::evaluate`].
//!
//! # Ordre d'evaluation
//!
//! L'ordre n'est pas negociable et fait l'objet de tests dedies :
//!
//! 1. **Arret d'urgence engage** — priorite absolue, y compris a l'arret. Un arret
//!    d'urgence declenche pendant que le robot est au repos doit tout de meme forcer le
//!    passage en `SAFE_STOP`.
//! 2. **Autorite refusee** — l'etat courant n'autorise pas le mouvement : vitesse nulle
//!    immediate, sans violation (c'est le fonctionnement nominal).
//! 3. **Consigne non finie** (`NaN`, infini) — defaut logiciel, arret requis.
//! 4. **Watchdog de commande expire** — le flux de commandes s'est tari. Arret requis
//!    **si le robot roule** ou si la derniere consigne lui demandait de partir. S'il est
//!    deja immobile et qu'on ne lui demande rien, le constat est signale sans escalade :
//!    il est deja dans l'etat sur, et l'y verrouiller exigerait une intervention humaine
//!    apres chaque mission accomplie.
//! 5. **Saturation des vitesses** — la consigne est bornee, jamais rejetee.
//! 6. **Saturation des accelerations** — sur le `dt` ecoule depuis l'evaluation precedente.
//!
//! Un arret de securite produit une vitesse **nulle immediate**, court-circuitant le
//! limiteur d'acceleration. La deceleration physique est bornee par la rampe cote
//! microcontroleur et par l'inertie des moteurs.

mod layer;
mod limits;
mod violation;
mod watchdog;

pub use layer::{MotionAuthority, SafetyDecision, SafetyLayer};
pub use limits::{SafetyLimits, SafetyLimitsError};
pub use violation::SafetyViolation;
pub use watchdog::Watchdog;
