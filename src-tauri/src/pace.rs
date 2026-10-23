//! The pace seam: a metric reading on one side, a frame interval on the other.
//!
//! [`Pacer`] owns everything between "which metric" and "how long each frame is
//! shown" — the source, the 5-point window, the formula in [`crate::speed`] — so
//! the run cycle only has to ask for a sample and read back an interval. Keeping
//! it apart from the tray is what makes the pace testable without an app.
//!
//! The reading is taken through [`MetricSource`], not by reaching into the source
//! struct, so the pace is driven by the very metric named in
//! [`SpeedMetric::metric_name`] — the one a snapshot publishes.

use std::time::Duration;

use rustcat_metrics::cpu::CpuSource;
use rustcat_metrics::metric::{self, MetricSource};
use rustcat_metrics::{Metric, MetricValue};

use crate::config::{Config, SpeedMetric};
use crate::speed::{RollingAverage, interval_ms};

/// Turns readings of the configured metric into the current frame interval.
pub struct Pacer {
    metric: SpeedMetric,
    cpu: CpuSource,
    average: RollingAverage,
    interval: Duration,
    idle_interval: Duration,
}

impl Pacer {
    pub fn new(config: &Config) -> Self {
        Self {
            metric: config.speed_metric(),
            cpu: CpuSource::new(),
            average: RollingAverage::default(),
            interval: config.idle_interval(),
            idle_interval: config.idle_interval(),
        }
    }

    /// The metric whose load this pace follows.
    pub fn metric(&self) -> SpeedMetric {
        self.metric
    }

    /// Read the metric once and recompute the pace, returning the reading so the
    /// caller can show it without a second read.
    ///
    /// The interval is set from the window average, not from this reading alone,
    /// so one spiky sample moves the cat a fifth of the way.
    pub fn sample(&mut self) -> f64 {
        let load = self.read();
        self.average.push(load);
        self.interval = self
            .average
            .average()
            .map_or(self.idle_interval, |average| {
                Duration::from_millis(interval_ms(average))
            });
        load
    }

    /// How long the current frame is shown, as of the last [`sample`](Self::sample).
    ///
    /// Before the first sample this is the configured idle interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// The configured metric, read once.
    fn read(&mut self) -> f64 {
        let readings = match self.metric {
            SpeedMetric::Cpu => self.cpu.read(),
        };
        reading(&readings, self.metric.metric_name())
    }
}

/// The named reading as a 0-100 load.
///
/// A source that did not publish the metric, published it as text, or published
/// a value outside the range yields the extremes rather than a surprise: the cat
/// idles, or the cat sprints, and never divides by something it made up.
fn reading(readings: &[Metric], name: &str) -> f64 {
    let value = match metric::find(readings, name).map(|metric| &metric.value) {
        Some(MetricValue::Count(count)) => *count as f64,
        Some(MetricValue::Gauge(gauge)) => *gauge,
        Some(MetricValue::Text(_)) | None => 0.0,
    };
    // `clamp` passes NaN through, and `interval_ms` already degrades it to the
    // slowest pace, so there is nothing extra to do here.
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustcat_metrics::Unit;

    fn readings(value: impl Into<MetricValue>) -> Vec<Metric> {
        vec![Metric::new("cpu.global_usage", value, Unit::Percent)]
    }

    /// A configuration whose idle interval is the given number of milliseconds,
    /// built the way a caller would.
    fn configured(idle_ms: u64) -> Config {
        crate::RustCatBuilder::new()
            .idle_interval(Duration::from_millis(idle_ms))
            .build()
            .config()
            .clone()
    }

    #[test]
    fn reads_the_named_metric() {
        assert_eq!(reading(&readings(42.5), "cpu.global_usage"), 42.5);
    }

    #[test]
    fn a_count_is_read_as_a_number() {
        assert_eq!(reading(&readings(7u64), "cpu.global_usage"), 7.0);
    }

    #[test]
    fn a_missing_metric_idles_rather_than_failing() {
        assert_eq!(reading(&readings(42.5), "cpu.absent"), 0.0);
        assert_eq!(reading(&[], "cpu.global_usage"), 0.0);
    }

    #[test]
    fn a_text_reading_idles() {
        assert_eq!(reading(&readings("busy"), "cpu.global_usage"), 0.0);
    }

    #[test]
    fn out_of_range_readings_are_clamped() {
        assert_eq!(reading(&readings(-4.0), "cpu.global_usage"), 0.0);
        assert_eq!(reading(&readings(150.0), "cpu.global_usage"), 100.0);
    }

    #[test]
    fn nan_idles() {
        assert_eq!(reading(&readings(f64::NAN), "cpu.global_usage"), 0.0);
    }

    /// The pace a fresh pacer reports has to be the configured one, before any
    /// reading exists — the run cycle uses it to place the first frame.
    #[test]
    fn a_fresh_pacer_uses_the_idle_interval() {
        let pacer = Pacer::new(&configured(123));

        assert_eq!(pacer.interval(), Duration::from_millis(123));
        assert_eq!(pacer.metric(), SpeedMetric::Cpu);
    }

    #[test]
    fn the_configured_metric_names_a_published_reading() {
        // The seam between the two crates: a name that no source publishes would
        // make every sample idle, silently.
        let published = CpuSource::new().read();
        assert!(
            metric::find(&published, SpeedMetric::Cpu.metric_name()).is_some(),
            "{} is not published by CpuSource",
            SpeedMetric::Cpu.metric_name()
        );
    }

    #[test]
    fn sampling_moves_the_pace_off_the_idle_interval() {
        let mut pacer = Pacer::new(&configured(500));
        let load = pacer.sample();

        assert!((0.0..=100.0).contains(&load), "load out of range: {load}");
        assert!(pacer.interval() <= Duration::from_millis(500));
        assert!(pacer.interval() >= Duration::from_millis(25));
    }
}
