//! Pure geometry and pixel-format helpers for the macOS desktop (testable on any OS).
//!
//! macOS places displays in a global space measured in points, while screenshots are in backing
//! pixels (a Retina display has two pixels per point). Relay's contract is that every rectangle
//! and coordinate is in pixels, so the macOS layer builds a pixel layout:
//!
//! * each display keeps its own pixel size (backing pixels, so captures are full resolution);
//! * each display's origin is its point origin multiplied by the largest scale of any display.
//!
//! Scaling every origin by the same factor `S >= s_i` keeps displays that do not overlap in
//! points from overlapping in pixels, even when their scale factors differ. Conversions always go
//! through one display, so mixed-scale setups stay exact inside each display.

use crate::types::{DisplayInfo, OsError, Rect, Result};

/// Rectangle in macOS global points (origin top-left of the main display, y grows down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PtRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PtRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }

    /// Squared distance from a point to the rectangle (0 inside).
    fn distance2(&self, x: f64, y: f64) -> f64 {
        let dx = (self.x - x).max(0.0).max(x - (self.x + self.w));
        let dy = (self.y - y).max(0.0).max(y - (self.y + self.h));
        dx * dx + dy * dy
    }
}

/// One display as macOS reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    /// `CGDirectDisplayID`.
    pub cg_id: u32,
    /// Bounds in global points.
    pub points: PtRect,
    /// Backing size in pixels.
    pub pixel_w: u32,
    pub pixel_h: u32,
    pub primary: bool,
}

impl Screen {
    /// Backing pixels per point (2.0 on a Retina display).
    pub fn scale(&self) -> f64 {
        if self.points.w > 0.0 {
            f64::from(self.pixel_w) / self.points.w
        } else {
            1.0
        }
    }
}

/// The pixel layout of every active display.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Primary first, then left to right, top to bottom (same order as Windows).
    screens: Vec<Screen>,
    /// Largest scale factor of any display; multiplies every origin.
    unit: f64,
}

fn rect_contains_f(r: &Rect, x: f64, y: f64) -> bool {
    x >= f64::from(r.x)
        && y >= f64::from(r.y)
        && x < f64::from(r.x) + f64::from(r.w)
        && y < f64::from(r.y) + f64::from(r.h)
}

impl Layout {
    pub fn new(mut screens: Vec<Screen>) -> Result<Self> {
        screens.retain(|s| s.points.w > 0.0 && s.points.h > 0.0 && s.pixel_w > 0 && s.pixel_h > 0);
        if screens.is_empty() {
            return Err(OsError::Platform("no displays found".into()));
        }
        screens.sort_by(|a, b| {
            (!a.primary, a.points.x, a.points.y)
                .partial_cmp(&(!b.primary, b.points.x, b.points.y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let unit = screens.iter().map(Screen::scale).fold(1.0_f64, f64::max);
        Ok(Self { screens, unit })
    }

    #[cfg(test)]
    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    /// Pixel rectangle of screen `i`.
    pub fn pixel_rect(&self, i: usize) -> Rect {
        let s = &self.screens[i.min(self.screens.len() - 1)];
        Rect {
            x: (s.points.x * self.unit).round() as i32,
            y: (s.points.y * self.unit).round() as i32,
            w: s.pixel_w,
            h: s.pixel_h,
        }
    }

    /// Displays in Relay's form (ids 1.., primary first).
    pub fn displays(&self) -> Vec<DisplayInfo> {
        (0..self.screens.len())
            .map(|i| DisplayInfo {
                id: i as u32 + 1,
                rect: self.pixel_rect(i),
                primary: self.screens[i].primary,
            })
            .collect()
    }

    /// Bounding rectangle of every display, in pixels.
    pub fn virtual_rect(&self) -> Rect {
        let rects: Vec<Rect> = (0..self.screens.len())
            .map(|i| self.pixel_rect(i))
            .collect();
        let x0 = rects.iter().map(|r| i64::from(r.x)).min().unwrap_or(0);
        let y0 = rects.iter().map(|r| i64::from(r.y)).min().unwrap_or(0);
        let x1 = rects
            .iter()
            .map(|r| i64::from(r.x) + i64::from(r.w))
            .max()
            .unwrap_or(0);
        let y1 = rects
            .iter()
            .map(|r| i64::from(r.y) + i64::from(r.h))
            .max()
            .unwrap_or(0);
        Rect {
            x: x0 as i32,
            y: y0 as i32,
            w: (x1 - x0).max(0) as u32,
            h: (y1 - y0).max(0) as u32,
        }
    }

    /// Screen whose pixel rectangle contains the point, else the nearest one.
    fn screen_for_pixel(&self, x: f64, y: f64) -> usize {
        let mut best = (0, f64::INFINITY);
        for i in 0..self.screens.len() {
            let r = self.pixel_rect(i);
            if rect_contains_f(&r, x, y) {
                return i;
            }
            let pr = PtRect {
                x: f64::from(r.x),
                y: f64::from(r.y),
                w: f64::from(r.w),
                h: f64::from(r.h),
            };
            let d = pr.distance2(x, y);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    }

    /// Screen whose point rectangle contains the point, else the nearest one.
    fn screen_for_point(&self, x: f64, y: f64) -> usize {
        let mut best = (0, f64::INFINITY);
        for (i, s) in self.screens.iter().enumerate() {
            if s.points.contains(x, y) {
                return i;
            }
            let d = s.points.distance2(x, y);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    }

    /// Pixel position to global points (for posting events). Points outside every display are
    /// clamped onto the nearest one.
    pub fn to_point(&self, x: i32, y: i32) -> (f64, f64) {
        // Aim at the pixel centre, like the Windows path.
        let (fx, fy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let i = self.screen_for_pixel(fx, fy);
        let s = &self.screens[i];
        let r = self.pixel_rect(i);
        let scale = s.scale();
        let lx = (fx - f64::from(r.x)).clamp(0.0, f64::from(r.w) - 0.5);
        let ly = (fy - f64::from(r.y)).clamp(0.0, f64::from(r.h) - 0.5);
        (s.points.x + lx / scale, s.points.y + ly / scale)
    }

    /// Global points to a pixel position.
    pub fn to_pixel(&self, x: f64, y: f64) -> (i32, i32) {
        let i = self.screen_for_point(x, y);
        let s = &self.screens[i];
        let r = self.pixel_rect(i);
        let scale = s.scale();
        let px = f64::from(r.x) + (x - s.points.x) * scale;
        let py = f64::from(r.y) + (y - s.points.y) * scale;
        (px.floor() as i32, py.floor() as i32)
    }

    /// A rectangle in global points (a window) as pixels, scaled by the display under its centre.
    pub fn rect_to_pixels(&self, pt: PtRect) -> Rect {
        let i = self.screen_for_point(pt.x + pt.w / 2.0, pt.y + pt.h / 2.0);
        let s = &self.screens[i];
        let r = self.pixel_rect(i);
        let scale = s.scale();
        Rect {
            x: (f64::from(r.x) + (pt.x - s.points.x) * scale).round() as i32,
            y: (f64::from(r.y) + (pt.y - s.points.y) * scale).round() as i32,
            w: (pt.w.max(0.0) * scale).round() as u32,
            h: (pt.h.max(0.0) * scale).round() as u32,
        }
    }

    /// Which display a capture of `rect` (pixels) comes from, and the display-local point
    /// rectangle to ask macOS for. The rectangle must lie inside one display.
    pub fn capture_region(&self, rect: Rect) -> Result<(Screen, PtRect)> {
        if rect.w == 0 || rect.h == 0 {
            return Err(OsError::InvalidArgument(
                "capture rectangle is empty".into(),
            ));
        }
        for i in 0..self.screens.len() {
            let r = self.pixel_rect(i);
            let inside = i64::from(rect.x) >= i64::from(r.x)
                && i64::from(rect.y) >= i64::from(r.y)
                && i64::from(rect.x) + i64::from(rect.w) <= i64::from(r.x) + i64::from(r.w)
                && i64::from(rect.y) + i64::from(rect.h) <= i64::from(r.y) + i64::from(r.h);
            if inside {
                let s = self.screens[i];
                let scale = s.scale();
                return Ok((
                    s,
                    PtRect {
                        x: f64::from(rect.x - r.x) / scale,
                        y: f64::from(rect.y - r.y) / scale,
                        w: f64::from(rect.w) / scale,
                        h: f64::from(rect.h) / scale,
                    },
                ));
            }
        }
        Err(OsError::OutOfBounds(format!(
            "capture rectangle {rect:?} does not lie inside a single display"
        )))
    }
}

// CGImage bitmap info bits (CGImage.h).
const ALPHA_INFO_MASK: u32 = 0x1F;
const BYTE_ORDER_MASK: u32 = 0x7000;
const BYTE_ORDER_DEFAULT: u32 = 0;
const BYTE_ORDER_32_LITTLE: u32 = 2 << 12;
const BYTE_ORDER_32_BIG: u32 = 4 << 12;

/// Byte offsets of red, green and blue inside one 32-bit pixel for a CGImage `bitmap_info`.
pub fn rgb_offsets(bitmap_info: u32) -> Result<[usize; 3]> {
    // Alpha first: PremultipliedFirst (2), First (4), NoneSkipFirst (6). None (0) has no 4th byte.
    let alpha = bitmap_info & ALPHA_INFO_MASK;
    let alpha_first = match alpha {
        2 | 4 | 6 => true,
        1 | 3 | 5 => false,
        other => {
            return Err(OsError::Platform(format!(
                "unsupported screen image alpha layout {other}"
            )))
        }
    };
    // In memory, big-endian ARGB is A,R,G,B; little-endian ARGB is B,G,R,A.
    match (bitmap_info & BYTE_ORDER_MASK, alpha_first) {
        (BYTE_ORDER_32_LITTLE, true) => Ok([2, 1, 0]),
        (BYTE_ORDER_32_LITTLE, false) => Ok([3, 2, 1]),
        (BYTE_ORDER_DEFAULT | BYTE_ORDER_32_BIG, true) => Ok([1, 2, 3]),
        (BYTE_ORDER_DEFAULT | BYTE_ORDER_32_BIG, false) => Ok([0, 1, 2]),
        (other, _) => Err(OsError::Platform(format!(
            "unsupported screen image byte order {other:#x}"
        ))),
    }
}

/// Convert a 32-bit CGImage buffer (rows `stride` bytes apart) to tightly packed opaque RGBA.
pub fn to_rgba(
    data: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    bits_per_pixel: usize,
    bitmap_info: u32,
) -> Result<Vec<u8>> {
    if bits_per_pixel != 32 {
        return Err(OsError::Platform(format!(
            "unsupported screen image depth {bits_per_pixel} bits per pixel"
        )));
    }
    if stride < width * 4 || data.len() < stride * height.saturating_sub(1) + width * 4 {
        return Err(OsError::Platform("screen image buffer is too small".into()));
    }
    let [r, g, b] = rgb_offsets(bitmap_info)?;
    let mut out = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let line = &data[row * stride..row * stride + width * 4];
        for px in line.chunks_exact(4) {
            out.extend_from_slice(&[px[r], px[g], px[b], 255]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(cg_id: u32, x: f64, y: f64, w: f64, h: f64, scale: f64, primary: bool) -> Screen {
        Screen {
            cg_id,
            points: PtRect { x, y, w, h },
            pixel_w: (w * scale) as u32,
            pixel_h: (h * scale) as u32,
            primary,
        }
    }

    fn retina_plus_external() -> Layout {
        // A 1440x900 Retina laptop with a 1920x1080 1x display to its right.
        Layout::new(vec![
            screen(7, 1440.0, 0.0, 1920.0, 1080.0, 1.0, false),
            screen(1, 0.0, 0.0, 1440.0, 900.0, 2.0, true),
        ])
        .unwrap()
    }

    #[test]
    fn primary_first_and_pixel_sizes() {
        let l = retina_plus_external();
        let d = l.displays();
        assert_eq!(d[0].id, 1);
        assert!(d[0].primary);
        assert_eq!(
            d[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 2880,
                h: 1800
            }
        );
        assert_eq!(
            d[1].rect,
            Rect {
                x: 2880,
                y: 0,
                w: 1920,
                h: 1080
            }
        );
        assert_eq!(l.screens()[0].cg_id, 1);
        assert_eq!(l.screens()[0].scale(), 2.0);
    }

    #[test]
    fn mixed_scales_never_overlap() {
        let l = retina_plus_external();
        let a = l.pixel_rect(0);
        let b = l.pixel_rect(1);
        assert!(i64::from(a.x) + i64::from(a.w) <= i64::from(b.x));
        assert_eq!(
            l.virtual_rect(),
            Rect {
                x: 0,
                y: 0,
                w: 4800,
                h: 1800
            }
        );
    }

    #[test]
    fn displays_left_of_and_above_main_get_negative_origins() {
        let l = Layout::new(vec![
            screen(1, 0.0, 0.0, 1440.0, 900.0, 2.0, true),
            screen(2, -1280.0, -200.0, 1280.0, 1024.0, 1.0, false),
        ])
        .unwrap();
        assert_eq!(
            l.pixel_rect(1),
            Rect {
                x: -2560,
                y: -400,
                w: 1280,
                h: 1024
            }
        );
        let v = l.virtual_rect();
        assert_eq!((v.x, v.y), (-2560, -400));
    }

    #[test]
    fn pixel_point_round_trip() {
        let l = retina_plus_external();
        // Retina: pixel 1000,600 is point 500.25,300.25 (pixel centre).
        let (x, y) = l.to_point(1000, 600);
        assert!((x - 500.25).abs() < 1e-9 && (y - 300.25).abs() < 1e-9);
        assert_eq!(l.to_pixel(x, y), (1000, 600));
        // External 1x display.
        let (x, y) = l.to_point(2880 + 100, 50);
        assert!((x - 1540.5).abs() < 1e-9 && (y - 50.5).abs() < 1e-9);
        assert_eq!(l.to_pixel(x, y), (2980, 50));
    }

    #[test]
    fn points_outside_every_display_are_clamped() {
        let l = retina_plus_external();
        let (x, y) = l.to_point(-50, -50);
        assert!(x >= 0.0 && y >= 0.0 && x < 1440.0 && y < 900.0);
        // The gap below the 1x display (pixels 2880.., 1080..1800) maps onto the nearest display.
        let (x, y) = l.to_point(3000, 1500);
        assert!(l.screens().iter().any(|s| s.points.contains(x, y)));
    }

    #[test]
    fn window_rects_scale_by_their_display() {
        let l = retina_plus_external();
        let r = l.rect_to_pixels(PtRect {
            x: 100.0,
            y: 50.0,
            w: 400.0,
            h: 300.0,
        });
        assert_eq!(
            r,
            Rect {
                x: 200,
                y: 100,
                w: 800,
                h: 600
            }
        );
        let r = l.rect_to_pixels(PtRect {
            x: 1500.0,
            y: 10.0,
            w: 200.0,
            h: 100.0,
        });
        assert_eq!(
            r,
            Rect {
                x: 2940,
                y: 10,
                w: 200,
                h: 100
            }
        );
    }

    #[test]
    fn capture_region_is_display_local_points() {
        let l = retina_plus_external();
        let (s, pt) = l
            .capture_region(Rect {
                x: 200,
                y: 100,
                w: 800,
                h: 600,
            })
            .unwrap();
        assert_eq!(s.cg_id, 1);
        assert_eq!(
            pt,
            PtRect {
                x: 100.0,
                y: 50.0,
                w: 400.0,
                h: 300.0
            }
        );
        let (s, pt) = l.capture_region(l.pixel_rect(1)).unwrap();
        assert_eq!(s.cg_id, 7);
        assert_eq!(
            pt,
            PtRect {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0
            }
        );
        assert!(l
            .capture_region(Rect {
                x: 2800,
                y: 0,
                w: 200,
                h: 10
            })
            .is_err());
        assert!(l
            .capture_region(Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 10
            })
            .is_err());
    }

    #[test]
    fn empty_layout_is_an_error() {
        assert!(Layout::new(vec![]).is_err());
        assert!(Layout::new(vec![screen(1, 0.0, 0.0, 0.0, 0.0, 1.0, true)]).is_err());
    }

    #[test]
    fn pixel_formats() {
        // Screen captures are normally BGRA premultiplied-first, little-endian.
        assert_eq!(rgb_offsets(2 | BYTE_ORDER_32_LITTLE).unwrap(), [2, 1, 0]);
        assert_eq!(rgb_offsets(6 | BYTE_ORDER_32_LITTLE).unwrap(), [2, 1, 0]);
        assert_eq!(rgb_offsets(1).unwrap(), [0, 1, 2]);
        assert_eq!(rgb_offsets(2 | BYTE_ORDER_32_BIG).unwrap(), [1, 2, 3]);
        assert_eq!(rgb_offsets(1 | BYTE_ORDER_32_LITTLE).unwrap(), [3, 2, 1]);
        assert!(rgb_offsets(0).is_err());
        assert!(rgb_offsets(2 | (1 << 12)).is_err());
    }

    #[test]
    fn converts_strided_bgra_to_opaque_rgba() {
        // 2x2 image, stride 12 (4 bytes padding per row), BGRA.
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, 9, 9, 9, 9, //
            7, 8, 9, 0, 10, 11, 12, 0, 9, 9, 9, 9,
        ];
        let rgba = to_rgba(&data, 2, 2, 12, 32, 2 | BYTE_ORDER_32_LITTLE).unwrap();
        assert_eq!(
            rgba,
            vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
        );
        assert!(to_rgba(&data, 2, 2, 12, 24, 2 | BYTE_ORDER_32_LITTLE).is_err());
        assert!(to_rgba(&data[..10], 2, 2, 12, 32, 2 | BYTE_ORDER_32_LITTLE).is_err());
    }
}
