//! The running cat: the five animation frames, decoded once at startup.
//!
//! The art is a pure-black alpha mask (every pixel has RGB `0,0,0`; the shape
//! lives entirely in the alpha channel), which is why two palettes exist:
//!
//! - **macOS** sets the tray icon as a *template* image, so the system re-tints
//!   it (white on a dark menu bar, black on a light one) and RGB is ignored.
//!   The black set is fine.
//! - **Windows** has no template concept, so the visible colour is whatever the
//!   PNG says. The taskbar can be light or dark, so [`theme::dark_taskbar`]
//!   picks between the black and white sets.
//!
//! The white frames are derived, not authored: each is its black counterpart
//! with RGB forced to 255 and alpha left bit-identical. See
//! `assets/runners/cat/README.md`, and `make_white.py` beside it.
//!
//! [`theme::dark_taskbar`]: crate::theme::dark_taskbar

use tauri::image::Image;

/// Number of frames in the run cycle, and the length of every array below.
pub const FRAME_COUNT: usize = 5;

/// Frames as shipped: black, for a light taskbar (and for macOS, where the
/// template treatment ignores RGB anyway).
static BLACK: [&[u8]; FRAME_COUNT] = [
    include_bytes!("../assets/runners/cat/cat_0.png"),
    include_bytes!("../assets/runners/cat/cat_1.png"),
    include_bytes!("../assets/runners/cat/cat_2.png"),
    include_bytes!("../assets/runners/cat/cat_3.png"),
    include_bytes!("../assets/runners/cat/cat_4.png"),
];

/// The same frames with RGB forced to white, for a dark taskbar.
static WHITE: [&[u8]; FRAME_COUNT] = [
    include_bytes!("../assets/runners/cat/cat_white_0.png"),
    include_bytes!("../assets/runners/cat/cat_white_1.png"),
    include_bytes!("../assets/runners/cat/cat_white_2.png"),
    include_bytes!("../assets/runners/cat/cat_white_3.png"),
    include_bytes!("../assets/runners/cat/cat_white_4.png"),
];

/// One decoded frame, held as raw RGBA.
///
/// The pixels are kept rather than a ready-made [`Image`] because `Image`'s
/// accessor is `fn rgba(&'a self) -> &'a [u8]` on `impl<'a> Image<'a>`: on a
/// stored `Image<'static>` that demands a `&'static self` borrow, so the pixels
/// of a stored image cannot be read back at all. Keeping the bytes lets the
/// palette invariant stay under test; building the `Image` on demand costs one
/// 4 KB allocation per frame, which is noise next to handing the icon to the
/// platform.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Frame {
    fn decode(png: &[u8]) -> tauri::Result<Self> {
        let image = Image::from_bytes(png)?;
        Ok(Self {
            width: image.width(),
            height: image.height(),
            pixels: image.rgba().to_vec(),
        })
    }

    /// The frame as a tray-icon-ready image. Owned, so it can outlive `self`.
    pub fn to_image(&self) -> Image<'static> {
        Image::new_owned(self.pixels.clone(), self.width, self.height)
    }

    /// The raw RGBA of the frame, and its size. Both exist so the tests can
    /// assert on the pixels that actually reach the tray, which is otherwise
    /// only observable through the platform.
    #[cfg(test)]
    fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[cfg(test)]
    fn width(&self) -> u32 {
        self.width
    }

    #[cfg(test)]
    fn height(&self) -> u32 {
        self.height
    }
}

/// Both palettes, decoded once.
///
/// Decoding up front matters because the animation tick can be as short as
/// 25 ms; a PNG decode per frame would be pure waste.
pub struct Frames {
    black: [Frame; FRAME_COUNT],
    white: [Frame; FRAME_COUNT],
}

impl Frames {
    /// Decode both palettes. Fails only if a bundled PNG is malformed, which is
    /// a build error rather than a runtime condition.
    pub fn load() -> tauri::Result<Self> {
        Ok(Self {
            black: decode_all(BLACK)?,
            white: decode_all(WHITE)?,
        })
    }

    /// The frame to draw, wrapped into range so callers can increment an index
    /// forever without caring about `FRAME_COUNT`.
    pub fn frame(&self, index: usize, dark_taskbar: bool) -> &Frame {
        let set = if dark_taskbar {
            &self.white
        } else {
            &self.black
        };
        &set[index % FRAME_COUNT]
    }
}

fn decode_all(pngs: [&'static [u8]; FRAME_COUNT]) -> tauri::Result<[Frame; FRAME_COUNT]> {
    // `Image::from_bytes` needs the `image-png` feature.
    let mut frames: [Option<Frame>; FRAME_COUNT] = Default::default();
    for (slot, png) in frames.iter_mut().zip(pngs) {
        *slot = Some(Frame::decode(png)?);
    }
    Ok(frames.map(|f| f.expect("every slot is filled by the loop above")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tauri rejects a tray icon whose RGBA buffer disagrees with its declared
    /// size, and a mismatched frame would only surface at runtime.
    #[test]
    fn every_frame_decodes_to_a_consistent_32x32() {
        let frames = Frames::load().expect("bundled frames must decode");
        for dark in [false, true] {
            for i in 0..FRAME_COUNT {
                let f = frames.frame(i, dark);
                assert_eq!((f.width(), f.height()), (32, 32), "frame {i} dark={dark}");
                assert_eq!(
                    f.pixels().len(),
                    32 * 32 * 4,
                    "frame {i} dark={dark}: RGBA8 buffer size"
                );
            }
        }
    }

    #[test]
    fn index_wraps_around() {
        let frames = Frames::load().expect("bundled frames must decode");
        for i in 0..FRAME_COUNT {
            assert_eq!(
                frames.frame(i, false),
                frames.frame(i + FRAME_COUNT, false),
                "frame {i} and {i} + FRAME_COUNT must be the same frame"
            );
        }
    }

    /// The white set is derived from the black one by rewriting RGB only, so
    /// this is the invariant that breaks if the generator is ever run wrong.
    /// It also pins the direction: the shipped frames stay black.
    #[test]
    fn white_frames_differ_only_in_rgb() {
        let frames = Frames::load().expect("bundled frames must decode");
        for i in 0..FRAME_COUNT {
            let black = frames.frame(i, false);
            let white = frames.frame(i, true);
            assert_eq!(
                black.pixels().len(),
                white.pixels().len(),
                "frame {i}: the two palettes must agree on size"
            );

            let black_px = black.pixels().as_chunks::<4>().0;
            let white_px = white.pixels().as_chunks::<4>().0;

            for (px, (b, w)) in black_px.iter().zip(white_px).enumerate() {
                assert_eq!(b[3], w[3], "frame {i} px {px}: alpha must be identical");
                assert_eq!(&b[..3], &[0, 0, 0], "frame {i} px {px}: black RGB");
                assert_eq!(&w[..3], &[255, 255, 255], "frame {i} px {px}: white RGB");
            }
        }
    }

    /// The frames have to actually differ, or the cat would stand still.
    #[test]
    fn the_five_frames_are_distinct() {
        let frames = Frames::load().expect("bundled frames must decode");
        for i in 0..FRAME_COUNT {
            for j in (i + 1)..FRAME_COUNT {
                assert_ne!(
                    frames.frame(i, false),
                    frames.frame(j, false),
                    "frames {i} and {j} are identical"
                );
            }
        }
    }
}
