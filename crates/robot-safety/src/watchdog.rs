use core::time::Duration;

use robot_types::Monotonic;

/// Chien de garde generique base sur le temps fourni par l'appelant.
///
/// Utilise en Milestone 1 pour le timeout de commande, et reutilise en Milestone 4 pour
/// le heartbeat de la liaison serie avec le microcontroleur.
///
/// # Armement
///
/// Un watchdog qui n'a jamais ete nourri n'est **pas** expire : il est desarme. C'est
/// `robot-core` qui etablit la ligne de base en le nourrissant a l'entree d'un etat de
/// mouvement. A partir de la, une commande doit arriver dans le delai imparti sous peine
/// d'arret de securite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watchdog {
    timeout: Duration,
    last_feed: Option<Monotonic>,
}

impl Watchdog {
    /// Cree un watchdog desarme.
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_feed: None,
        }
    }

    /// Delai configure.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Nourrit le watchdog et l'arme.
    pub const fn feed(&mut self, now: Monotonic) {
        self.last_feed = Some(now);
    }

    /// Desarme le watchdog : il ne peut plus expirer tant qu'il n'est pas renourri.
    pub const fn disarm(&mut self) {
        self.last_feed = None;
    }

    /// Indique si le watchdog a ete nourri au moins une fois depuis son dernier desarmement.
    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.last_feed.is_some()
    }

    /// Temps ecoule depuis le dernier repas, ou `None` si le watchdog est desarme.
    #[must_use]
    pub fn elapsed_since_feed(&self, now: Monotonic) -> Option<Duration> {
        self.last_feed
            .map(|fed_at| now.saturating_duration_since(fed_at))
    }

    /// Indique si le delai est ecoule. Un watchdog desarme n'expire jamais.
    #[must_use]
    pub fn is_expired(&self, now: Monotonic) -> bool {
        self.elapsed_since_feed(now)
            .is_some_and(|elapsed| elapsed >= self.timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watchdog() -> Watchdog {
        Watchdog::new(Duration::from_millis(500))
    }

    #[test]
    fn a_new_watchdog_is_disarmed() {
        let dog = watchdog();

        assert!(!dog.is_armed());
        assert_eq!(dog.elapsed_since_feed(Monotonic::from_millis(10_000)), None);
    }

    #[test]
    fn a_disarmed_watchdog_never_expires() {
        let dog = watchdog();

        assert!(!dog.is_expired(Monotonic::from_millis(10_000)));
    }

    #[test]
    fn feeding_arms_the_watchdog() {
        let mut dog = watchdog();

        dog.feed(Monotonic::from_millis(1_000));

        assert!(dog.is_armed());
    }

    #[test]
    fn a_freshly_fed_watchdog_has_not_expired() {
        let mut dog = watchdog();
        dog.feed(Monotonic::from_millis(1_000));

        assert!(!dog.is_expired(Monotonic::from_millis(1_499)));
    }

    #[test]
    fn the_watchdog_expires_once_the_timeout_is_reached() {
        let mut dog = watchdog();
        dog.feed(Monotonic::from_millis(1_000));

        assert!(dog.is_expired(Monotonic::from_millis(1_500)));
        assert!(dog.is_expired(Monotonic::from_millis(9_999)));
    }

    #[test]
    fn feeding_again_resets_the_countdown() {
        let mut dog = watchdog();
        dog.feed(Monotonic::from_millis(1_000));
        dog.feed(Monotonic::from_millis(1_400));

        assert!(!dog.is_expired(Monotonic::from_millis(1_800)));
        assert!(dog.is_expired(Monotonic::from_millis(1_900)));
    }

    #[test]
    fn elapsed_since_feed_reports_the_age_of_the_last_meal() {
        let mut dog = watchdog();
        dog.feed(Monotonic::from_millis(1_000));

        assert_eq!(
            dog.elapsed_since_feed(Monotonic::from_millis(1_120)),
            Some(Duration::from_millis(120))
        );
    }

    #[test]
    fn disarming_stops_the_countdown() {
        let mut dog = watchdog();
        dog.feed(Monotonic::from_millis(1_000));

        dog.disarm();

        assert!(!dog.is_armed());
        assert!(!dog.is_expired(Monotonic::from_millis(99_999)));
    }
}
