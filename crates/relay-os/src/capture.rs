//! Screenshot capture, downscaling and PNG encoding on top of [`Desktop`].

use image::imageops::FilterType;
use image::{ImageBuffer, ImageEncoder, Rgba};

use crate::scaling::{fit_size, Budget, DisplayGeometry};
use crate::types::{Desktop, DisplayInfo, Frame, OsError, Rect, Result};

/// An encoded screenshot as the model sees it.
#[derive(Debug, Clone)]
pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn to_image(frame: Frame) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    ImageBuffer::from_raw(frame.width, frame.height, frame.rgba)
        .ok_or_else(|| OsError::Platform("frame buffer size mismatch".into()))
}

fn encode(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| OsError::Platform(format!("png encode: {e}")))?;
    Ok(out)
}

fn shrink(frame: Frame, budget: &Budget) -> Result<Shot> {
    let (tw, th) = fit_size(frame.width, frame.height, budget);
    let mut img = to_image(frame)?;
    if (tw, th) != (img.width(), img.height()) {
        img = image::imageops::resize(&img, tw, th, FilterType::Triangle);
    }
    Ok(Shot {
        png: encode(&img)?,
        width: img.width(),
        height: img.height(),
    })
}

/// Screenshot of a whole display within `budget`.
pub fn screenshot(desktop: &dyn Desktop, display: &DisplayInfo, budget: &Budget) -> Result<Shot> {
    budget.validate()?;
    shrink(desktop.capture(display.rect)?, budget)
}

/// Zoom into an inclusive screenshot-space region, captured at full resolution and fitted to the budget.
pub fn zoom(
    desktop: &dyn Desktop,
    geometry: &DisplayGeometry,
    region: [i64; 4],
    budget: &Budget,
) -> Result<Shot> {
    let rect: Rect = geometry.region_to_physical(region)?;
    shrink(desktop.capture(rect)?, budget)
}

/// Small thumbnail used to detect whether the screen changed.
pub fn thumbnail(desktop: &dyn Desktop, rect: Rect, edge: u32) -> Result<Frame> {
    let frame = desktop.capture(rect)?;
    let (tw, th) = fit_size(
        frame.width,
        frame.height,
        &Budget {
            max_edge: edge.max(64),
            max_pixels: u32::MAX,
        },
    );
    let img = image::imageops::resize(&to_image(frame)?, tw, th, FilterType::Triangle);
    Ok(Frame {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

/// Number of pixels whose colour channels differ by more than `threshold` between two frames.
/// Frames of different sizes count as entirely changed.
pub fn changed_pixels(a: &Frame, b: &Frame, threshold: u8) -> usize {
    if a.width != b.width || a.height != b.height {
        return a.rgba.len().max(b.rgba.len()) / 4;
    }
    a.rgba
        .chunks_exact(4)
        .zip(b.rgba.chunks_exact(4))
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .take(3)
                .any(|(x, y)| x.abs_diff(*y) > threshold)
        })
        .count()
}

/// Fraction (0..=1) of pixels that changed; see [`changed_pixels`].
pub fn changed_fraction(a: &Frame, b: &Frame, threshold: u8) -> f64 {
    let total = (a.rgba.len().max(b.rgba.len()) / 4).max(1);
    changed_pixels(a, b, threshold) as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeDesktop;

    #[test]
    fn screenshot_is_a_png_within_budget() {
        let fake = FakeDesktop::new();
        let display = fake.displays().unwrap().remove(0);
        let shot = screenshot(&fake, &display, &Budget::default()).unwrap();
        assert_eq!(&shot.png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!((shot.width, shot.height), (1568, 882));
    }

    #[test]
    fn zoom_fits_the_budget_and_returns_a_png() {
        let fake = FakeDesktop::new();
        let display = fake.displays().unwrap().remove(0);
        let geo = DisplayGeometry::new(display.rect, &Budget::default()).unwrap();
        let shot = zoom(&fake, &geo, [0, 0, 199, 99], &Budget::default()).unwrap();
        assert_eq!(&shot.png[..4], b"\x89PNG");
        assert!(shot.width <= 1568);
        assert!(zoom(&fake, &geo, [0, 0, 5000, 9], &Budget::default()).is_err());
    }

    #[test]
    fn change_detection_ignores_identical_and_flags_different_frames() {
        let fake = FakeDesktop::new();
        let rect = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };
        let a = thumbnail(&fake, rect, 100).unwrap();
        let same = thumbnail(&fake, rect, 100).unwrap();
        assert_eq!(changed_fraction(&a, &same, 24), 0.0);
        fake.set_shade(200);
        let b = thumbnail(&fake, rect, 100).unwrap();
        assert!(changed_fraction(&a, &b, 24) > 0.9);
    }

    #[test]
    fn mismatched_frames_count_as_changed() {
        let a = Frame {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        let b = Frame {
            width: 2,
            height: 1,
            rgba: vec![0; 8],
        };
        assert_eq!(changed_fraction(&a, &b, 24), 1.0);
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    fn blank(w: u32, h: u32) -> Frame {
        Frame {
            width: w,
            height: h,
            rgba: vec![255; (w * h * 4) as usize],
        }
    }

    #[test]
    fn a_small_text_sized_patch_is_counted() {
        let a = blank(3440, 1440);
        let mut b = a.clone();
        // Dark 130x16 patch, like a short line of typed text.
        for y in 100..116u32 {
            for x in 200..330u32 {
                let i = ((y * 3440 + x) * 4) as usize;
                b.rgba[i] = 0;
                b.rgba[i + 1] = 0;
                b.rgba[i + 2] = 0;
            }
        }
        assert_eq!(changed_pixels(&a, &b, 24), 130 * 16);
        assert_eq!(changed_pixels(&a, &a, 24), 0);
    }

    #[test]
    fn faint_differences_below_the_threshold_are_ignored() {
        let a = blank(10, 10);
        let mut b = a.clone();
        b.rgba[0] = 240;
        assert_eq!(changed_pixels(&a, &b, 24), 0);
        assert_eq!(changed_pixels(&a, &b, 10), 1);
    }
}
