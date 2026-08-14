//! Encodage COBS — *Consistent Overhead Byte Stuffing*.
//!
//! Supprime tout octet nul du contenu encode, ce qui rend le `0x00` utilisable comme
//! delimiteur de trame sans la moindre ambiguite. Apres du bruit sur la ligne, le
//! recepteur se resynchronise au prochain `0x00` sans risquer de prendre un octet de
//! donnees pour un debut de trame.
//!
//! Le surcout est d'un octet, plus un par tranche entamee de 254 octets — contre jusqu'au
//! double pour un echappement classique.

use core::fmt;

/// Echec d'encodage ou de decodage COBS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobsError {
    /// Le tampon de sortie est trop petit.
    OutputTooSmall {
        /// Taille disponible.
        available: usize,
        /// Taille necessaire.
        required: usize,
    },
    /// Un octet nul a ete rencontre dans une trame encodee : elle est corrompue.
    UnexpectedZero,
    /// La trame s'acheve au milieu d'un groupe annonce.
    Truncated,
}

impl fmt::Display for CobsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputTooSmall {
                available,
                required,
            } => write!(
                f,
                "tampon de sortie trop petit : {available} octets pour {required} requis"
            ),
            Self::UnexpectedZero => f.write_str("octet nul dans une trame encodee"),
            Self::Truncated => f.write_str("trame COBS tronquee"),
        }
    }
}

impl core::error::Error for CobsError {}

/// Taille maximale d'un encodage COBS pour `len` octets de donnees.
#[must_use]
pub const fn max_encoded_len(len: usize) -> usize {
    len + len / 254 + 1
}

/// Encode `input` dans `output`, sans le delimiteur final.
///
/// # Errors
///
/// Renvoie [`CobsError::OutputTooSmall`] si `output` ne peut pas contenir le resultat.
pub fn encode(input: &[u8], output: &mut [u8]) -> Result<usize, CobsError> {
    let required = max_encoded_len(input.len());
    if output.len() < required {
        return Err(CobsError::OutputTooSmall {
            available: output.len(),
            required,
        });
    }

    // `code_index` reserve la place du compteur du groupe courant : on ne connait sa
    // valeur qu'une fois le groupe termine.
    let mut code_index = 0_usize;
    let mut write = 1_usize;
    let mut code = 1_u8;

    for &byte in input {
        if byte == 0 {
            output[code_index] = code;
            code_index = write;
            write += 1;
            code = 1;
        } else {
            output[write] = byte;
            write += 1;
            code += 1;

            // Un groupe ne peut pas depasser 254 octets utiles : on le ferme et on en
            // ouvre un nouveau.
            if code == 0xFF {
                output[code_index] = code;
                code_index = write;
                write += 1;
                code = 1;
            }
        }
    }

    output[code_index] = code;

    Ok(write)
}

/// Decode `input` — une trame encodee, delimiteur exclu — dans `output`.
///
/// # Errors
///
/// Renvoie [`CobsError`] si la trame est corrompue, tronquee, ou si `output` est trop
/// petit.
pub fn decode(input: &[u8], output: &mut [u8]) -> Result<usize, CobsError> {
    // Un octet nul ne peut pas apparaitre dans une trame COBS valide : c'est toute la
    // propriete sur laquelle repose le delimiteur. En trouver un signale une trame
    // corrompue, et le signaler vaut mieux que decoder des donnees fausses.
    if input.contains(&0) {
        return Err(CobsError::UnexpectedZero);
    }

    let mut read = 0_usize;
    let mut write = 0_usize;

    while read < input.len() {
        let group = usize::from(input[read]) - 1;
        read += 1;

        if read + group > input.len() {
            return Err(CobsError::Truncated);
        }

        // Le zero implicite qui suit un groupe complet compte dans la place requise.
        let trailing_zero = group != 0xFE && read + group < input.len();
        let required = write + group + usize::from(trailing_zero);
        if required > output.len() {
            return Err(CobsError::OutputTooSmall {
                available: output.len(),
                required,
            });
        }

        output[write..write + group].copy_from_slice(&input[read..read + group]);
        write += group;
        read += group;

        if trailing_zero {
            output[write] = 0;
            write += 1;
        }
    }

    Ok(write)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode puis decode, et verifie qu'on retrouve l'original.
    fn round_trip(data: &[u8]) {
        let mut encoded = [0_u8; 1024];
        let mut decoded = [0_u8; 1024];

        let encoded_len = encode(data, &mut encoded).expect("encodage");
        assert!(
            !encoded[..encoded_len].contains(&0),
            "l'encodage laisse un octet nul, le delimiteur devient ambigu"
        );

        let decoded_len = decode(&encoded[..encoded_len], &mut decoded).expect("decodage");
        assert_eq!(&decoded[..decoded_len], data, "aller-retour infidele");
    }

    #[test]
    fn an_empty_input_round_trips() {
        round_trip(&[]);
    }

    #[test]
    fn data_without_zeros_round_trips() {
        round_trip(&[0x11, 0x22, 0x33, 0x44]);
    }

    #[test]
    fn a_single_zero_round_trips() {
        round_trip(&[0x00]);
    }

    #[test]
    fn leading_and_trailing_zeros_round_trip() {
        round_trip(&[0x00, 0x11, 0x00, 0x22, 0x00]);
    }

    #[test]
    fn only_zeros_round_trip() {
        round_trip(&[0x00; 16]);
    }

    #[test]
    fn a_run_longer_than_the_group_limit_round_trips() {
        // 254 octets non nuls forcent la fermeture d'un groupe : c'est le cas limite.
        round_trip(&[0xAA; 254]);
        round_trip(&[0xAA; 255]);
        round_trip(&[0xAA; 600]);
    }

    #[test]
    fn a_realistic_frame_round_trips() {
        // Une trame de teleoperation typique, avec les zeros que produisent des
        // flottants et des entiers de faible valeur.
        round_trip(&[
            0x01, 0x01, 0x2A, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0xE8, 0x03, 0x00,
            0x00, 0x12, 0x34,
        ]);
    }

    #[test]
    fn the_reference_encoding_matches() {
        // Vecteur classique de la specification COBS.
        let mut encoded = [0_u8; 16];
        let len = encode(&[0x11, 0x22, 0x00, 0x33], &mut encoded).expect("encodage");

        assert_eq!(&encoded[..len], &[0x03, 0x11, 0x22, 0x02, 0x33]);
    }

    #[test]
    fn an_output_buffer_too_small_is_rejected() {
        let mut encoded = [0_u8; 2];

        assert!(matches!(
            encode(&[0x11, 0x22, 0x33], &mut encoded),
            Err(CobsError::OutputTooSmall { .. })
        ));
    }

    #[test]
    fn a_zero_inside_an_encoded_frame_is_rejected() {
        let mut decoded = [0_u8; 16];

        assert_eq!(
            decode(&[0x03, 0x11, 0x00, 0x02], &mut decoded),
            Err(CobsError::UnexpectedZero)
        );
    }

    #[test]
    fn a_truncated_frame_is_rejected() {
        let mut decoded = [0_u8; 16];

        // Annonce un groupe de 5 octets, n'en fournit que 2.
        assert_eq!(
            decode(&[0x06, 0x11, 0x22], &mut decoded),
            Err(CobsError::Truncated)
        );
    }

    #[test]
    fn the_announced_maximum_length_is_never_exceeded() {
        for length in [0_usize, 1, 253, 254, 255, 508, 509, 1000] {
            let data = [0xAA_u8; 1000];
            let mut encoded = [0_u8; 1024];

            let encoded_len = encode(&data[..length], &mut encoded).expect("encodage");

            assert!(
                encoded_len <= max_encoded_len(length),
                "{length} octets encodes en {encoded_len}, borne annoncee {}",
                max_encoded_len(length)
            );
        }
    }
}
