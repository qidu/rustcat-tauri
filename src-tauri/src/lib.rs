//! A running cat in the macOS menu bar and the Windows notification area, its
//! pace set by a system metric.
//!
//! There is no main window: the tray icon is the app. Its pace follows the metric
//! chosen in [`Config`], read from [`rustcat_metrics`], and a left click opens a
//! small panel with the CPU and memory readings the page polls for.
//!
//! # Examples
//!
//! The shipped cat, with the defaults:
//!
//! ```no_run
//! rustcat_tauri::run().expect("RustCat could not start");
//! ```
//!
//! The same cat, configured, so a caller can set the pace source and how often it
//! is read before anything starts:
//!
//! ```no_run
//! use rustcat_tauri::{RustCat, SpeedMetric};
//! use std::time::Duration;
//!
//! RustCat::builder()
//!     .speed_metric(SpeedMetric::Cpu)
//!     .sample_interval(Duration::from_secs(2))
//!     .tooltip("RustCat")
//!     .quit_label("Quit")
//!     .run()
//!     .expect("RustCat could not start");
//! ```
//!
//! `run` blocks until the app exits, so it must be called from the main thread.
//! To keep the tray icon without handing over `main`, use
//! [`RustCat::builder`] plus [`RustCat::run`](RustCat::run) at the point the
//! caller is ready.

pub mod cat;
pub mod config;
pub mod pace;
pub mod speed;
pub mod theme;
pub mod tray;

mod panel;

use rustcat_metrics::{cpu::CpuSource, memory::MemorySource, types::SystemSnapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub use config::{Config, RustCatBuilder, SpeedMetric};

/// The cat, as configured but not yet running.
///
/// Built through [`RustCat::builder`]. Holding one costs nothing: the tray icon,
/// the sources and the log plugin are all created by [`run`](Self::run).
#[derive(Debug, Clone)]
pub struct RustCat {
    config: Config,
}

impl RustCat {
    /// The shipped cat: [`Config::default`].
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    /// A builder carrying the shipped defaults.
    pub fn builder() -> RustCatBuilder {
        RustCatBuilder::new()
    }

    /// A cat with a configuration already in hand.
    pub fn with_config(config: Config) -> Self {
        Self { config }
    }

    /// The configuration this cat will run with.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Start the app and return when it exits.
    ///
    /// Must be called from the main thread: the platform takes the main thread
    /// for its own event loop, which is what a tray icon lives on.
    pub fn run(self) -> tauri::Result<()> {
        app(self.config).run(tauri::generate_context!())
    }
}

impl Default for RustCat {
    fn default() -> Self {
        Self::new()
    }
}

/// The default cat: [`RustCat::new`] then [`RustCat::run`].
///
/// The entry point `src/main.rs` calls, and the `mobile_entry_point` the platform
/// looks for.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    RustCat::new().run()
}

/// Every source the app reads, behind one lock each.
///
/// The sources are opened once and kept: a platform handle created per reading
/// would cost more than the reading does.
pub(crate) struct MetricsState {
    cpu: Mutex<CpuSource>,
    memory: Mutex<MemorySource>,
    /// Whether a reading has been logged yet; see `get_system_snapshot`.
    reported: AtomicBool,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            cpu: Mutex::new(CpuSource::new()),
            memory: Mutex::new(MemorySource::new()),
            reported: AtomicBool::new(false),
        }
    }
}

/// `#[tauri::command(async)]` keeps this off the main thread — the body is
/// blocking sysinfo work and there is nothing to await, so a real `async fn`
/// would only occupy an async-runtime worker.
#[tauri::command(async)]
fn get_system_snapshot(
    state: tauri::State<'_, Arc<MetricsState>>,
) -> Result<SystemSnapshot, String> {
    let mut snapshot = SystemSnapshot::new();

    snapshot.cpu = Some(state.cpu.lock().map_err(|e| e.to_string())?.refresh());
    snapshot.memory = Some(state.memory.lock().map_err(|e| e.to_string())?.refresh());

    // Worth a line because the webview loads as soon as the app starts, whether
    // or not the panel is ever on screen: this proves the page came up and can
    // read real numbers. Later readings would be one line a second for good.
    if !state.reported.swap(true, Ordering::Relaxed) {
        log::info!(
            "first snapshot served: cpu={:?}% memory={:?} bytes used",
            snapshot.cpu.as_ref().map(|cpu| cpu.global_usage),
            snapshot.memory.as_ref().map(|memory| memory.used_bytes)
        );
    }

    Ok(snapshot)
}

/// Hide the panel: the page calls this when Escape is pressed.
#[tauri::command]
fn hide_panel(app: AppHandle) {
    log::info!("panel hidden by the page (Escape)");
    panel::hide(&app);
}

/// The page's own account of itself, so its view of the panel sits in the log
/// next to the window server's.
///
/// The two ways the panel can fail to show anything are identical from the Rust
/// side: a window whose page never painted, and a page that painted a placeholder
/// because no reading ever arrived. Only the page can tell them apart, and it
/// says which by reporting whether it is being asked to draw frames at all and
/// what the two cards currently read.
#[tauri::command]
fn panel_report(report: String) {
    log::info!("panel page: {report}");
}

/// The app, wired up but not running.
fn app(config: Config) -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .manage(Arc::new(MetricsState::default()))
        .manage(panel::PanelState::default())
        .invoke_handler(tauri::generate_handler![
            get_system_snapshot,
            hide_panel,
            panel_report
        ])
        .setup(move |app| {
            // Always on, not only in debug builds: the panel's self-report and
            // the tray's lines are the only account a release run gives of
            // itself, and a launch from Finder has no terminal to print to. The
            // default targets are stdout plus the app's log directory, so the
            // file is left behind either way.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
            match app.path().app_log_dir() {
                Ok(dir) => log::info!("logs: {}", dir.display()),
                Err(error) => log::warn!("no log directory: {error}"),
            }

            // No Dock icon and nothing in the app switcher: the menu bar icon is
            // the whole presence of the app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::build(app.handle(), cat::Frames::load()?, &config)?;

            // The tray icon is the whole user interface, so a debug run has to
            // leave a trace of what it built: without this, a run that never
            // reacted to a click is indistinguishable from one whose setup never
            // ran at all.
            log::info!(
                "RustCat started, tray icon in place, pace follows {} every {:?}",
                config.speed_metric().metric_name(),
                config.sample_interval()
            );

            // The panel is declared in `tauri.conf.json`, so Tauri built it
            // before this closure ran.
            if let Some(panel) = app.get_webview_window(panel::PANEL_LABEL) {
                log::info!("panel window found; clicking the cat opens it");
                panel::attach(&panel, app.handle());
            } else {
                log::warn!("no panel window: a left click will have nothing to show");
            }

            // The panel is placed against the screen the tray icon is on, so the
            // screens in play belong in the log beside the placement itself: a
            // panel can be in exactly the right place relative to the icon and
            // still be on a screen nobody is looking at.
            match app.available_monitors() {
                Ok(monitors) => {
                    for monitor in &monitors {
                        log::info!(
                            "monitor {:?}: at {:?} size {:?} scale {}",
                            monitor.name(),
                            monitor.position(),
                            monitor.size(),
                            monitor.scale_factor()
                        );
                    }
                }
                Err(error) => log::warn!("no monitors reported: {error}"),
            }

            // A tray click cannot be scripted, which leaves the whole show path
            // untestable by hand. `RUSTCAT_SHOW_AT_START=1 cargo run` opens the
            // panel on its own a moment after startup so the path can be seen.
            if cfg!(debug_assertions) && std::env::var_os("RUSTCAT_SHOW_AT_START").is_some() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    log::info!("RUSTCAT_SHOW_AT_START: opening the panel");
                    panel::show(&handle, None);
                });
            }

            Ok(())
        })
}
