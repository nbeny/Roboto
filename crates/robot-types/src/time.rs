use core::fmt;
use core::ops::{Add, AddAssign};
use core::time::Duration;

/// Instant monotone, exprime comme une duree ecoulee depuis le demarrage du systeme.
///
/// Le coeur du robot ne lit jamais l'horloge : il recoit un `Monotonic` en parametre.
/// L'appelant le derive de `std::time::Instant` sur le robot reel, de l'horloge de
/// simulation sous Gazebo, ou d'une valeur fabriquee dans les tests.
///
/// Les soustractions sont saturantes : un temps qui recule ne provoque jamais de panique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Monotonic(Duration);

impl Monotonic {
    /// L'instant de demarrage.
    pub const ZERO: Self = Self(Duration::ZERO);

    /// Construit un instant a partir de la duree ecoulee depuis le demarrage.
    #[must_use]
    pub const fn from_since_boot(since_boot: Duration) -> Self {
        Self(since_boot)
    }

    /// Construit un instant a partir d'un nombre de millisecondes depuis le demarrage.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    /// Duree ecoulee depuis le demarrage.
    #[must_use]
    pub const fn since_boot(self) -> Duration {
        self.0
    }

    /// Duree ecoulee depuis `earlier`, saturee a zero si `earlier` est posterieur.
    #[must_use]
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    /// Avance l'instant de `delta`, ou `None` en cas de debordement.
    #[must_use]
    pub fn checked_add(self, delta: Duration) -> Option<Self> {
        self.0.checked_add(delta).map(Self)
    }
}

impl Add<Duration> for Monotonic {
    type Output = Self;

    fn add(self, delta: Duration) -> Self {
        Self(self.0 + delta)
    }
}

impl AddAssign<Duration> for Monotonic {
    fn add_assign(&mut self, delta: Duration) {
        *self = *self + delta;
    }
}

impl fmt::Display for Monotonic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}s", self.0.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_duration_since_returns_the_elapsed_time() {
        let start = Monotonic::from_millis(1_000);
        let later = Monotonic::from_millis(1_750);

        assert_eq!(
            later.saturating_duration_since(start),
            Duration::from_millis(750)
        );
    }

    #[test]
    fn saturating_duration_since_returns_zero_when_time_goes_backwards() {
        let later = Monotonic::from_millis(1_750);
        let start = Monotonic::from_millis(1_000);

        assert_eq!(start.saturating_duration_since(later), Duration::ZERO);
    }

    #[test]
    fn adding_a_duration_advances_the_instant() {
        let start = Monotonic::from_millis(500);

        let advanced = start + Duration::from_millis(250);

        assert_eq!(advanced, Monotonic::from_millis(750));
    }

    #[test]
    fn add_assign_advances_in_place() {
        let mut instant = Monotonic::ZERO;

        instant += Duration::from_millis(40);
        instant += Duration::from_millis(60);

        assert_eq!(instant, Monotonic::from_millis(100));
    }

    #[test]
    fn checked_add_reports_overflow_instead_of_panicking() {
        let far_future = Monotonic::from_since_boot(Duration::MAX);

        assert_eq!(far_future.checked_add(Duration::from_secs(1)), None);
    }

    #[test]
    fn instants_order_chronologically() {
        assert!(Monotonic::from_millis(1) < Monotonic::from_millis(2));
        assert_eq!(Monotonic::ZERO.since_boot(), Duration::ZERO);
    }
}
