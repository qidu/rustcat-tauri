//! The tray icon: the running cat itself, its menu, and the run cycle.
//!
//! The icon lives in the macOS menu bar or the Windows notification area
//! depending on the platform, and its pace follows the metric named in
//! [`Config`] — see [`crate::pace`] for the reading, [`crate::speed`] for the
//! formula and [`crate::cat`] for the frames.
//!
//! Left click opens or closes the panel ([`crate::panel`]); the context menu
//! holds Quit. There is no main window: the tray icon *is* the app.

use std::thread;
use std::time::{Duration, Instant};

use rustcat_metrics::memory::MemorySource;
use rustcat_metrics::metric::{self, Metric, MetricSource};
use tauri::AppHandle;
use tauri::Wry;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::cat::{self, Frames};
use crate::config::Config;
use crate::pace::Pacer;
use crate::{panel, theme};

/// Id of the tray icon, so the run cycle can find it again.
pub const TRAY_ID: &str = "rustcat";
/// Id of the Quit menu item.
const QUIT_ID: &str = "quit";
/// Id of the Show Panel menu item.
const SHOW_ID: &str = "show";
/// Ids of the two read-only readings at the top of the menu. The first is the
/// metric the pace follows, whatever it is set to.
const SPEED_ID: &str = "reading_speed";
const MEMORY_ID: &str = "reading_memory";

/// Bytes in a gibibyte, for the readings. Matches what the panel prints.
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Floor for the loop's sleep, so a deadline already in the past cannot make it
/// spin.
const MIN_SLEEP: Duration = Duration::from_millis(1);

/// Create the tray icon, then start the cat running.
///
/// Takes ownership of `frames` because the run cycle needs them for as long as
/// the app lives.
pub fn build(app: &AppHandle, frames: Frames, config: &Config) -> tauri::Result<()> {
    let metric_label = config.speed_metric().label();

    // The two readings are menu items rather than only page content: a menu is
    // drawn by the platform, so the numbers still appear on a machine where the
    // webview never paints. They are disabled — they show a value, they do not
    // do anything.
    let speed_item = MenuItem::with_id(
        app,
        SPEED_ID,
        format!("{metric_label} —"),
        false,
        None::<&str>,
    )?;
    let memory_item = MenuItem::with_id(app, MEMORY_ID, "Memory —", false, None::<&str>)?;
    let show = MenuItem::with_id(app, SHOW_ID, "Show Panel", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, config.quit_label(), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&speed_item, &memory_item, &separator, &show, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(frames.frame(0, false).to_image())
        // macOS re-tints a template image for the current menu bar, so the same
        // artwork works on a light and a dark one. Windows has no equivalent and
        // picks a palette instead, inside the run cycle.
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(config.tooltip())
        .menu(&menu)
        // Left click belongs to the panel; the menu is on right click only.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                app.exit(0);
            } else if event.id() == SHOW_ID {
                // The same path a left click takes, for a click the panel did
                // not answer.
                log::info!("menu: showing the panel");
                panel::show(app, None);
            }
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();

            // A menubar app has no window to explain itself in, so every event
            // that could answer "why did nothing happen" is worth a line in the
            // log. `Move` is left out: it fires all the way across the icon and
            // would bury the rest.
            match &event {
                TrayIconEvent::Click {
                    button,
                    button_state,
                    position,
                    ..
                } => log::info!("tray click: {button:?} {button_state:?} at {position:?}"),
                TrayIconEvent::DoubleClick { button, .. } => {
                    log::info!("tray double click: {button:?}");
                }
                TrayIconEvent::Enter { .. } => log::info!("pointer entered the tray icon"),
                TrayIconEvent::Leave { .. } => log::info!("pointer left the tray icon"),
                TrayIconEvent::Move { .. } => {}
                // The enum is non-exhaustive, and a variant we do not know about
                // is one the panel has no use for.
                _ => {}
            }

            // The platform reports the press and the release as separate `Click`
            // events. The press is the one to act on: the release can go astray
            // — the pointer only has to leave the 34x22 icon between the two,
            // and a press that activates an inactive app can swallow the rest of
            // the gesture — and an app that waits for it looks like it does not
            // respond to clicks at all. Acting on both would toggle the panel
            // twice and cancel itself out.
            if let TrayIconEvent::Click {
                rect,
                button: MouseButton::Left,
                button_state: MouseButtonState::Down,
                ..
            } = event
            {
                log::info!("left click on the tray icon");
                panel::toggle(app, Some(rect));
            }
        })
        .build(app)?;

    log::info!("context menu: live readings, Show Panel, Quit");
    spawn_run_cycle(app.clone(), frames, config.clone(), speed_item, memory_item);
    Ok(())
}

/// Animate the cat on its own thread, advancing a frame per deadline and
/// re-reading the configured metric once per sample interval.
///
/// Runs until the tray icon goes away, which is what happens as the app exits.
/// Icon updates are safe from here: Tauri routes them to the main thread.
fn spawn_run_cycle(
    app: AppHandle,
    frames: Frames,
    config: Config,
    speed_item: MenuItem<Wry>,
    memory_item: MenuItem<Wry>,
) {
    thread::spawn(move || {
        let mut pacer = Pacer::new(&config);
        let mut memory_source = MemorySource::new();
        let mut interval = config.idle_interval();
        let mut dark_taskbar = theme::dark_taskbar();
        let mut frame = 0usize;

        let sample_interval = config.sample_interval();
        let metric_label = config.speed_metric().label();
        let app_name = config.tooltip().to_owned();

        let mut next_sample = Instant::now();
        let mut next_frame = Instant::now() + interval;

        while let Some(tray) = app.tray_by_id(TRAY_ID) {
            let now = Instant::now();

            if now >= next_sample {
                next_sample = now + sample_interval;

                let load = pacer.sample();
                interval = pacer.interval();

                // Windows lets the taskbar theme change at runtime, and this is
                // the same periodic tick that already exists.
                dark_taskbar = theme::dark_taskbar();

                // Read through the metric layer, so the row shows the same
                // `memory.used_percent` any other consumer would see — including
                // its guard against a system that reports no total.
                let memory = memory_source.read();
                let reading = |name: &str| {
                    metric::find(&memory, name)
                        .and_then(|metric: &Metric| metric.value.as_f64())
                        .unwrap_or(0.0)
                };
                let percent = reading("memory.used_percent");
                let used = reading("memory.used_bytes") / GIB;
                let total = reading("memory.total_bytes") / GIB;

                let _ = speed_item.set_text(format!("{metric_label} {load:.0}%"));
                let _ = memory_item
                    .set_text(format!("Memory {percent:.0}% ({used:.1} / {total:.1} GB)"));

                // Ignored on macOS, where a status item shows no tooltip; the
                // menu rows above are what a Mac user reads.
                let _ = tray.set_tooltip(Some(format!(
                    "{app_name} — {metric_label} {load:.0}% · Memory {percent:.0}%"
                )));
            }

            if now >= next_frame {
                next_frame = now + interval;
                let image = frames.frame(frame, dark_taskbar).to_image();
                // Atomic on macOS, where setting the icon and its template
                // status separately would render it twice and flicker.
                let _ = tray.set_icon_with_as_template(Some(image), cfg!(target_os = "macos"));
                frame = (frame + 1) % cat::FRAME_COUNT;
            }

            let until = next_frame
                .min(next_sample)
                .saturating_duration_since(Instant::now());
            thread::sleep(until.max(MIN_SLEEP));
        }
    });
}
