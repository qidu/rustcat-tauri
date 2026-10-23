//! The little CPU/memory panel that hangs off the tray icon.
//!
//! It is an ordinary always-on-top window (`panel` in `tauri.conf.json`), hidden
//! at rest and placed next to the tray icon each time it is shown. Three things
//! close it: a left click on the tray icon, losing focus, and Escape (which the
//! page turns into a [`hide_panel`](crate::hide_panel) call).
//!
//! ## Why there is an auto-hide timestamp
//!
//! On a left click the tray icon, the panel loses focus *first* — the blur hides
//! it — and the click event arrives after. By then the panel is already hidden,
//! so a plain "toggle" would read the click as a request to reopen and the panel
//! would never close. So the blur records when it fired, and a click that lands
//! within [`SAME_GESTURE`] of it is treated as the dismissal it really was
//! rather than as a fresh open.
//!
//! The cost is that a click this soon after *any* focus loss is swallowed, so a
//! click-away followed by a tray click within 300 ms leaves the panel shut. That
//! is rare, and every alternative is worse: without the stamp, clicking the tray
//! icon can never dismiss the panel.
//!
//! ## Why a blur only counts once the panel has taken focus
//!
//! Showing the panel is itself a focus change, and the click that asked for it is
//! still being resolved while it happens: the panel can be ordered front, blur,
//! and take focus for real a moment later. Hiding on that first blur makes the
//! panel flash and disappear, which is indistinguishable from clicking doing
//! nothing at all.
//!
//! No timeout can tell those two blurs apart — the one that means "the user moved
//! on" can arrive any number of milliseconds after the one that is just the click
//! being delivered. So the panel watches for the evidence instead: it remembers
//! whether it has *taken* focus since it was shown, and a blur only closes it when
//! it has. A panel that never took focus is still opening, and leaving it up is
//! the harmless error; the tray icon closes it either way.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, Rect, WebviewWindow, WindowEvent};

/// Window label from `tauri.conf.json`.
pub const PANEL_LABEL: &str = "panel";

/// Space between the tray icon and the panel.
const GAP: i32 = 6;
/// Keep the panel at least this far inside the screen edge.
const MARGIN: i32 = 8;
/// How soon after an auto-hide a click still counts as the same gesture.
const SAME_GESTURE: Duration = Duration::from_millis(300);
/// Used only if the window cannot report its own size.
const FALLBACK_SIZE: (f64, f64) = (340.0, 470.0);

/// What the panel remembers between clicks.
#[derive(Default)]
struct Timing {
    /// When a blur closed the panel by itself, while that is still unclaimed.
    auto_hidden_at: Option<Instant>,
    /// Whether the panel has taken focus since it was last shown.
    took_focus: bool,
}

/// Hides the panel and remembers that it was not the user's doing.
#[derive(Default)]
pub struct PanelState {
    timing: Mutex<Timing>,
}

impl PanelState {
    fn lock(&self) -> MutexGuard<'_, Timing> {
        // A panic elsewhere must not take the panel's ability to close with it.
        self.timing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn note_auto_hide(&self) {
        self.lock().auto_hidden_at = Some(Instant::now());
    }

    /// Drops any stamp, so the click that follows is read as a request to open.
    fn forget_auto_hide(&self) {
        self.lock().auto_hidden_at = None;
    }

    /// Records a fresh open. Any stamp from an earlier gesture is spent by it, and
    /// focus is the new window's to take rather than a leftover from last time.
    fn note_shown(&self) {
        let mut timing = self.lock();
        timing.auto_hidden_at = None;
        timing.took_focus = false;
    }

    /// Records that the panel has focus, which is what makes losing it mean
    /// something.
    fn note_focused(&self) {
        self.lock().took_focus = true;
    }

    /// Consumes the stamp: it describes one gesture, so it must not survive to
    /// colour a later click.
    fn took_recent_auto_hide(&self) -> bool {
        match self.lock().auto_hidden_at.take() {
            Some(at) => at.elapsed() < SAME_GESTURE,
            None => false,
        }
    }

    /// Whether the panel has held focus since it was shown, and so whether a blur
    /// is the user moving on rather than the click that opened it still being
    /// delivered.
    fn took_focus(&self) -> bool {
        self.lock().took_focus
    }
}

/// Show the panel if it is hidden, hide it if it is shown.
///
/// `tray` is the tray icon's rectangle when the click that triggered this could
/// report one; without it the window keeps the position it already had.
pub fn toggle(app: &AppHandle, tray: Option<Rect>) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    let state = app.state::<PanelState>();

    // Read visibility before consuming the stamp so the two orderings of the
    // blur/click pair both resolve the way the user meant them to.
    let visible = window.is_visible().unwrap_or(false);
    let just_auto_hidden = state.took_recent_auto_hide();

    if visible {
        hide(app);
    } else if just_auto_hidden {
        // The blur from this very click already closed it; the click was the
        // dismissal, not a request to reopen.
    } else {
        show(app, tray);
    }
}

/// The tray icon is the only thing that opens the panel in a real session, and a
/// tray click cannot be scripted, so a debug run can ask for the panel directly
/// (see `RUSTCAT_SHOW_AT_START` in `lib.rs`).
pub fn show(app: &AppHandle, tray: Option<Rect>) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    // Position first, so it never appears for a frame at the old spot.
    place(&window, tray);
    // A menu bar popover belongs to the Space the user is looking at, not to
    // whichever one happened to be current when the app started.
    let _ = window.set_visible_on_all_workspaces(true);
    // Stamped before showing: the focus change that showing causes can come back
    // as a blur before this call returns.
    app.state::<PanelState>().note_shown();
    let _ = window.show();
    // Focus is what makes click-away close it, so it is not optional.
    let _ = window.set_focus();
    log::info!("panel shown at {:?}", tray.map(|r| r.position));
    report(app, "shown");
    // `show` and `set_focus` report no error even when the window ends up off
    // screen or unfocused, so the answer is asked again once the window server
    // has had a moment to composite it.
    let later = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        report(&later, "1.5s after being shown");
    });
}

/// What the window server actually has, as opposed to what was asked for.
///
/// A panel that is ordered front but never painted is indistinguishable, from the
/// Rust side alone, from one that never opened: both leave the user looking at an
/// empty menu bar. So every step of the show path leaves a reading behind.
pub fn report(app: &AppHandle, when: &str) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        log::warn!("panel {when}: the window is gone");
        return;
    };
    log::info!(
        "panel {when}: visible={:?} focused={:?} at={:?} size={:?} scale={:?}",
        window.is_visible(),
        window.is_focused(),
        window.outer_position(),
        window.outer_size(),
        window.scale_factor(),
    );
}

/// Hide the panel.
///
/// A deliberate hide leaves no auto-hide stamp behind: ordering the panel out
/// makes it resign key, and that blur is the window going away rather than the
/// user dismissing it. Stamping it would swallow the next click, so a second
/// click on the tray icon would do nothing.
pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PANEL_LABEL) {
        let _ = window.hide();
        app.state::<PanelState>().forget_auto_hide();
        log::info!("panel hidden");
    }
}

/// The window lost focus: treat it as a click-away and close it.
fn hide_losing_focus(app: &AppHandle) {
    let state = app.state::<PanelState>();

    // A panel that has not taken focus yet is still being opened, and this blur
    // belongs to the click that opened it — hiding now is what makes a click look
    // like it did nothing at all.
    if !state.took_focus() {
        log::info!("panel blurred before it took focus; leaving it open");
        return;
    }

    hide(app);
    // Stamped after the hide, so the stamp that hiding the panel would otherwise
    // leave (it resigns key on the way out) is gone and this one, the real
    // dismissal, is the one the click that follows will read.
    state.note_auto_hide();
    log::info!("panel dismissed by losing focus");
}

/// Wire the panel's close-on-blur and close-instead-of-quit behaviour.
pub fn attach(window: &WebviewWindow, app: &AppHandle) {
    let app = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(true) => {
            // Logged because "the panel never took focus" is the difference
            // between a click that opens nothing and one that opens and closes.
            log::info!("panel took focus");
            app.state::<PanelState>().note_focused();
        }
        WindowEvent::Focused(false) => {
            log::info!("panel lost focus");
            hide_losing_focus(&app);
        }
        WindowEvent::CloseRequested { api, .. } => {
            // Closing is hiding: the tray icon is the app, and the panel is a
            // popover. Quitting goes through the tray menu.
            api.prevent_close();
            hide(&app);
        }
        _ => {}
    });
}

/// Move the panel next to the tray icon.
///
/// Does nothing when the icon's rectangle is unknown — the icon is drawn by the
/// platform and the rectangle can genuinely be unavailable, and leaving the
/// window where it was beats moving it somewhere arbitrary.
fn place(window: &WebviewWindow, tray: Option<Rect>) {
    let Some(rect) = tray else {
        return;
    };
    let Ok(scale) = window.scale_factor() else {
        return;
    };

    let icon = rect.size.to_physical::<u32>(scale);
    let icon_at = rect.position.to_physical::<i32>(scale);
    let icon = Bounds::new(icon_at.x, icon_at.y, icon.width as i32, icon.height as i32);

    let panel = match window.outer_size() {
        Ok(size) => (size.width as i32, size.height as i32),
        Err(_) => (
            (FALLBACK_SIZE.0 * scale) as i32,
            (FALLBACK_SIZE.1 * scale) as i32,
        ),
    };

    // The screen the icon is on, falling back to the primary one.
    let screen = window
        .monitor_from_point(icon.center_x() as f64, icon.center_y() as f64)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .map(|m| {
            let at = m.position();
            let size = m.size();
            Bounds::new(at.x, at.y, size.width as i32, size.height as i32)
        });

    let (x, y) = match screen {
        Some(screen) => anchor(icon, panel, screen),
        // No screen to clamp against: still anchor to the icon.
        None => (icon.center_x() - panel.0 / 2, icon.bottom() + GAP),
    };

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// An axis-aligned rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bounds {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Bounds {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn right(&self) -> i32 {
        self.x + self.w
    }

    fn bottom(&self) -> i32 {
        self.y + self.h
    }

    fn center_x(&self) -> i32 {
        self.x + self.w / 2
    }

    fn center_y(&self) -> i32 {
        self.y + self.h / 2
    }
}

/// Where to put a `panel`-sized window anchored to `icon`, inside `screen`.
///
/// Which side of the icon the panel goes on is decided by which half of the
/// screen the icon is in: below an icon near the top (the macOS menu bar), above
/// one near the bottom (the default Windows taskbar). That also lands correctly
/// for a taskbar docked to the top or side, and names no platform to do it.
fn anchor(icon: Bounds, panel: (i32, i32), screen: Bounds) -> (i32, i32) {
    let (w, h) = panel;
    let below = icon.center_y() < screen.y + screen.h / 2;

    let x = clamp(
        icon.center_x() - w / 2,
        screen.x + MARGIN,
        screen.right() - MARGIN - w,
    );
    let y = if below {
        clamp(
            icon.bottom() + GAP,
            screen.y + MARGIN,
            screen.bottom() - MARGIN - h,
        )
    } else {
        clamp(
            icon.y - GAP - h,
            screen.y + MARGIN,
            screen.bottom() - MARGIN - h,
        )
    };
    (x, y)
}

/// Clamp that survives a screen smaller than the panel.
///
/// `i32::clamp` panics when `min > max`, which is exactly what a tiny or
/// heavily-scaled screen produces here; raising `max` to `min` degrades to the
/// lowest position instead.
fn clamp(v: i32, min: i32, max: i32) -> i32 {
    v.clamp(min, max.max(min))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920x1080 screen at the origin.
    const SCREEN: Bounds = Bounds {
        x: 0,
        y: 0,
        w: 1920,
        h: 1080,
    };
    const PANEL: (i32, i32) = (340, 470);

    #[test]
    fn menu_bar_icon_puts_the_panel_below_it() {
        // A macOS status item: 24x24 at the very top, near the right edge.
        let icon = Bounds::new(1600, 0, 24, 24);
        let (x, y) = anchor(icon, PANEL, SCREEN);
        assert_eq!(y, 24 + GAP, "panel top edge sits just under the icon");
        assert_eq!(x, 1600 + 12 - 170, "panel is centred on the icon");
    }

    #[test]
    fn taskbar_icon_puts_the_panel_above_it() {
        // A Windows notification-area icon in a 40px bottom taskbar.
        let icon = Bounds::new(1800, 1040, 16, 16);
        let (x, y) = anchor(icon, PANEL, SCREEN);
        assert_eq!(y, 1040 - GAP - 470, "panel bottom edge sits above the icon");
        assert_eq!(
            x,
            SCREEN.right() - MARGIN - PANEL.0,
            "the notification area is too far right to centre on, so the panel \
             is pulled back inside the screen instead of hanging off the edge"
        );
    }

    #[test]
    fn panel_is_kept_inside_the_screen() {
        // An icon hard against the top-left corner.
        let (x, y) = anchor(Bounds::new(0, 0, 24, 24), PANEL, SCREEN);
        assert_eq!((x, y), (MARGIN, 24 + GAP));
    }

    #[test]
    fn panel_is_kept_inside_a_second_monitor_placed_to_the_right() {
        let screen = Bounds::new(1920, 0, 1920, 1080);
        // An icon at that screen's left edge would put the panel off-screen.
        let (x, _) = anchor(Bounds::new(1920, 0, 24, 24), PANEL, screen);
        assert_eq!(
            x,
            1920 + MARGIN,
            "clamped to the second monitor's left edge"
        );
    }

    #[test]
    fn a_screen_smaller_than_the_panel_does_not_panic() {
        let screen = Bounds::new(0, 0, 200, 200);
        let (x, y) = anchor(Bounds::new(0, 0, 16, 16), PANEL, screen);
        assert_eq!(
            (x, y),
            (MARGIN, MARGIN),
            "there is no room for the panel on either axis, so it lands in the \
             screen's top-left corner rather than at a nonsense position"
        );
    }

    #[test]
    fn auto_hide_stamp_is_recent_once_and_then_spent() {
        let state = PanelState::default();
        assert!(!state.took_recent_auto_hide(), "nothing stamped yet");

        state.note_auto_hide();
        assert!(
            state.took_recent_auto_hide(),
            "the stamp describes one gesture"
        );
        assert!(
            !state.took_recent_auto_hide(),
            "and must not colour a second click"
        );
    }

    #[test]
    fn showing_spends_an_older_stamp() {
        let state = PanelState::default();
        state.note_auto_hide();
        state.note_shown();
        assert!(
            !state.took_recent_auto_hide(),
            "the gesture that stamp described is over, so the next click opens"
        );
    }

    #[test]
    fn a_blur_before_the_panel_takes_focus_is_not_a_dismissal() {
        let state = PanelState::default();

        state.note_shown();
        assert!(
            !state.took_focus(),
            "the panel is still opening, so a blur now belongs to the click that \
             opened it rather than to the user moving on"
        );

        state.note_focused();
        assert!(state.took_focus(), "now losing focus means something");
    }

    #[test]
    fn showing_again_waits_for_its_own_focus() {
        let state = PanelState::default();
        state.note_shown();
        state.note_focused();

        state.note_shown();
        assert!(
            !state.took_focus(),
            "focus from the previous time the panel was up must not be read as \
             this one's, or the blur still on its way would close it again"
        );
    }

    #[test]
    fn a_deliberate_hide_leaves_no_stamp_behind() {
        let state = PanelState::default();
        state.note_auto_hide();

        state.forget_auto_hide();
        assert!(
            !state.took_recent_auto_hide(),
            "the panel was still up when the click landed, so the next click is a \
             request to open rather than a dismissal that already happened"
        );
    }
}
