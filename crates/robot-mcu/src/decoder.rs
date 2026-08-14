use crate::cobs;
use crate::message::{DecodeError, MAX_ENCODED_LEN, MAX_FRAME_LEN, Message, decode_frame};

/// Une trame decodee ne peut pas etre plus longue que son encodage : le tampon
/// intermediaire du decodage COBS est donc toujours suffisant.
const _: () = assert!(MAX_FRAME_LEN <= MAX_ENCODED_LEN);

/// Decodeur en flux : on lui donne les octets tels qu'ils arrivent de la liaison.
///
/// Une liaison serie ne livre pas des trames, elle livre des octets. Le decodeur accumule
/// jusqu'au delimiteur `0x00`, puis rend le message ou la raison de son rejet.
///
/// # Resynchronisation
///
/// Apres du bruit, une coupure de cable ou un redemarrage du microcontroleur au milieu
/// d'une trame, le decodeur repart au prochain delimiteur. C'est toute la raison d'etre de
/// COBS : aucun octet nul ne peut apparaitre dans une trame valide, donc un `0x00` est
/// toujours une fin de trame et jamais une donnee.
#[derive(Debug, Clone)]
pub struct Decoder {
    buffer: [u8; MAX_ENCODED_LEN],
    len: usize,
    /// Une trame plus longue que le protocole ne l'autorise est arrivee.
    ///
    /// On memorise le debordement plutot que d'abandonner en silence : sans cela, la
    /// trame tronquee serait decodee comme si de rien n'etait, et livrerait des donnees
    /// fausses ayant l'air valides.
    overflowed: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// Cree un decodeur vide.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0; MAX_ENCODED_LEN],
            len: 0,
            overflowed: false,
        }
    }

    /// Vide le decodeur et repart d'une trame neuve.
    pub const fn reset(&mut self) {
        self.len = 0;
        self.overflowed = false;
    }

    /// Nombre d'octets accumules dans la trame en cours.
    #[must_use]
    pub const fn pending(&self) -> usize {
        self.len
    }

    /// Consomme un octet.
    ///
    /// Rend `None` tant que la trame n'est pas complete, puis le message decode ou la
    /// raison de son rejet.
    pub fn push(&mut self, byte: u8) -> Option<Result<(u8, Message), DecodeError>> {
        if byte != 0 {
            if self.len < self.buffer.len() {
                self.buffer[self.len] = byte;
                self.len += 1;
            } else {
                self.overflowed = true;
            }

            return None;
        }

        // Delimiteur. Deux delimiteurs de suite, ou un delimiteur en tete de flux, ne
        // delimitent rien : on ne signale pas une trame vide comme une erreur.
        if self.len == 0 && !self.overflowed {
            return None;
        }

        let result = if self.overflowed {
            Err(DecodeError::TooLong {
                len: self.buffer.len() + 1,
            })
        } else {
            let mut frame = [0_u8; MAX_FRAME_LEN];
            cobs::decode(&self.buffer[..self.len], &mut frame)
                .map_err(DecodeError::from)
                .and_then(|len| decode_frame(&frame[..len]))
        };

        self.reset();

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MAX_ENCODED_LEN as ENCODED, StopReason, encode};

    fn sample() -> Message {
        Message::WheelVelocity {
            left: 2.5,
            right: -2.5,
            timestamp_ms: 1_234,
        }
    }

    /// Encode un message et rend les octets tels qu'ils circuleraient sur la liaison.
    fn wire(message: &Message, sequence: u8) -> ([u8; ENCODED], usize) {
        let mut buffer = [0_u8; ENCODED];
        let len = encode(message, sequence, &mut buffer).expect("encodage");
        (buffer, len)
    }

    /// Pousse tous les octets et rend le dernier resultat produit.
    fn feed(decoder: &mut Decoder, bytes: &[u8]) -> Option<Result<(u8, Message), DecodeError>> {
        let mut last = None;
        for &byte in bytes {
            if let Some(result) = decoder.push(byte) {
                last = Some(result);
            }
        }
        last
    }

    #[test]
    fn a_complete_frame_yields_its_message() {
        let mut decoder = Decoder::new();
        let (bytes, len) = wire(&sample(), 7);

        let result = feed(&mut decoder, &bytes[..len]).expect("une trame complete");

        assert_eq!(result, Ok((7, sample())));
    }

    #[test]
    fn nothing_is_produced_before_the_delimiter() {
        let mut decoder = Decoder::new();
        let (bytes, len) = wire(&sample(), 0);

        // Tous les octets sauf le delimiteur final.
        for &byte in &bytes[..len - 1] {
            assert_eq!(decoder.push(byte), None, "trame rendue trop tot");
        }

        assert!(decoder.pending() > 0);
        assert!(decoder.push(0).is_some());
    }

    #[test]
    fn consecutive_frames_are_decoded_independently() {
        let mut decoder = Decoder::new();
        let first = sample();
        let second = Message::Heartbeat { timestamp_ms: 99 };

        let (bytes, len) = wire(&first, 1);
        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((1, first))));

        let (bytes, len) = wire(&second, 2);
        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((2, second))));
    }

    #[test]
    fn leading_delimiters_are_ignored() {
        let mut decoder = Decoder::new();

        assert_eq!(decoder.push(0), None);
        assert_eq!(decoder.push(0), None);

        let (bytes, len) = wire(&sample(), 3);
        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((3, sample()))));
    }

    #[test]
    fn the_decoder_resynchronises_after_noise() {
        let mut decoder = Decoder::new();

        // Du bruit, puis un delimiteur : la trame parasite est rejetee.
        let noise = feed(&mut decoder, &[0x42, 0x13, 0x37, 0x00]);
        assert!(matches!(noise, Some(Err(_))), "le bruit doit etre rejete");

        // La trame suivante passe.
        let (bytes, len) = wire(&sample(), 4);
        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((4, sample()))));
    }

    #[test]
    fn a_frame_cut_in_half_is_rejected_then_the_next_one_passes() {
        let mut decoder = Decoder::new();
        let (bytes, len) = wire(&sample(), 5);

        // Moitie de trame, puis un delimiteur : le microcontroleur a redemarre.
        let truncated = feed(&mut decoder, &bytes[..len / 2]);
        assert_eq!(truncated, None);
        assert!(matches!(decoder.push(0), Some(Err(_))));

        let (bytes, len) = wire(&sample(), 6);
        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((6, sample()))));
    }

    #[test]
    fn an_oversized_frame_is_reported_rather_than_silently_truncated() {
        let mut decoder = Decoder::new();

        for _ in 0..(MAX_ENCODED_LEN * 3) {
            let _ = decoder.push(0xAA);
        }

        assert!(matches!(
            decoder.push(0),
            Some(Err(DecodeError::TooLong { .. }))
        ));
    }

    #[test]
    fn the_decoder_recovers_after_an_oversized_frame() {
        let mut decoder = Decoder::new();
        for _ in 0..(MAX_ENCODED_LEN * 2) {
            let _ = decoder.push(0xAA);
        }
        let _ = decoder.push(0);

        let (bytes, len) = wire(&sample(), 8);

        assert_eq!(feed(&mut decoder, &bytes[..len]), Some(Ok((8, sample()))));
    }

    #[test]
    fn a_reset_discards_the_frame_in_progress() {
        let mut decoder = Decoder::new();
        let (bytes, len) = wire(&sample(), 9);
        let _ = feed(&mut decoder, &bytes[..len - 1]);

        decoder.reset();

        assert_eq!(decoder.pending(), 0);
        assert_eq!(decoder.push(0), None, "rien ne doit subsister");
    }

    #[test]
    fn a_stream_of_many_frames_loses_nothing() {
        let mut decoder = Decoder::new();
        let mut decoded = 0_usize;

        for sequence in 0..=255_u8 {
            let message = Message::Stop {
                reason: StopReason::SafeStop,
                timestamp_ms: u32::from(sequence),
            };
            let (bytes, len) = wire(&message, sequence);

            for &byte in &bytes[..len] {
                if let Some(result) = decoder.push(byte) {
                    assert_eq!(result, Ok((sequence, message)));
                    decoded += 1;
                }
            }
        }

        assert_eq!(decoded, 256, "des trames ont ete perdues");
    }
}
