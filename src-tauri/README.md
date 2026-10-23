# rustcat-tauri

A cat that runs in the system tray, its pace set by a system metric.

There is no main window: the tray icon *is* the app. The cat runs faster as the
metric climbs and drops back to a slow idle when the machine is quiet. A left
click opens a small always-on-top panel with the readings behind the animation.

Readings come from [`rustcat-metrics`](../rustcat-metrics), so any metric that
crate can publish is a candidate pace source.

## Running the demo

```sh
cargo run
```

That runs [`src/main.rs`](src/main.rs) — about ten lines that call
`rustcat_tauri::run()`, the library's default cat. To see the panel without
clicking the tray icon:

```sh
RUSTCAT_SHOW_AT_START=1 cargo run
```

(Debug builds only; the variable is ignored in release.)

## Using it as a library

The library gives you the tray icon without giving up `main`:
`RustCat::builder()` describes the cat, and `run` starts it when you are ready.

```rust
use rustcat_tauri::{RustCat, SpeedMetric};
use std::time::Duration;

RustCat::builder()
    .speed_metric(SpeedMetric::Cpu)
    .sample_interval(Duration::from_secs(2))
    .idle_interval(Duration::from_millis(500))
    .tooltip("RustCat")
    .quit_label("Quit RustCat")
    .run()
    .expect("RustCat could not start");
```

`run` blocks until the app exits, so it must be called from the main thread —
`RustCatBuilder::build` gives you the value without starting anything, and
`RustCat::run` starts it at the point the caller is ready.

Every setting has a default, so `rustcat_tauri::run()` and the builder above
produce the same cat.

## Settings

| Builder method | Default | What it does |
| --- | --- | --- |
| `speed_metric` | `SpeedMetric::Cpu` | Which metric paces the cat |
| `sample_interval` | 1s | How often the metric is read (clamped to 50ms minimum) |
| `idle_interval` | 500ms | Frame interval when no metric is readable (clamped to 1ms minimum) |
| `tooltip` | `"RustCat"` | Tray tooltip and the prefix of the live status line |
| `quit_label` | `"Quit RustCat"` | The tray menu's quit item |

The two intervals are clamped rather than rejected: a zero `sample_interval`
would read the metric on every loop turn and peg a core, and a caller computing
an interval from configuration should not be able to take the app down.

## Choosing the pace source

`SpeedMetric` names the reading that sets the pace, and is the extension point
for pacing off something else:

```rust
use rustcat_tauri::SpeedMetric;

assert_eq!(SpeedMetric::Cpu.metric_name(), "cpu.global_usage");
assert_eq!(SpeedMetric::Cpu.label(), "CPU");
```

`metric_name` must match a reading `rustcat-metrics` actually publishes — a name
nothing publishes would make every sample fall back to the idle interval, and
the cat would sit still while the menu row showed `CPU 0%`. The crate's own tests
assert that seam, so a rename on either side fails the build rather than
producing a motionless cat.

CPU is currently the only variant. Adding another means adding a variant, its
name, and its label; the sampling, averaging and pacing are shared.

## How the pace is computed

`speed.rs` holds the pure part, matched against RunCat365's `CalculateInterval`:

```text
speed    = max(1.0, load / 5.0)   // a fixed 40fps tier
interval = (500.0 / speed) as u64 // milliseconds
```

So an idle machine runs a frame every 500ms and a pegged one every ~25ms, and
the reading is averaged over a 5-sample window first so a single spike does not
lurch the animation. Readings are clamped to 0–100, and a missing, non-numeric
or NaN reading idles rather than stopping the cat or panicking.

## What the tray shows

- The icon: the cat, from the five frames compiled in from `assets/runners/cat/`.
  macOS gets a template image and lets the OS tint it; Windows reads its own
  theme setting and picks the black or white palette to match.
- The menu: a row named after the pace metric with its current value, a row with
  memory used as a percentage and in GB, and the quit item.
- The panel, on left click: the same readings, positioned against the tray icon
  on whichever monitor the icon is on. It closes on Escape, on blur, or on a
  second tray click (a click within 300ms of the panel appearing is treated as
  the same gesture and ignored, so the opening click cannot immediately close it).

## Platform support

The tray app builds for macOS and Windows, which is what its CI covers. The
library types are `#[cfg_attr(mobile, tauri::mobile_entry_point)]`-annotated, but
the tray and panel behaviour is written for desktop.

## License

MIT — see [LICENSE](LICENSE).
