//! Controle d'integrite des trames.
//!
//! COBS restitue les limites de trame, pas l'integrite du contenu : il faut les deux.

/// Calcule le CRC-16/CCITT-FALSE d'une suite d'octets.
///
/// Polynome `0x1021`, valeur initiale `0xFFFF`, sans reflexion ni ou-exclusif final.
///
/// Ce CRC detecte toutes les erreurs simples et doubles, ainsi que toutes les rafales
/// jusqu'a 16 bits — largement suffisant pour vingt centimetres de cable USB.
///
/// L'implementation est directe, sans table : le firmware n'a pas de memoire a gaspiller
/// pour 512 octets de table sur des trames de quelques dizaines d'octets.
#[must_use]
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;

    for &byte in data {
        crc ^= u16::from(byte) << 8;

        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }

    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_vector_matches() {
        // Vecteur de reference universel du CRC-16/CCITT-FALSE.
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn an_empty_input_yields_the_initial_value() {
        assert_eq!(crc16_ccitt(&[]), 0xFFFF);
    }

    #[test]
    fn a_single_flipped_bit_changes_the_result() {
        let original = crc16_ccitt(&[0x01, 0x02, 0x03, 0x04]);
        let flipped = crc16_ccitt(&[0x01, 0x02, 0x03, 0x05]);

        assert_ne!(original, flipped);
    }

    #[test]
    fn swapping_two_bytes_changes_the_result() {
        // Une simple somme ne verrait pas la difference. Le CRC, si.
        assert_ne!(crc16_ccitt(&[0xAA, 0xBB]), crc16_ccitt(&[0xBB, 0xAA]));
    }

    #[test]
    fn leading_zeros_are_not_ignored() {
        assert_ne!(crc16_ccitt(&[0x00, 0x2A]), crc16_ccitt(&[0x2A]));
    }
}
