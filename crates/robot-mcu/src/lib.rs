//! Protocole serie entre le coeur Rust et le microcontroleur.
//!
//! Cette crate est `no_std` et sans dependance, parce qu'elle tourne aux **deux
//! extremites** de la liaison : sur le calculateur embarque et dans le firmware du
//! Raspberry Pi Pico 2. Un protocole implemente deux fois finit toujours par diverger ;
//! celui-ci n'existe qu'une fois.
//!
//! La specification complete est dans `docs/hardware/serial-protocol.md`.
//!
//! # Forme d'une trame
//!
//! ```text
//!                  ┌──── protege par le CRC ────┐
//! +---------+------+------+-----+---------+------+
//! | VERSION | TYPE | SEQ  | ... | PAYLOAD | CRC16|
//! +---------+------+------+-----+---------+------+
//!                                               │
//!                             COBS ─────────────┘ + 0x00
//! ```
//!
//! # Le microcontroleur ne connait pas le robot
//!
//! Les consignes sont exprimees **par roue**, en rad/s. La cinematique differentielle
//! vit cote calculateur, dans `robot-hal`, ou le rayon de roue et l'empattement sont deja
//! connus. Dupliquer ces constantes dans le firmware garantirait qu'un jour les deux ne
//! concordent plus.

#![no_std]

pub mod cobs;
pub mod crc;
mod decoder;
mod message;

pub use cobs::CobsError;
pub use crc::crc16_ccitt;
pub use decoder::Decoder;
pub use message::{
    DecodeError, EncodeError, MAX_ENCODED_LEN, MAX_FRAME_LEN, Message, PROTOCOL_VERSION,
    StatusFlags, StopReason, Telemetry, encode,
};
