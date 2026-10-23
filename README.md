# rustcat-tauri

A cat that runs in your system tray, and runs faster the harder your machine is
working.

Built with [Tauri 2](https://tauri.app). There is no main window — the tray icon
is the app. Its pace follows a live system metric, read through
[`rustcat-metrics`](../rustcat-metrics); left-clicking the cat opens a small
always-on-top panel with the readings behind the animation.

```
        CPU  ≤5%  →  500 ms a frame   (the floor)
        CPU  50%  →   50 ms a frame
        CPU 100%  →   25 ms a frame   (40 fps)
```

## Requirements

- Rust 1.90+ (`rustup`)
- macOS: Xcode command line tools
- Windows: MSVC build tools, WebView2
- The `rustcat-metrics` crate checked out as a **sibling** directory — the build
  reaches it through the relative path `../../rustcat-metrics`

## Run it

```sh
cargo run --manifest-path src-tauri/Cargo.toml
```

To see the panel without hunting for the tray icon:

```sh
RUSTCAT_SHOW_AT_START=1 cargo run --manifest-path src-tauri/Cargo.toml
```

Release bundle:

```sh
cargo tauri build     # or: cargo build --release --manifest-path src-tauri/Cargo.toml
```

## Using it as a library

The tray cat is also a Rust library, so it can live inside a larger app instead
of owning `main`:

```rust
use rustcat_tauri::{RustCat, SpeedMetric};
use std::time::Duration;

RustCat::builder()
    .speed_metric(SpeedMetric::Cpu)
    .sample_interval(Duration::from_secs(2))
    .idle_interval(Duration::from_millis(500))
    .tooltip("RustCat")
    .quit_label("Quit RustCat")
    .run()?;
```

`run` blocks until the app exits, so call it from the main thread.

## Layout

| Path | What it is |
| --- | --- |
| `src-tauri/src/lib.rs` | `RustCat` — the library's entry point |
| `src-tauri/src/config.rs` | `Config`, `RustCatBuilder`, `SpeedMetric` |
| `src-tauri/src/pace.rs` | `Pacer` — the metric, the 5-sample average, the interval |
| `src-tauri/src/tray.rs` | The tray icon, its menu, and the animation loop |
| `src-tauri/src/cat.rs` | The five frames, black and white palettes |
| `src-tauri/src/panel.rs` | The always-on-top panel and how it closes |
| `src-tauri/src/main.rs` | The demo bin — `cargo run` launches this |
| `dist/index.html` | The panel's page, a single static file |
| `src-tauri/README.md` | **The library API in full** |

## Docs

- [`src-tauri/README.md`](src-tauri/README.md) — settings, the pace formula, the
  `SpeedMetric` extension point, platform support
- [`rustcat-metrics`](../rustcat-metrics) — the readings, and how to publish your
  own alongside them

## License

MIT — see [`src-tauri/LICENSE`](src-tauri/LICENSE).
