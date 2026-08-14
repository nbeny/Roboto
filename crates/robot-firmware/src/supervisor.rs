use robot_mcu::{Message, StatusFlags};

/// Consignes de vitesse des deux roues, en rad/s.
///
/// Volontairement en `f32` et distinct de `robot_hal::WheelSpeeds`, qui est en `f64` :
/// ce sont les deux cotes de la liaison. Le calculateur calcule en double precision, le
/// microcontroleur recoit et asservit en simple. Confondre les deux types masquerait la
/// perte de precision au passage du fil.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WheelSetpoint {
    /// Roue gauche, en rad/s.
    pub left: f32,
    /// Roue droite, en rad/s.
    pub right: f32,
}

impl WheelSetpoint {
    /// Les deux roues a l'arret.
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
    };

    /// Construit un couple de consignes.
    #[must_use]
    pub const fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }
}

/// Supervision du microcontroleur : consigne courante, watchdog, drapeaux d'etat.
///
/// C'est ici que reside la seule decision autonome du microcontroleur : **couper les
/// moteurs quand plus personne ne lui parle**. Ce watchdog est independant de celui du
/// coeur, et c'est tout l'interet. Si le calculateur se fige, plante, ou si le cable USB
/// se debranche, aucun logiciel en amont n'est en mesure de demander l'arret.
///
/// ```text
/// arret d'urgence materiel     coupe la puissance          instantane
/// watchdog microcontroleur     coupe les moteurs           200 ms   <-- ici
/// watchdog du coeur            passage en SAFE_STOP        500 ms
/// ```
#[derive(Debug, Clone)]
pub struct Supervisor {
    setpoint: WheelSetpoint,
    last_activity_ms: Option<u32>,
    timeout_ms: u32,
    flags: StatusFlags,
    /// Un `Stop` recu verrouille l'arret jusqu'a la prochaine consigne.
    stopped: bool,
}

impl Supervisor {
    /// Delai de watchdog par defaut, en millisecondes.
    ///
    /// Plus court que le timeout de commande du coeur, qui vaut 500 ms. La hierarchie
    /// est voulue : chaque etage reagit avant celui du dessus, donc chaque etage protege
    /// contre la defaillance de son superieur.
    pub const DEFAULT_TIMEOUT_MS: u32 = 200;

    /// Cree une supervision, moteurs coupes.
    #[must_use]
    pub const fn new(timeout_ms: u32) -> Self {
        Self {
            setpoint: WheelSetpoint::ZERO,
            last_activity_ms: None,
            timeout_ms,
            flags: StatusFlags::from_bits(0),
            stopped: true,
        }
    }

    /// Consigne courante, telle qu'elle doit etre appliquee aux asservissements.
    #[must_use]
    pub const fn setpoint(&self) -> WheelSetpoint {
        self.setpoint
    }

    /// Drapeaux d'etat a publier en telemetrie.
    #[must_use]
    pub const fn flags(&self) -> StatusFlags {
        self.flags
    }

    /// Indique si l'etage de puissance doit etre actif.
    #[must_use]
    pub const fn motors_enabled(&self) -> bool {
        self.flags.contains(StatusFlags::MOTORS_ENABLED)
    }

    /// Traite un message recu du calculateur.
    ///
    /// Les messages emis par le microcontroleur lui-meme sont ignores : les recevoir
    /// signalerait un cablage errone, pas une commande.
    pub fn handle(&mut self, message: &Message) {
        match *message {
            Message::WheelVelocity {
                left,
                right,
                timestamp_ms,
            } => {
                self.last_activity_ms = Some(timestamp_ms);

                // Un defaut materiel prime sur toute consigne : on note l'activite, mais
                // on ne relance pas les moteurs.
                if self.flags.is_faulted() {
                    return;
                }

                self.setpoint = WheelSetpoint::new(left, right);
                self.stopped = false;
                self.flags = StatusFlags::from_bits(
                    (self.flags.bits() & !StatusFlags::WATCHDOG_EXPIRED)
                        | StatusFlags::MOTORS_ENABLED,
                );
            }
            Message::Stop { timestamp_ms, .. } => {
                self.last_activity_ms = Some(timestamp_ms);
                self.halt();
            }
            Message::Heartbeat { timestamp_ms } => {
                // Reame le watchdog sans toucher a la consigne : le robot peut rester a
                // l'arret sans que le microcontroleur conclue a une perte de liaison.
                self.last_activity_ms = Some(timestamp_ms);
            }
            Message::Telemetry(_) | Message::Fault { .. } => {}
        }
    }

    /// Fait avancer l'horloge et applique le watchdog.
    ///
    /// Renvoie la consigne a appliquer : nulle des que le watchdog a expire.
    pub fn tick(&mut self, now_ms: u32) -> WheelSetpoint {
        let expired = self
            .last_activity_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= self.timeout_ms);

        if expired {
            self.halt();
            self.flags = StatusFlags::from_bits(self.flags.bits() | StatusFlags::WATCHDOG_EXPIRED);
        }

        self.setpoint
    }

    /// Signale un defaut materiel : les moteurs sont coupes et le restent.
    ///
    /// La levee n'est pas prevue en version 1 du protocole : un defaut de pont en H ou
    /// une surintensite demandent qu'un humain regarde le robot avant qu'il ne reparte.
    pub fn raise_fault(&mut self, flag: u8) {
        self.flags = StatusFlags::from_bits(self.flags.bits() | flag);
        self.halt();
    }

    fn halt(&mut self) {
        self.setpoint = WheelSetpoint::ZERO;
        self.stopped = true;
        self.flags = StatusFlags::from_bits(self.flags.bits() & !StatusFlags::MOTORS_ENABLED);
    }

    /// Indique si l'arret est verrouille.
    #[must_use]
    pub const fn is_stopped(&self) -> bool {
        self.stopped
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new(Self::DEFAULT_TIMEOUT_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use robot_mcu::StopReason;

    fn command(left: f32, right: f32, at: u32) -> Message {
        Message::WheelVelocity {
            left,
            right,
            timestamp_ms: at,
        }
    }

    // --- Etat initial ---------------------------------------------------------------

    #[test]
    fn a_fresh_supervisor_keeps_the_motors_off() {
        let supervisor = Supervisor::default();

        assert_eq!(supervisor.setpoint(), WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
        assert!(supervisor.is_stopped());
    }

    #[test]
    fn a_supervisor_that_never_heard_anything_stays_off() {
        let mut supervisor = Supervisor::default();

        assert_eq!(supervisor.tick(10_000), WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
    }

    // --- Commande -------------------------------------------------------------------

    #[test]
    fn a_setpoint_is_applied_and_enables_the_motors() {
        let mut supervisor = Supervisor::default();

        supervisor.handle(&command(3.0, -3.0, 1_000));

        assert_eq!(supervisor.setpoint(), WheelSetpoint::new(3.0, -3.0));
        assert!(supervisor.motors_enabled());
        assert!(!supervisor.is_stopped());
    }

    #[test]
    fn a_stop_cuts_the_motors_and_latches() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(3.0, 3.0, 1_000));

        supervisor.handle(&Message::Stop {
            reason: StopReason::EmergencyStop,
            timestamp_ms: 1_010,
        });

        assert_eq!(supervisor.setpoint(), WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
        assert!(supervisor.is_stopped());
    }

    #[test]
    fn a_new_setpoint_releases_a_latched_stop() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&Message::Stop {
            reason: StopReason::Operator,
            timestamp_ms: 1_000,
        });

        supervisor.handle(&command(2.0, 2.0, 1_010));

        assert_eq!(supervisor.setpoint(), WheelSetpoint::new(2.0, 2.0));
        assert!(supervisor.motors_enabled());
    }

    #[test]
    fn the_microcontroller_ignores_its_own_messages() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(2.0, 2.0, 1_000));

        supervisor.handle(&Message::Fault {
            code: 3,
            timestamp_ms: 1_010,
        });

        assert_eq!(
            supervisor.setpoint(),
            WheelSetpoint::new(2.0, 2.0),
            "un message entrant du mauvais sens ne doit rien changer"
        );
    }

    // --- Watchdog -------------------------------------------------------------------

    #[test]
    fn the_setpoint_survives_until_the_timeout() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 1_000));

        let held = supervisor.tick(1_000 + Supervisor::DEFAULT_TIMEOUT_MS - 1);

        assert_eq!(held, WheelSetpoint::new(4.0, 4.0));
        assert!(!supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED));
    }

    #[test]
    fn the_watchdog_cuts_the_motors_at_the_timeout() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 1_000));

        let cut = supervisor.tick(1_000 + Supervisor::DEFAULT_TIMEOUT_MS);

        assert_eq!(cut, WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
        assert!(supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED));
    }

    #[test]
    fn a_heartbeat_holds_the_watchdog_without_moving_the_robot() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 1_000));

        supervisor.handle(&Message::Heartbeat {
            timestamp_ms: 1_150,
        });
        let held = supervisor.tick(1_300);

        assert_eq!(held, WheelSetpoint::new(4.0, 4.0), "consigne inchangee");
        assert!(!supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED));
    }

    #[test]
    fn a_steady_command_stream_never_trips_the_watchdog() {
        let mut supervisor = Supervisor::default();

        for step in 0..200_u32 {
            let now = 1_000 + step * 20;
            supervisor.handle(&command(4.0, 4.0, now));
            supervisor.tick(now);

            assert!(
                supervisor.motors_enabled(),
                "coupure inattendue a l'etape {step}"
            );
        }
    }

    #[test]
    fn a_command_after_an_expiry_restarts_the_motors() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 1_000));
        supervisor.tick(1_500);
        assert!(supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED));

        supervisor.handle(&command(2.0, 2.0, 1_600));

        assert!(supervisor.motors_enabled());
        assert!(
            !supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED),
            "le drapeau doit retomber une fois la liaison retablie"
        );
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_trip_the_watchdog() {
        // `saturating_sub` rend zero : un horodatage plus ancien que le dernier connu ne
        // doit pas se lire comme un delai enorme.
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 5_000));

        let held = supervisor.tick(4_000);

        assert_eq!(held, WheelSetpoint::new(4.0, 4.0));
    }

    // --- Defauts materiels ----------------------------------------------------------

    #[test]
    fn a_hardware_fault_cuts_the_motors() {
        let mut supervisor = Supervisor::default();
        supervisor.handle(&command(4.0, 4.0, 1_000));

        supervisor.raise_fault(StatusFlags::OVERCURRENT);

        assert_eq!(supervisor.setpoint(), WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
        assert!(supervisor.flags().is_faulted());
    }

    #[test]
    fn a_hardware_fault_outranks_any_further_command() {
        // Un calculateur qui insiste ne doit pas pouvoir relancer un pont en H en defaut.
        let mut supervisor = Supervisor::default();
        supervisor.raise_fault(StatusFlags::DRIVER_FAULT);

        supervisor.handle(&command(5.0, 5.0, 2_000));

        assert_eq!(supervisor.setpoint(), WheelSetpoint::ZERO);
        assert!(!supervisor.motors_enabled());
    }

    #[test]
    fn a_command_during_a_fault_still_feeds_the_watchdog() {
        // La liaison est vivante, seul le materiel est en defaut : les deux diagnostics
        // doivent rester distincts.
        let mut supervisor = Supervisor::default();
        supervisor.raise_fault(StatusFlags::DRIVER_FAULT);

        supervisor.handle(&command(5.0, 5.0, 2_000));
        supervisor.tick(2_050);

        assert!(
            !supervisor.flags().contains(StatusFlags::WATCHDOG_EXPIRED),
            "la liaison est vivante, le watchdog n'a pas lieu d'expirer"
        );
    }
}
