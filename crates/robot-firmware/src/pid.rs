use core::fmt;

/// Gains d'un asservissement proportionnel-integral-derive.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PidGains {
    /// Gain proportionnel.
    pub kp: f32,
    /// Gain integral, par seconde.
    pub ki: f32,
    /// Gain derive, en secondes.
    pub kd: f32,
}

/// Gains inexploitables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainsError {
    /// Un gain n'est pas fini.
    NonFinite,
    /// Un gain est negatif : le signe inverserait la contre-reaction.
    Negative,
    /// La borne de sortie n'est pas finie et strictement positive.
    InvalidOutputLimit,
}

impl fmt::Display for GainsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => f.write_str("gain non fini"),
            Self::Negative => f.write_str("gain negatif"),
            Self::InvalidOutputLimit => f.write_str("borne de sortie invalide"),
        }
    }
}

impl core::error::Error for GainsError {}

/// Asservissement de vitesse d'une roue.
///
/// Rend une commande normalisee dans `[-limite, +limite]`, destinee au rapport cyclique
/// du pont en H.
///
/// # Anti-emballement
///
/// Le terme integral n'est accumule que lorsque la sortie n'est pas saturee dans le meme
/// sens que l'erreur. Sans cela, une roue bloquee ferait croitre l'integrale
/// indefiniment, et le robot partirait a pleine vitesse a l'instant ou elle se libere —
/// des secondes apres que la consigne a change.
#[derive(Debug, Clone)]
pub struct VelocityPid {
    gains: PidGains,
    output_limit: f32,
    integral: f32,
    previous_error: f32,
    has_previous: bool,
}

impl VelocityPid {
    /// Construit un asservissement.
    ///
    /// # Errors
    ///
    /// Renvoie [`GainsError`] si un gain n'est pas fini ou est negatif, ou si la borne de
    /// sortie n'est pas finie et strictement positive.
    pub fn new(gains: PidGains, output_limit: f32) -> Result<Self, GainsError> {
        for gain in [gains.kp, gains.ki, gains.kd] {
            if !gain.is_finite() {
                return Err(GainsError::NonFinite);
            }
            if gain < 0.0 {
                return Err(GainsError::Negative);
            }
        }

        if !output_limit.is_finite() || output_limit <= 0.0 {
            return Err(GainsError::InvalidOutputLimit);
        }

        Ok(Self {
            gains,
            output_limit,
            integral: 0.0,
            previous_error: 0.0,
            has_previous: false,
        })
    }

    /// Gains en vigueur.
    #[must_use]
    pub const fn gains(&self) -> PidGains {
        self.gains
    }

    /// Terme integral accumule.
    #[must_use]
    pub const fn integral(&self) -> f32 {
        self.integral
    }

    /// Efface l'etat accumule.
    ///
    /// A appeler des que les moteurs sont coupes : reprendre avec l'integrale d'avant
    /// l'arret ferait bondir la roue au reveil.
    pub const fn reset(&mut self) {
        self.integral = 0.0;
        self.previous_error = 0.0;
        self.has_previous = false;
    }

    /// Calcule la commande pour un pas de temps de `dt_s` secondes.
    ///
    /// Une entree non finie ou un pas de temps nul remettent l'asservissement a zero et
    /// rendent une commande nulle : dans les deux cas, la boucle n'a plus de sens et
    /// continuer sur des valeurs douteuses serait pire que s'arreter.
    pub fn update(&mut self, setpoint: f32, measured: f32, dt_s: f32) -> f32 {
        if !setpoint.is_finite() || !measured.is_finite() || !dt_s.is_finite() || dt_s <= 0.0 {
            self.reset();
            return 0.0;
        }

        let error = setpoint - measured;

        let derivative = if self.has_previous {
            (error - self.previous_error) / dt_s
        } else {
            // Pas de derivee au premier cycle : la difference avec un passe inexistant
            // produirait un a-coup proportionnel a la consigne.
            0.0
        };

        let candidate_integral = self.integral + error * dt_s;
        let unsaturated =
            self.gains.kp * error + self.gains.ki * candidate_integral + self.gains.kd * derivative;

        let output = unsaturated.clamp(-self.output_limit, self.output_limit);

        // On n'accumule que si la sortie n'est pas saturee dans le sens de l'erreur.
        let saturated_against_error = (unsaturated > self.output_limit && error > 0.0)
            || (unsaturated < -self.output_limit && error < 0.0);
        if !saturated_against_error {
            self.integral = candidate_integral;
        }

        self.previous_error = error;
        self.has_previous = true;

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f32 = 1e-5;

    fn proportional_only() -> VelocityPid {
        VelocityPid::new(
            PidGains {
                kp: 0.1,
                ki: 0.0,
                kd: 0.0,
            },
            1.0,
        )
        .expect("gains valides")
    }

    // --- Construction ---------------------------------------------------------------

    #[test]
    fn negative_gains_are_refused() {
        assert_eq!(
            VelocityPid::new(
                PidGains {
                    kp: -0.1,
                    ki: 0.0,
                    kd: 0.0
                },
                1.0
            )
            .err(),
            Some(GainsError::Negative)
        );
    }

    #[test]
    fn non_finite_gains_are_refused() {
        assert_eq!(
            VelocityPid::new(
                PidGains {
                    kp: f32::NAN,
                    ki: 0.0,
                    kd: 0.0
                },
                1.0
            )
            .err(),
            Some(GainsError::NonFinite)
        );
    }

    #[test]
    fn an_invalid_output_limit_is_refused() {
        let gains = PidGains::default();

        assert_eq!(
            VelocityPid::new(gains, 0.0).err(),
            Some(GainsError::InvalidOutputLimit)
        );
        assert_eq!(
            VelocityPid::new(gains, f32::INFINITY).err(),
            Some(GainsError::InvalidOutputLimit)
        );
    }

    // --- Reponse --------------------------------------------------------------------

    #[test]
    fn no_error_produces_no_command() {
        let mut pid = proportional_only();

        assert!(pid.update(5.0, 5.0, 0.02).abs() < TOLERANCE);
    }

    #[test]
    fn the_command_pushes_towards_the_setpoint() {
        let mut pid = proportional_only();

        assert!(pid.update(5.0, 0.0, 0.02) > 0.0, "trop lent : accelerer");

        let mut pid = proportional_only();
        assert!(pid.update(-5.0, 0.0, 0.02) < 0.0, "consigne arriere");

        let mut pid = proportional_only();
        assert!(pid.update(0.0, 5.0, 0.02) < 0.0, "trop vite : freiner");
    }

    #[test]
    fn the_command_is_clamped_to_the_output_limit() {
        let mut pid = proportional_only();

        // Une erreur de 1000 rad/s avec kp = 0,1 donnerait 100.
        let output = pid.update(1_000.0, 0.0, 0.02);

        assert!((output - 1.0).abs() < TOLERANCE, "sortie {output}");
    }

    #[test]
    fn the_integral_term_removes_a_steady_state_error() {
        // Un proportionnel seul laisse toujours un ecart residuel. L'integrale le comble.
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 0.0,
                ki: 1.0,
                kd: 0.0,
            },
            1.0,
        )
        .expect("gains valides");

        let first = pid.update(1.0, 0.0, 0.1);
        let second = pid.update(1.0, 0.0, 0.1);

        assert!(
            second > first,
            "l'integrale doit croitre : {first} {second}"
        );
    }

    #[test]
    fn the_derivative_term_is_silent_on_the_first_cycle() {
        // Sans passe, une derivee calculee produirait un a-coup proportionnel a la
        // consigne — exactement au moment ou le robot demarre.
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 0.0,
                ki: 0.0,
                kd: 10.0,
            },
            1.0,
        )
        .expect("gains valides");

        assert!(pid.update(5.0, 0.0, 0.02).abs() < TOLERANCE);
    }

    #[test]
    fn the_derivative_term_opposes_a_rising_error() {
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 0.0,
                ki: 0.0,
                kd: 0.01,
            },
            1.0,
        )
        .expect("gains valides");

        let _ = pid.update(1.0, 0.0, 0.02);
        let rising = pid.update(2.0, 0.0, 0.02);

        assert!(rising > 0.0, "une erreur croissante appelle une correction");
    }

    // --- Anti-emballement -----------------------------------------------------------

    #[test]
    fn the_integral_does_not_wind_up_while_saturated() {
        // Cas concret : roue bloquee. Sans anti-emballement, l'integrale croit sans fin
        // et le robot part a pleine vitesse quand la roue se libere.
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 1.0,
                ki: 10.0,
                kd: 0.0,
            },
            1.0,
        )
        .expect("gains valides");

        for _ in 0..100 {
            let _ = pid.update(50.0, 0.0, 0.02);
        }
        let wound_up = pid.integral();

        for _ in 0..100 {
            let _ = pid.update(50.0, 0.0, 0.02);
        }

        assert!(
            (pid.integral() - wound_up).abs() < TOLERANCE,
            "l'integrale continue de croitre en saturation : {wound_up} puis {}",
            pid.integral()
        );
    }

    #[test]
    fn the_integral_unwinds_once_the_error_reverses() {
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 0.0,
                ki: 1.0,
                kd: 0.0,
            },
            1.0,
        )
        .expect("gains valides");

        for _ in 0..50 {
            let _ = pid.update(50.0, 0.0, 0.02);
        }
        let saturated = pid.integral();

        // La roue se libere et depasse la consigne : l'integrale doit redescendre.
        let _ = pid.update(0.0, 50.0, 0.02);

        assert!(pid.integral() < saturated, "l'integrale doit se resorber");
    }

    // --- Robustesse -----------------------------------------------------------------

    #[test]
    fn a_non_finite_input_stops_the_loop_instead_of_propagating() {
        let mut pid = proportional_only();
        let _ = pid.update(1.0, 0.0, 0.02);

        assert_eq!(pid.update(f32::NAN, 0.0, 0.02), 0.0);
        assert_eq!(pid.integral(), 0.0, "l'etat doit etre efface");
    }

    #[test]
    fn a_null_time_step_stops_the_loop() {
        let mut pid = proportional_only();

        assert_eq!(pid.update(5.0, 0.0, 0.0), 0.0);
        assert_eq!(pid.update(5.0, 0.0, -0.02), 0.0);
    }

    #[test]
    fn a_reset_clears_everything() {
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 0.0,
                ki: 1.0,
                kd: 0.0,
            },
            1.0,
        )
        .expect("gains valides");
        for _ in 0..10 {
            let _ = pid.update(1.0, 0.0, 0.02);
        }
        assert!(pid.integral() > 0.0);

        pid.reset();

        assert_eq!(pid.integral(), 0.0);
    }

    #[test]
    fn the_output_never_leaves_its_bounds() {
        let mut pid = VelocityPid::new(
            PidGains {
                kp: 5.0,
                ki: 20.0,
                kd: 1.0,
            },
            0.8,
        )
        .expect("gains valides");

        for step in 0..500 {
            let setpoint = if step % 50 < 25 { 100.0 } else { -100.0 };
            let output = pid.update(setpoint, 0.0, 0.02);

            assert!(
                (-0.8..=0.8).contains(&output),
                "sortie {output} hors bornes a l'etape {step}"
            );
        }
    }
}
