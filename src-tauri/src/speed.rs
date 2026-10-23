//! Pure speed-domain helpers: a 5-point rolling window plus the pace formula.
//!
//! Behaviour is equivalent to RustCat's `src/speed.rs` (itself matched against
//! RunCat365's `CalculateInterval` and `CPURepository` window):
//!
//! ```text
//! speed    = max(1.0, load / 5.0)   // fixed 40fps tier, rate = 1
//! interval = (500.0 / speed) as u64 // milliseconds, truncating toward zero
//! ```

/// RunCat365's `CPU_INFO_LIST_LIMIT_SIZE = 5`.
const WINDOW: usize = 5;

/// A 5-sample rolling average.
///
/// The mean does not depend on write order, so a ring buffer is enough: no
/// shifting, no heap allocation.
#[derive(Default)]
pub struct RollingAverage {
    buf: [f64; WINDOW],
    len: usize,
    head: usize,
}

impl RollingAverage {
    pub fn push(&mut self, v: f64) {
        self.buf[self.head] = v;
        self.head = (self.head + 1) % WINDOW;
        self.len = (self.len + 1).min(WINDOW);
    }

    /// `None` while there are no samples (honest empty semantics). Once the
    /// window is full the whole buffer is valid, since order does not matter.
    pub fn average(&self) -> Option<f64> {
        (self.len > 0).then(|| self.buf[..self.len].iter().sum::<f64>() / self.len as f64)
    }
}

/// CPU load (0–100) → frame interval (ms).
///
/// The clamp is a seam-level backstop; the normal path already caps the value
/// at 100 upstream.
pub fn interval_ms(load: f64) -> u64 {
    let speed = (load.clamp(0.0, 100.0) / 5.0).max(1.0);
    (500.0 / speed) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- the pace formula --
    #[test]
    fn zero_load_is_slowest() {
        assert_eq!(interval_ms(0.0), 500);
    }
    #[test]
    fn below_speed_floor_stays_slowest() {
        assert_eq!(interval_ms(1.0), 500);
    }
    #[test]
    fn ten_percent() {
        assert_eq!(interval_ms(10.0), 250);
    }
    #[test]
    fn fifty_percent() {
        assert_eq!(interval_ms(50.0), 50);
    }
    #[test]
    fn near_full_truncates_toward_zero() {
        assert_eq!(interval_ms(99.9), 25);
    }
    #[test]
    fn full_load_is_fastest() {
        assert_eq!(interval_ms(100.0), 25);
    }
    #[test]
    fn over_hundred_is_clamped() {
        assert_eq!(interval_ms(150.0), 25);
    }
    #[test]
    fn negative_is_clamped() {
        assert_eq!(interval_ms(-3.0), 500);
    }
    #[test]
    fn nan_degrades_to_slowest() {
        assert_eq!(interval_ms(f64::NAN), 500);
    }

    // -- the 5-point rolling window --
    #[test]
    fn empty_window_has_no_average() {
        assert_eq!(RollingAverage::default().average(), None);
    }
    #[test]
    fn average_of_partial_window() {
        let mut w = RollingAverage::default();
        w.push(10.0);
        w.push(20.0);
        assert_eq!(w.average(), Some(15.0));
    }
    #[test]
    fn average_of_full_window() {
        let mut w = RollingAverage::default();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            w.push(v);
        }
        assert_eq!(w.average(), Some(3.0));
    }
    #[test]
    fn sixth_sample_evicts_oldest() {
        let mut w = RollingAverage::default();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            w.push(v);
        }
        assert_eq!(w.average(), Some(4.0));
    }

    // -- combined: the window average feeds the formula --
    #[test]
    fn window_feeds_interval() {
        let mut w = RollingAverage::default();
        for v in [0.0, 0.0, 0.0, 0.0, 100.0] {
            w.push(v);
        }
        assert_eq!(interval_ms(w.average().unwrap()), 125);
    }
}
