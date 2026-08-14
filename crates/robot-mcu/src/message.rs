use core::fmt;

use crate::cobs::{self, CobsError};
use crate::crc::crc16_ccitt;

/// Version du protocole implementee par cette crate.
pub const PROTOCOL_VERSION: u8 = 1;

/// Taille de l'en-tete : version, type, sequence.
const HEADER_LEN: usize = 3;
/// Taille du CRC en fin de trame.
const CRC_LEN: usize = 2;
/// Charge utile la plus longue du protocole, celle de [`Telemetry`].
const MAX_PAYLOAD_LEN: usize = 21;

/// Taille maximale d'une trame avant encodage COBS.
pub const MAX_FRAME_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN + CRC_LEN;

/// Taille maximale d'une trame encodee, delimiteur compris.
pub const MAX_ENCODED_LEN: usize = cobs::max_encoded_len(MAX_FRAME_LEN) + 1;

/// Motif d'un arret demande par le calculateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Demande explicite d'un operateur.
    Operator,
    /// Arret de securite decide par le coeur.
    SafeStop,
    /// Arret d'urgence.
    EmergencyStop,
    /// Extinction ordonnee du robot.
    Shutdown,
}

impl StopReason {
    /// Code transmis sur la liaison.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Operator => 0,
            Self::SafeStop => 1,
            Self::EmergencyStop => 2,
            Self::Shutdown => 3,
        }
    }

    /// Reconstruit un motif depuis son code, ou `None` s'il est inconnu.
    #[must_use]
    pub const fn from_u8(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Operator),
            1 => Some(Self::SafeStop),
            2 => Some(Self::EmergencyStop),
            3 => Some(Self::Shutdown),
            _ => None,
        }
    }
}

/// Drapeaux d'etat rapportes par le microcontroleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusFlags(u8);

impl StatusFlags {
    /// Etage de puissance actif.
    pub const MOTORS_ENABLED: u8 = 1 << 0;
    /// Aucune commande recue dans le delai imparti.
    pub const WATCHDOG_EXPIRED: u8 = 1 << 1;
    /// Le pont en H signale un defaut.
    pub const DRIVER_FAULT: u8 = 1 << 2;
    /// Surintensite detectee.
    pub const OVERCURRENT: u8 = 1 << 3;
    /// Encodeur incoherent ou muet.
    pub const ENCODER_FAULT: u8 = 1 << 4;

    /// Construit des drapeaux depuis leur representation binaire.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Representation binaire.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Indique si un drapeau est leve.
    #[must_use]
    pub const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    /// Indique si un defaut interdit le mouvement.
    #[must_use]
    pub const fn is_faulted(self) -> bool {
        self.0 & (Self::DRIVER_FAULT | Self::OVERCURRENT | Self::ENCODER_FAULT) != 0
    }
}

/// Mesures rapportees par le microcontroleur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    /// Vitesse mesuree de la roue gauche, en rad/s.
    pub left: f32,
    /// Vitesse mesuree de la roue droite, en rad/s.
    pub right: f32,
    /// Cumul d'impulsions de l'encodeur gauche.
    pub left_ticks: i32,
    /// Cumul d'impulsions de l'encodeur droit.
    pub right_ticks: i32,
    /// Horloge du microcontroleur, en millisecondes.
    pub timestamp_ms: u32,
    /// Drapeaux d'etat.
    pub flags: StatusFlags,
}

/// Message echange sur la liaison serie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    /// Consigne de vitesse par roue, en rad/s. Reame le watchdog du microcontroleur.
    WheelVelocity {
        /// Consigne de la roue gauche.
        left: f32,
        /// Consigne de la roue droite.
        right: f32,
        /// Horloge de l'emetteur, en millisecondes.
        timestamp_ms: u32,
    },
    /// Arret immediat, sans rampe, verrouille jusqu'a la prochaine consigne.
    Stop {
        /// Motif de l'arret.
        reason: StopReason,
        /// Horloge de l'emetteur, en millisecondes.
        timestamp_ms: u32,
    },
    /// Signe de vie sans consigne. Reame le watchdog sans rien changer d'autre.
    Heartbeat {
        /// Horloge de l'emetteur, en millisecondes.
        timestamp_ms: u32,
    },
    /// Mesures periodiques du microcontroleur.
    Telemetry(Telemetry),
    /// Defaut signale par le microcontroleur.
    Fault {
        /// Code de defaut.
        code: u8,
        /// Horloge du microcontroleur, en millisecondes.
        timestamp_ms: u32,
    },
}

impl Message {
    /// Code de type transmis sur la liaison.
    ///
    /// Le bit 7 vaut 1 pour les messages emis par le microcontroleur : un coup d'oeil au
    /// type suffit a savoir dans quel sens circule la trame.
    #[must_use]
    pub const fn message_type(&self) -> u8 {
        match self {
            Self::WheelVelocity { .. } => 0x01,
            Self::Stop { .. } => 0x02,
            Self::Heartbeat { .. } => 0x03,
            Self::Telemetry(_) => 0x81,
            Self::Fault { .. } => 0x82,
        }
    }

    /// Indique si le message est emis par le microcontroleur.
    #[must_use]
    pub const fn is_from_mcu(&self) -> bool {
        self.message_type() & 0x80 != 0
    }

    /// Ecrit la charge utile et renvoie sa longueur.
    fn write_payload(&self, output: &mut [u8]) -> Result<usize, EncodeError> {
        match *self {
            Self::WheelVelocity {
                left,
                right,
                timestamp_ms,
            } => {
                reject_non_finite(left, right)?;
                room_for(output, 12)?;
                output[0..4].copy_from_slice(&left.to_le_bytes());
                output[4..8].copy_from_slice(&right.to_le_bytes());
                output[8..12].copy_from_slice(&timestamp_ms.to_le_bytes());
                Ok(12)
            }
            Self::Stop {
                reason,
                timestamp_ms,
            } => {
                room_for(output, 5)?;
                output[0] = reason.as_u8();
                output[1..5].copy_from_slice(&timestamp_ms.to_le_bytes());
                Ok(5)
            }
            Self::Heartbeat { timestamp_ms } => {
                room_for(output, 4)?;
                output[0..4].copy_from_slice(&timestamp_ms.to_le_bytes());
                Ok(4)
            }
            Self::Telemetry(measured) => {
                reject_non_finite(measured.left, measured.right)?;
                room_for(output, 21)?;
                output[0..4].copy_from_slice(&measured.left.to_le_bytes());
                output[4..8].copy_from_slice(&measured.right.to_le_bytes());
                output[8..12].copy_from_slice(&measured.left_ticks.to_le_bytes());
                output[12..16].copy_from_slice(&measured.right_ticks.to_le_bytes());
                output[16..20].copy_from_slice(&measured.timestamp_ms.to_le_bytes());
                output[20] = measured.flags.bits();
                Ok(21)
            }
            Self::Fault { code, timestamp_ms } => {
                room_for(output, 5)?;
                output[0] = code;
                output[1..5].copy_from_slice(&timestamp_ms.to_le_bytes());
                Ok(5)
            }
        }
    }

    /// Reconstruit un message depuis son type et sa charge utile.
    fn parse(message_type: u8, payload: &[u8]) -> Result<Self, DecodeError> {
        match message_type {
            0x01 => {
                exact_len(message_type, payload, 12)?;
                let left = read_f32(payload, 0);
                let right = read_f32(payload, 4);
                accept_finite(left, right)?;
                Ok(Self::WheelVelocity {
                    left,
                    right,
                    timestamp_ms: read_u32(payload, 8),
                })
            }
            0x02 => {
                exact_len(message_type, payload, 5)?;
                let reason = StopReason::from_u8(payload[0])
                    .ok_or(DecodeError::InvalidStopReason(payload[0]))?;
                Ok(Self::Stop {
                    reason,
                    timestamp_ms: read_u32(payload, 1),
                })
            }
            0x03 => {
                exact_len(message_type, payload, 4)?;
                Ok(Self::Heartbeat {
                    timestamp_ms: read_u32(payload, 0),
                })
            }
            0x81 => {
                exact_len(message_type, payload, 21)?;
                let left = read_f32(payload, 0);
                let right = read_f32(payload, 4);
                accept_finite(left, right)?;
                Ok(Self::Telemetry(Telemetry {
                    left,
                    right,
                    left_ticks: read_i32(payload, 8),
                    right_ticks: read_i32(payload, 12),
                    timestamp_ms: read_u32(payload, 16),
                    flags: StatusFlags::from_bits(payload[20]),
                }))
            }
            0x82 => {
                exact_len(message_type, payload, 5)?;
                Ok(Self::Fault {
                    code: payload[0],
                    timestamp_ms: read_u32(payload, 1),
                })
            }
            unknown => Err(DecodeError::UnknownMessageType(unknown)),
        }
    }
}

fn room_for(output: &[u8], required: usize) -> Result<(), EncodeError> {
    if output.len() < required {
        return Err(EncodeError::OutputTooSmall {
            available: output.len(),
            required,
        });
    }
    Ok(())
}

fn reject_non_finite(left: f32, right: f32) -> Result<(), EncodeError> {
    if left.is_finite() && right.is_finite() {
        Ok(())
    } else {
        Err(EncodeError::NonFiniteVelocity)
    }
}

/// Refuse une vitesse non finie a la reception.
///
/// Une trame peut etre parfaitement intacte et contenir malgre tout un `NaN`, si
/// l'emetteur a un defaut. La frontiere le refuse plutot que de le propager.
fn accept_finite(left: f32, right: f32) -> Result<(), DecodeError> {
    if left.is_finite() && right.is_finite() {
        Ok(())
    } else {
        Err(DecodeError::NonFiniteVelocity)
    }
}

fn exact_len(message_type: u8, payload: &[u8], expected: usize) -> Result<(), DecodeError> {
    if payload.len() == expected {
        Ok(())
    } else {
        Err(DecodeError::UnexpectedPayloadLength {
            message_type,
            expected,
            found: payload.len(),
        })
    }
}

fn read_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_i32(payload: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

/// Echec d'encodage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// Le tampon de sortie est trop petit.
    OutputTooSmall {
        /// Taille disponible.
        available: usize,
        /// Taille necessaire.
        required: usize,
    },
    /// Une consigne contient un `NaN` ou un infini.
    NonFiniteVelocity,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputTooSmall {
                available,
                required,
            } => write!(
                f,
                "tampon de sortie trop petit : {available} octets pour {required} requis"
            ),
            Self::NonFiniteVelocity => f.write_str("consigne de vitesse non finie"),
        }
    }
}

impl core::error::Error for EncodeError {}

/// Echec de decodage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// La trame encodee est malformee.
    Cobs(CobsError),
    /// La trame est trop courte pour contenir un en-tete et un CRC.
    TooShort {
        /// Longueur constatee.
        len: usize,
    },
    /// La trame depasse la taille maximale du protocole.
    TooLong {
        /// Longueur constatee.
        len: usize,
    },
    /// Le CRC ne correspond pas.
    ChecksumMismatch {
        /// CRC transmis.
        expected: u16,
        /// CRC recalcule.
        computed: u16,
    },
    /// Version de protocole non geree.
    UnsupportedVersion {
        /// Version recue.
        found: u8,
        /// Version geree.
        expected: u8,
    },
    /// Type de message inconnu.
    UnknownMessageType(u8),
    /// La charge utile n'a pas la longueur attendue pour ce type.
    UnexpectedPayloadLength {
        /// Type concerne.
        message_type: u8,
        /// Longueur attendue.
        expected: usize,
        /// Longueur constatee.
        found: usize,
    },
    /// Motif d'arret inconnu.
    InvalidStopReason(u8),
    /// Une consigne contient un `NaN` ou un infini.
    NonFiniteVelocity,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cobs(error) => write!(f, "trame COBS invalide : {error}"),
            Self::TooShort { len } => write!(f, "trame trop courte : {len} octets"),
            Self::TooLong { len } => write!(f, "trame trop longue : {len} octets"),
            Self::ChecksumMismatch { expected, computed } => write!(
                f,
                "CRC incorrect : transmis {expected:#06x}, recalcule {computed:#06x}"
            ),
            Self::UnsupportedVersion { found, expected } => write!(
                f,
                "version de protocole {found} non geree, cette implementation parle la {expected}"
            ),
            Self::UnknownMessageType(code) => write!(f, "type de message inconnu : {code:#04x}"),
            Self::UnexpectedPayloadLength {
                message_type,
                expected,
                found,
            } => write!(
                f,
                "charge utile de {found} octets pour le type {message_type:#04x}, {expected} attendus"
            ),
            Self::InvalidStopReason(code) => write!(f, "motif d'arret inconnu : {code}"),
            Self::NonFiniteVelocity => f.write_str("consigne de vitesse non finie"),
        }
    }
}

impl core::error::Error for DecodeError {}

impl From<CobsError> for DecodeError {
    fn from(error: CobsError) -> Self {
        Self::Cobs(error)
    }
}

/// Encode un message en trame prete a emettre, delimiteur `0x00` compris.
///
/// # Errors
///
/// Renvoie [`EncodeError`] si le tampon est trop petit ou si une consigne n'est pas finie.
pub fn encode(message: &Message, sequence: u8, output: &mut [u8]) -> Result<usize, EncodeError> {
    let mut frame = [0_u8; MAX_FRAME_LEN];
    frame[0] = PROTOCOL_VERSION;
    frame[1] = message.message_type();
    frame[2] = sequence;

    let payload_len = message.write_payload(&mut frame[HEADER_LEN..])?;
    let body_len = HEADER_LEN + payload_len;

    let checksum = crc16_ccitt(&frame[..body_len]);
    frame[body_len..body_len + CRC_LEN].copy_from_slice(&checksum.to_le_bytes());
    let frame_len = body_len + CRC_LEN;

    // Verifie la place avant d'encoder, delimiteur compris : cela evite d'avoir a
    // traduire une erreur COBS en erreur d'encodage, et rend le message d'erreur exact.
    let required = cobs::max_encoded_len(frame_len) + 1;
    room_for(output, required)?;

    let encoded_len =
        cobs::encode(&frame[..frame_len], output).map_err(|_| EncodeError::OutputTooSmall {
            available: output.len(),
            required,
        })?;
    output[encoded_len] = 0;

    Ok(encoded_len + 1)
}

/// Decode une trame **deja debarrassee de son encodage COBS et de son delimiteur**.
///
/// Renvoie le numero de sequence et le message.
///
/// # Errors
///
/// Renvoie [`DecodeError`] si la trame est corrompue, tronquee, ou d'une version ou d'un
/// type non geres.
pub fn decode_frame(frame: &[u8]) -> Result<(u8, Message), DecodeError> {
    if frame.len() > MAX_FRAME_LEN {
        return Err(DecodeError::TooLong { len: frame.len() });
    }
    if frame.len() < HEADER_LEN + CRC_LEN {
        return Err(DecodeError::TooShort { len: frame.len() });
    }

    let body_len = frame.len() - CRC_LEN;

    // Le CRC d'abord, la version ensuite. Si le controle d'integrite echoue, plus rien
    // n'est digne de confiance — pas meme l'octet de version. Annoncer une version
    // invalide serait un diagnostic trompeur.
    let expected = u16::from_le_bytes([frame[body_len], frame[body_len + 1]]);
    let computed = crc16_ccitt(&frame[..body_len]);
    if expected != computed {
        return Err(DecodeError::ChecksumMismatch { expected, computed });
    }

    let version = frame[0];
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            found: version,
            expected: PROTOCOL_VERSION,
        });
    }

    let message = Message::parse(frame[1], &frame[HEADER_LEN..body_len])?;

    Ok((frame[2], message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn telemetry() -> Message {
        Message::Telemetry(Telemetry {
            left: 12.5,
            right: -12.5,
            left_ticks: 123_456,
            right_ticks: -654_321,
            timestamp_ms: 1_000_000,
            flags: StatusFlags::from_bits(
                StatusFlags::MOTORS_ENABLED | StatusFlags::WATCHDOG_EXPIRED,
            ),
        })
    }

    fn every_message() -> [Message; 5] {
        [
            Message::WheelVelocity {
                left: 3.25,
                right: -1.75,
                timestamp_ms: 42_000,
            },
            Message::Stop {
                reason: StopReason::EmergencyStop,
                timestamp_ms: 7,
            },
            Message::Heartbeat {
                timestamp_ms: u32::MAX,
            },
            telemetry(),
            Message::Fault {
                code: 9,
                timestamp_ms: 123,
            },
        ]
    }

    /// Encode puis decode, en retirant le delimiteur et l'encodage COBS.
    fn round_trip(message: &Message, sequence: u8) -> (u8, Message) {
        let mut encoded = [0_u8; MAX_ENCODED_LEN];
        let encoded_len = encode(message, sequence, &mut encoded).expect("encodage");

        assert_eq!(
            encoded[encoded_len - 1],
            0,
            "la trame doit s'achever sur le delimiteur"
        );
        assert!(
            !encoded[..encoded_len - 1].contains(&0),
            "aucun octet nul ne doit subsister avant le delimiteur"
        );

        let mut frame = [0_u8; MAX_FRAME_LEN];
        let frame_len = cobs::decode(&encoded[..encoded_len - 1], &mut frame).expect("COBS");

        decode_frame(&frame[..frame_len]).expect("decodage")
    }

    // --- Aller-retour -------------------------------------------------------------

    #[test]
    fn every_message_survives_a_round_trip() {
        for message in every_message() {
            let (sequence, decoded) = round_trip(&message, 0x2A);

            assert_eq!(decoded, message, "message altere par l'aller-retour");
            assert_eq!(sequence, 0x2A, "sequence alteree");
        }
    }

    #[test]
    fn the_sequence_number_wraps_without_ceremony() {
        for sequence in [0_u8, 1, 127, 128, 255] {
            let (decoded, _) = round_trip(&Message::Heartbeat { timestamp_ms: 1 }, sequence);
            assert_eq!(decoded, sequence);
        }
    }

    #[test]
    fn extreme_timestamps_survive() {
        let (_, decoded) = round_trip(
            &Message::Heartbeat {
                timestamp_ms: u32::MAX,
            },
            0,
        );

        assert_eq!(
            decoded,
            Message::Heartbeat {
                timestamp_ms: u32::MAX
            }
        );
    }

    #[test]
    fn negative_tick_counts_survive() {
        let (_, decoded) = round_trip(&telemetry(), 0);

        let Message::Telemetry(measured) = decoded else {
            panic!("type altere");
        };
        assert_eq!(measured.right_ticks, -654_321);
    }

    // --- Direction et typage ------------------------------------------------------

    #[test]
    fn the_direction_is_readable_from_the_type_byte() {
        assert!(!Message::Heartbeat { timestamp_ms: 0 }.is_from_mcu());
        assert!(telemetry().is_from_mcu());
        assert!(
            Message::Fault {
                code: 0,
                timestamp_ms: 0
            }
            .is_from_mcu()
        );
    }

    // --- Tailles ------------------------------------------------------------------

    #[test]
    fn no_message_exceeds_the_announced_maximum() {
        for message in every_message() {
            let mut encoded = [0_u8; MAX_ENCODED_LEN];

            let len = encode(&message, 0, &mut encoded).expect("encodage");

            assert!(
                len <= MAX_ENCODED_LEN,
                "{len} octets pour {message:?}, borne annoncee {MAX_ENCODED_LEN}"
            );
        }
    }

    #[test]
    fn an_output_buffer_too_small_is_rejected() {
        let mut encoded = [0_u8; 4];

        assert!(matches!(
            encode(&telemetry(), 0, &mut encoded),
            Err(EncodeError::OutputTooSmall { .. })
        ));
    }

    // --- Rejets -------------------------------------------------------------------

    #[test]
    fn a_non_finite_setpoint_is_refused_at_encoding() {
        let mut encoded = [0_u8; MAX_ENCODED_LEN];

        assert_eq!(
            encode(
                &Message::WheelVelocity {
                    left: f32::NAN,
                    right: 0.0,
                    timestamp_ms: 0
                },
                0,
                &mut encoded
            ),
            Err(EncodeError::NonFiniteVelocity)
        );
    }

    #[test]
    fn a_non_finite_setpoint_is_refused_at_decoding() {
        // Un emetteur fautif pourrait produire une trame parfaitement valide contenant
        // un NaN. La frontiere doit le refuser, pas le transmettre.
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = PROTOCOL_VERSION;
        frame[1] = 0x01;
        frame[2] = 0;
        frame[3..7].copy_from_slice(&f32::NAN.to_le_bytes());
        frame[7..11].copy_from_slice(&0.0_f32.to_le_bytes());
        frame[11..15].copy_from_slice(&0_u32.to_le_bytes());
        let crc = crc16_ccitt(&frame[..15]);
        frame[15..17].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_frame(&frame[..17]),
            Err(DecodeError::NonFiniteVelocity)
        );
    }

    #[test]
    fn a_corrupted_byte_is_caught_by_the_checksum() {
        let mut encoded = [0_u8; MAX_ENCODED_LEN];
        let encoded_len = encode(&telemetry(), 0, &mut encoded).expect("encodage");

        let mut frame = [0_u8; MAX_FRAME_LEN];
        let frame_len = cobs::decode(&encoded[..encoded_len - 1], &mut frame).expect("COBS");
        frame[5] ^= 0x01;

        assert!(matches!(
            decode_frame(&frame[..frame_len]),
            Err(DecodeError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn an_unknown_protocol_version_is_refused_rather_than_guessed() {
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = PROTOCOL_VERSION + 1;
        frame[1] = 0x03;
        frame[2] = 0;
        frame[3..7].copy_from_slice(&1_u32.to_le_bytes());
        let crc = crc16_ccitt(&frame[..7]);
        frame[7..9].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_frame(&frame[..9]),
            Err(DecodeError::UnsupportedVersion {
                found: PROTOCOL_VERSION + 1,
                expected: PROTOCOL_VERSION,
            })
        );
    }

    #[test]
    fn the_checksum_is_verified_before_the_version() {
        // Si le CRC est faux, rien n'est digne de confiance — pas meme l'octet de
        // version. Signaler une version invalide serait trompeur.
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = 0xEE;
        frame[1] = 0x03;
        frame[2] = 0;
        frame[3..7].copy_from_slice(&1_u32.to_le_bytes());
        frame[7..9].copy_from_slice(&0_u16.to_le_bytes());

        assert!(matches!(
            decode_frame(&frame[..9]),
            Err(DecodeError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn an_unknown_message_type_is_reported() {
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = PROTOCOL_VERSION;
        frame[1] = 0x7F;
        frame[2] = 0;
        let crc = crc16_ccitt(&frame[..3]);
        frame[3..5].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_frame(&frame[..5]),
            Err(DecodeError::UnknownMessageType(0x7F))
        );
    }

    #[test]
    fn a_truncated_payload_is_reported_with_both_lengths() {
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = PROTOCOL_VERSION;
        frame[1] = 0x01;
        frame[2] = 0;
        frame[3..7].copy_from_slice(&1.0_f32.to_le_bytes());
        let crc = crc16_ccitt(&frame[..7]);
        frame[7..9].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_frame(&frame[..9]),
            Err(DecodeError::UnexpectedPayloadLength {
                message_type: 0x01,
                expected: 12,
                found: 4,
            })
        );
    }

    #[test]
    fn an_unknown_stop_reason_is_reported() {
        let mut frame = [0_u8; MAX_FRAME_LEN];
        frame[0] = PROTOCOL_VERSION;
        frame[1] = 0x02;
        frame[2] = 0;
        frame[3] = 99;
        frame[4..8].copy_from_slice(&0_u32.to_le_bytes());
        let crc = crc16_ccitt(&frame[..8]);
        frame[8..10].copy_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_frame(&frame[..10]),
            Err(DecodeError::InvalidStopReason(99))
        );
    }

    #[test]
    fn a_frame_too_short_to_hold_a_header_is_rejected() {
        assert!(matches!(
            decode_frame(&[0x01, 0x02]),
            Err(DecodeError::TooShort { .. })
        ));
    }

    // --- Drapeaux -----------------------------------------------------------------

    #[test]
    fn status_flags_report_what_they_carry() {
        let flags = StatusFlags::from_bits(StatusFlags::MOTORS_ENABLED | StatusFlags::OVERCURRENT);

        assert!(flags.contains(StatusFlags::MOTORS_ENABLED));
        assert!(flags.contains(StatusFlags::OVERCURRENT));
        assert!(!flags.contains(StatusFlags::WATCHDOG_EXPIRED));
        assert!(flags.is_faulted());
    }

    #[test]
    fn a_watchdog_expiry_alone_is_not_a_hardware_fault() {
        // Le watchdog qui expire est un fonctionnement nominal de la couche de securite,
        // pas une panne d'electronique. Les confondre brouillerait le diagnostic.
        let flags = StatusFlags::from_bits(StatusFlags::WATCHDOG_EXPIRED);

        assert!(!flags.is_faulted());
    }

    #[test]
    fn stop_reasons_round_trip_through_their_codes() {
        for reason in [
            StopReason::Operator,
            StopReason::SafeStop,
            StopReason::EmergencyStop,
            StopReason::Shutdown,
        ] {
            assert_eq!(StopReason::from_u8(reason.as_u8()), Some(reason));
        }
        assert_eq!(StopReason::from_u8(200), None);
    }
}
