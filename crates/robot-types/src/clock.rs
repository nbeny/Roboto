use core::time::Duration;

use crate::Monotonic;

/// Resultat d'une lecture d'horloge.
///
/// Marque `#[must_use]` a dessein : laisser tomber une lecture, c'est laisser tomber le
/// drapeau [`ClockReading::rewound`], donc manquer une perte de repere temporel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct ClockReading {
    /// Instant monotone interne correspondant a la lecture.
    pub now: Monotonic,
    /// L'horloge source a recule.
    ///
    /// Le temps interne, lui, ne recule jamais : il se fige le temps d'un cycle et
    /// l'origine est reancree. Le consommateur **doit** traiter ce drapeau comme une
    /// perte de repere temporel et forcer un arret de securite : entre deux lectures,
    /// il ne sait plus combien de temps s'est ecoule.
    ///
    /// N'est leve que si le recul depasse la tolerance de l'horloge : voir
    /// [`MonotonicClock::with_tolerance`].
    pub rewound: bool,
    /// Ampleur du recul constate sur l'horloge source, nulle en marche normale.
    ///
    /// Renseigne meme lorsque le recul reste sous la tolerance et que `rewound` est
    /// faux : un recul absorbe reste un fait observable, qui merite d'etre mesure.
    pub backward_step: Duration,
}

/// Convertit une horloge externe en temps monotone interne.
///
/// Le coeur du robot raisonne en [`Monotonic`], une duree depuis son propre demarrage.
/// Les sources de temps reelles ne fournissent pas cela : ROS 2 publie un temps epoch en
/// secondes et nanosecondes, Gazebo un temps de simulation qui repart de zero a chaque
/// relance, et un microcontroleur un compteur de ticks.
///
/// Cette horloge fait la conversion en garantissant une propriete que le coeur tient pour
/// acquise : **le temps interne ne decroit jamais**.
///
/// # Pourquoi le drapeau de recul importe
///
/// Une conversion naive qui se contenterait de saturer figerait le temps interne apres un
/// saut en arriere. Or un temps fige est un watchdog qui n'expire jamais : le robot
/// poursuivrait indefiniment sur sa derniere commande. Le recul doit donc etre signale,
/// pas absorbe en silence.
///
/// ```
/// use robot_types::MonotonicClock;
///
/// let mut clock = MonotonicClock::new();
///
/// let start = clock.stamp(1_700_000_000_000_000_000);
/// assert_eq!(start.now.since_boot().as_nanos(), 0);
///
/// let later = clock.stamp(1_700_000_000_250_000_000);
/// assert_eq!(later.now.since_boot().as_millis(), 250);
/// assert!(!later.rewound);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MonotonicClock {
    anchor: Option<Anchor>,
    /// Derniere valeur brute lue sur l'horloge source.
    last_source_nanos: u64,
    last: Monotonic,
    tolerance: Duration,
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Point de reference reliant l'horloge source au temps interne.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    /// Valeur de l'horloge source au moment de l'ancrage.
    source_nanos: u64,
    /// Temps interne correspondant.
    internal: Duration,
}

impl MonotonicClock {
    /// Tolerance par defaut aux reculs de l'horloge source.
    ///
    /// Une horloge murale n'est pas monotone : NTP la recale, et une machine virtuelle
    /// la resynchronise sur son hote — le cas se produit sous Docker Desktop et WSL2.
    /// Ces reculs se comptent en microsecondes et n'apprennent rien sur l'ecoulement
    /// reel du temps.
    ///
    /// La valeur est choisie loin des deux echelles qu'elle doit separer : deux ordres de
    /// grandeur au-dessus du recalage d'horloge, et un ordre de grandeur sous le timeout
    /// de commande de 500 ms. Absorber jusqu'a 50 ms ne peut donc pas masquer un blocage
    /// dangereux, tandis qu'une relance de simulateur — qui se compte en secondes — reste
    /// signalee.
    pub const DEFAULT_TOLERANCE: Duration = Duration::from_millis(50);

    /// Cree une horloge sans origine : la premiere lecture l'etablira.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_tolerance(Self::DEFAULT_TOLERANCE)
    }

    /// Cree une horloge avec une tolerance explicite aux reculs.
    ///
    /// Une tolerance nulle signale le moindre recul, ce qui convient a une source
    /// reputee monotone, par exemple un compteur de ticks de microcontroleur.
    #[must_use]
    pub const fn with_tolerance(tolerance: Duration) -> Self {
        Self {
            anchor: None,
            last_source_nanos: 0,
            last: Monotonic::ZERO,
            tolerance,
        }
    }

    /// Tolerance en vigueur.
    #[must_use]
    pub const fn tolerance(&self) -> Duration {
        self.tolerance
    }

    /// Indique si une origine a deja ete etablie.
    #[must_use]
    pub const fn is_started(&self) -> bool {
        self.anchor.is_some()
    }

    /// Dernier instant interne produit.
    #[must_use]
    pub const fn last(&self) -> Monotonic {
        self.last
    }

    /// Convertit une lecture de l'horloge source, exprimee en nanosecondes.
    ///
    /// L'origine de l'horloge source est sans importance : seuls les ecarts comptent.
    pub fn stamp(&mut self, source_nanos: u64) -> ClockReading {
        let Some(anchor) = self.anchor else {
            self.anchor = Some(Anchor {
                source_nanos,
                internal: Duration::ZERO,
            });
            self.last_source_nanos = source_nanos;
            self.last = Monotonic::ZERO;

            return ClockReading {
                now: Monotonic::ZERO,
                rewound: false,
                backward_step: Duration::ZERO,
            };
        };

        if source_nanos < self.last_source_nanos {
            let step = Duration::from_nanos(self.last_source_nanos - source_nanos);
            return self.rewind(source_nanos, step);
        }

        // `anchor.source_nanos <= last_source_nanos <= source_nanos` est un invariant :
        // l'ancre est posee sur une lecture, et toute lecture anterieure passe par la
        // branche de recul ci-dessus.
        let elapsed = source_nanos.saturating_sub(anchor.source_nanos);
        let candidate = Monotonic::from_since_boot(
            anchor
                .internal
                .saturating_add(Duration::from_nanos(elapsed)),
        );

        self.last = candidate;
        self.last_source_nanos = source_nanos;

        ClockReading {
            now: candidate,
            rewound: false,
            backward_step: Duration::ZERO,
        }
    }

    /// Reancre l'horloge sur la lecture courante en conservant le temps interne atteint.
    ///
    /// Le temps interne se fige donc pour un cycle, puis reprend sa progression a partir
    /// de la : c'est le seul moyen de rester monotone sans perdre l'echelle de temps.
    /// Cette reprise a lieu que le recul soit tolere ou non — seule l'alarme differe.
    fn rewind(&mut self, source_nanos: u64, step: Duration) -> ClockReading {
        self.anchor = Some(Anchor {
            source_nanos,
            internal: self.last.since_boot(),
        });
        self.last_source_nanos = source_nanos;

        ClockReading {
            now: self.last,
            rewound: step > self.tolerance,
            backward_step: step,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une origine arbitraire et volumineuse, pour verifier que la valeur absolue de
    /// l'horloge source n'a aucune importance.
    const EPOCH: u64 = 1_700_000_000_000_000_000;

    fn millis(value: u64) -> u64 {
        value * 1_000_000
    }

    /// Fait avancer l'horloge pour installer un etat de depart, sans examiner la lecture.
    fn advance(clock: &mut MonotonicClock, source_nanos: u64) {
        let _ = clock.stamp(source_nanos);
    }

    #[test]
    fn a_new_clock_has_no_origin() {
        let clock = MonotonicClock::new();

        assert!(!clock.is_started());
        assert_eq!(clock.last(), Monotonic::ZERO);
    }

    #[test]
    fn the_first_reading_establishes_the_origin_at_zero() {
        let mut clock = MonotonicClock::new();

        let reading = clock.stamp(EPOCH);

        assert_eq!(reading.now, Monotonic::ZERO);
        assert!(!reading.rewound);
        assert!(clock.is_started());
    }

    #[test]
    fn readings_measure_the_time_elapsed_since_the_origin() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);

        let reading = clock.stamp(EPOCH + millis(250));

        assert_eq!(reading.now, Monotonic::from_millis(250));
        assert!(!reading.rewound);
    }

    #[test]
    fn a_stalled_source_reports_the_same_instant_twice() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(100));

        let reading = clock.stamp(EPOCH + millis(100));

        assert_eq!(reading.now, Monotonic::from_millis(100));
        assert!(!reading.rewound);
    }

    #[test]
    fn a_backward_step_is_flagged_and_freezes_the_instant() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));

        let reading = clock.stamp(EPOCH + millis(300));

        assert_eq!(
            reading.now,
            Monotonic::from_millis(500),
            "le temps interne ne doit pas reculer"
        );
        assert!(reading.rewound);
    }

    #[test]
    fn the_clock_resumes_from_where_it_froze_after_a_backward_step() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));
        advance(&mut clock, EPOCH + millis(300));

        let reading = clock.stamp(EPOCH + millis(340));

        assert_eq!(reading.now, Monotonic::from_millis(540));
        assert!(!reading.rewound);
    }

    #[test]
    fn a_source_restarting_from_zero_is_handled() {
        // Cas concret : Gazebo relance, le temps de simulation repart de zero alors que
        // le robot tourne depuis 60 s.
        let mut clock = MonotonicClock::new();
        advance(&mut clock, 0);
        advance(&mut clock, 60 * 1_000_000_000);

        let restart = clock.stamp(0);
        let after_restart = clock.stamp(millis(100));

        assert!(restart.rewound);
        assert_eq!(restart.now, Monotonic::from_millis(60_000));
        assert!(!after_restart.rewound);
        assert_eq!(after_restart.now, Monotonic::from_millis(60_100));
    }

    #[test]
    fn a_reading_below_the_anchor_is_treated_as_a_rewind() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, millis(1_000));

        let reading = clock.stamp(millis(400));

        assert!(reading.rewound);
        assert_eq!(reading.now, Monotonic::ZERO);
    }

    #[test]
    fn a_normal_reading_reports_no_backward_step() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);

        let reading = clock.stamp(EPOCH + millis(250));

        assert_eq!(reading.backward_step, Duration::ZERO);
    }

    #[test]
    fn the_backward_step_measures_the_size_of_the_jump() {
        let mut clock = MonotonicClock::new();
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));

        let reading = clock.stamp(EPOCH + millis(300));

        assert_eq!(reading.backward_step, Duration::from_millis(200));
    }

    #[test]
    fn a_backward_step_within_tolerance_raises_no_alarm() {
        // Cas concret : recalage d'une horloge murale par NTP ou par l'hote d'une machine
        // virtuelle. Quelques millisecondes n'apprennent rien sur l'ecoulement du temps.
        let mut clock = MonotonicClock::with_tolerance(Duration::from_millis(50));
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));

        let reading = clock.stamp(EPOCH + millis(490));

        assert!(!reading.rewound, "un recul de 10 ms ne doit pas alarmer");
        assert_eq!(reading.backward_step, Duration::from_millis(10));
    }

    #[test]
    fn a_tolerated_backward_step_still_freezes_the_instant() {
        let mut clock = MonotonicClock::with_tolerance(Duration::from_millis(50));
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));

        let reading = clock.stamp(EPOCH + millis(490));

        assert_eq!(
            reading.now,
            Monotonic::from_millis(500),
            "tolerer un recul n'autorise pas le temps interne a reculer"
        );
    }

    #[test]
    fn the_clock_resumes_correctly_after_a_tolerated_step() {
        let mut clock = MonotonicClock::with_tolerance(Duration::from_millis(50));
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));
        advance(&mut clock, EPOCH + millis(490));

        let reading = clock.stamp(EPOCH + millis(510));

        assert_eq!(reading.now, Monotonic::from_millis(520));
        assert!(!reading.rewound);
    }

    #[test]
    fn a_backward_step_beyond_tolerance_is_flagged() {
        let mut clock = MonotonicClock::with_tolerance(Duration::from_millis(50));
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + millis(500));

        let reading = clock.stamp(EPOCH + millis(449));

        assert!(reading.rewound, "un recul de 51 ms depasse la tolerance");
        assert_eq!(reading.backward_step, Duration::from_millis(51));
    }

    #[test]
    fn a_zero_tolerance_flags_the_slightest_step() {
        let mut clock = MonotonicClock::with_tolerance(Duration::ZERO);
        advance(&mut clock, EPOCH);
        advance(&mut clock, EPOCH + 1_000);

        let reading = clock.stamp(EPOCH + 999);

        assert!(reading.rewound);
        assert_eq!(reading.backward_step, Duration::from_nanos(1));
    }

    #[test]
    fn the_default_tolerance_sits_between_clock_skew_and_a_simulator_restart() {
        let tolerance = MonotonicClock::new().tolerance();

        assert!(
            tolerance >= Duration::from_millis(1),
            "trop serre : le moindre recalage d'horloge declencherait une alarme"
        );
        assert!(
            tolerance < Duration::from_millis(500),
            "trop large : la tolerance doit rester sous le timeout de commande"
        );
    }

    #[test]
    fn internal_time_never_decreases_across_an_erratic_source() {
        let mut clock = MonotonicClock::new();
        let source = [
            EPOCH,
            EPOCH + millis(100),
            EPOCH + millis(50),
            EPOCH + millis(400),
            EPOCH,
            EPOCH + millis(10),
            EPOCH + millis(9_000),
        ];

        let mut previous = Monotonic::ZERO;
        for value in source {
            let reading = clock.stamp(value);
            assert!(
                reading.now >= previous,
                "recul du temps interne : {:?} apres {:?}",
                reading.now,
                previous
            );
            previous = reading.now;
        }
    }
}
