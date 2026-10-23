# Cat animation frames

`cat_0.png` … `cat_4.png` are the five frames of the running cat, 32×32 RGBA.

`cat_white_0.png` … `cat_white_4.png` are the same five frames with RGB forced
to white, for use on a dark Windows taskbar (see below). They are derived, not
copied: each is the matching `cat_N.png` with every pixel's RGB replaced by
`255,255,255` and the alpha channel left bit-identical. Regenerate them with
`python3 make_white.py .` from this directory (pure-stdlib PNG decode/re-encode)
if the art is ever replaced.

## Provenance

Copied unmodified from [RustCat](https://github.com/31i45/RustCat) by 31i45 —
`assets/runners/cat/` — which is itself "a faithful Rust rewrite of
[RunCat365](https://github.com/kyome22/RunCat365) by [kyome](https://github.com/kyome22)".
**The original concept and the cat artwork belong to kyome, under the MIT License.**

`LICENSE` in this directory is RustCat's MIT license, reproduced to carry the
attribution forward.

## How the frames are drawn

Each frame is a pure-black image whose shape lives entirely in the **alpha
channel** (every visible pixel is RGB `0,0,0` with a varying alpha). That makes
the art theme-agnostic in two different ways:

- **macOS** — the tray icon is set as a *template* image, so the system re-tints
  it (white on a dark menu bar, black on a light one). The RGB values are
  ignored entirely; only alpha matters.
- **Windows** — there is no template concept, so the app picks a palette
  itself: `cat_N.png` as-is for a light taskbar, `cat_white_N.png` for a dark
  one. See `src/theme.rs` and `src/cat.rs`.
