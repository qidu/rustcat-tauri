//! What a caller sets: which metric paces the cat, how often it is read, and the
//! two strings the platform shows.
//!
//! [`Config`] is built through [`RustCatBuilder`](crate::RustCatBuilder) rather
//! than by assigning fields, so adding a setting later stays compatible.

use std::time::Duration;

/// Which reading drives the cat's pace.
///
/// A variant is a metric name in [`rustcat_metrics`], read through the same
/// generic layer any other consumer uses — so what paces the cat is exactly what
/// a snapshot published under that name.
///
/// The enum is `#[non_exhaustive]` because a list of pace sources is the sort of
/// thing that grows, and a caller's `match` on it should keep compiling when it
/// does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpeedMetric {
    /// System-wide CPU load, 0-100%.
    #[default]
    Cpu,
}

impl SpeedMetric {
    /// The metric this reads, as published by `rustcat-metrics`.
    pub fn metric_name(self) -> &'static str {
        match self {
            SpeedMetric::Cpu => "cpu.global_usage",
        }
    }

    /// The name to show a person, for a menu row or a tooltip.
    pub fn label(self) -> &'static str {
        match self {
            SpeedMetric::Cpu => "CPU",
        }
    }
}

/// Everything about a running cat that a caller can change.
///
/// [`Default`] is the configuration the shipped cat runs with; take it and
/// change one thing rather than naming all of them.
#[derive(Debug, Clone)]
pub struct Config {
    speed_metric: SpeedMetric,
    sample_interval: Duration,
    idle_interval: Duration,
    tooltip: String,
    quit_label: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed_metric: SpeedMetric::default(),
            // RustCat's own 1000 ms CPU sample timer.
            sample_interval: Duration::from_millis(1000),
            // The frame interval before the first sample lands, which is also the
            // slowest pace: RustCat starts its cat at this speed.
            idle_interval: Duration::from_millis(500),
            tooltip: "RustCat".to_owned(),
            quit_label: "Quit RustCat".to_owned(),
        }
    }
}

impl Config {
    /// The metric the pace follows.
    pub fn speed_metric(&self) -> SpeedMetric {
        self.speed_metric
    }

    /// How often the metric is read.
    pub fn sample_interval(&self) -> Duration {
        self.sample_interval
    }

    /// The pace before the first sample lands, and the slowest the cat ever runs.
    pub fn idle_interval(&self) -> Duration {
        self.idle_interval
    }

    /// The name shown in the tooltip; the live readings are appended to it.
    ///
    /// macOS gives a status item no tooltip, so there the menu rows are what a
    /// person reads and this is only the app's name.
    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    /// The label on the menu item that exits the app.
    pub fn quit_label(&self) -> &str {
        &self.quit_label
    }
}

/// Reading the metric any faster than this costs a core to learn nothing —
/// CPU load is a figure averaged over its own interval, so samples closer
/// together than this describe the same moment. Clamped rather than rejected so
/// an interval computed from a setting cannot peg the machine.
const MIN_SAMPLE: Duration = Duration::from_millis(50);

/// The run loop already floors its sleep at 1 ms; this keeps the interval it is
/// counting down from being shorter than the floor.
const MIN_IDLE: Duration = Duration::from_millis(1);

/// Builds a [`RustCat`](crate::RustCat), and starts the default one.
///
/// ```
/// use rustcat_tauri::{RustCat, SpeedMetric};
/// use std::time::Duration;
///
/// let cat = RustCat::builder()
///     .speed_metric(SpeedMetric::Cpu)
///     .sample_interval(Duration::from_secs(2))
///     .build();
/// # let _ = cat;
/// ```
#[derive(Debug, Clone, Default)]
pub struct RustCatBuilder {
    config: Config,
}

impl RustCatBuilder {
    /// A builder carrying the shipped defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Which metric the cat's pace follows. Defaults to [`SpeedMetric::Cpu`].
    pub fn speed_metric(mut self, metric: SpeedMetric) -> Self {
        self.config.speed_metric = metric;
        self
    }

    /// How often the metric is read. Defaults to one second.
    ///
    /// Clamped to a floor of 50 ms — see [`MIN_SAMPLE`].
    pub fn sample_interval(mut self, interval: Duration) -> Self {
        self.config.sample_interval = interval.max(MIN_SAMPLE);
        self
    }

    /// The pace before the first sample lands, and the slowest the cat ever runs.
    /// Defaults to 500 ms.
    pub fn idle_interval(mut self, interval: Duration) -> Self {
        self.config.idle_interval = interval.max(MIN_IDLE);
        self
    }

    /// The name shown in the tooltip; the live readings are appended to it.
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.config.tooltip = tooltip.into();
        self
    }

    /// The label on the menu item that exits the app.
    pub fn quit_label(mut self, label: impl Into<String>) -> Self {
        self.config.quit_label = label.into();
        self
    }

    /// The configuration as built, for a caller that wants to inspect or store it
    /// before running.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The cat this builder describes, not yet running.
    pub fn build(self) -> crate::RustCat {
        crate::RustCat::with_config(self.config)
    }

    /// Build and run: [`build`](Self::build) then
    /// [`RustCat::run`](crate::RustCat::run).
    ///
    /// Returns when the app exits.
    pub fn run(self) -> tauri::Result<()> {
        self.build().run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_shipped_cat() {
        let config = Config::default();

        assert_eq!(config.speed_metric(), SpeedMetric::Cpu);
        assert_eq!(config.sample_interval(), Duration::from_millis(1000));
        assert_eq!(config.idle_interval(), Duration::from_millis(500));
        assert_eq!(config.tooltip(), "RustCat");
        assert_eq!(config.quit_label(), "Quit RustCat");
    }

    #[test]
    fn a_builder_changes_only_what_it_is_asked_to() {
        let config = RustCatBuilder::new()
            .speed_metric(SpeedMetric::Cpu)
            .tooltip("Kitty")
            .build()
            .config()
            .clone();

        assert_eq!(config.tooltip(), "Kitty");
        assert_eq!(config.sample_interval(), Duration::from_millis(1000));
        assert_eq!(config.quit_label(), "Quit RustCat");
    }

    /// A zero interval would sample as fast as the loop turns, so it is raised to
    /// the floor instead of being taken literally.
    #[test]
    fn sampling_faster_than_the_floor_is_clamped() {
        let config = RustCatBuilder::new()
            .sample_interval(Duration::ZERO)
            .idle_interval(Duration::ZERO)
            .build()
            .config()
            .clone();

        assert_eq!(config.sample_interval(), MIN_SAMPLE);
        assert_eq!(config.idle_interval(), MIN_IDLE);
    }

    #[test]
    fn each_metric_names_the_reading_it_follows() {
        assert_eq!(SpeedMetric::Cpu.metric_name(), "cpu.global_usage");
        assert_eq!(SpeedMetric::Cpu.label(), "CPU");
        assert_eq!(SpeedMetric::default(), SpeedMetric::Cpu);
    }
}
