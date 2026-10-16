//! Conversion between the screenshot pixels the model sees and physical screen pixels.
//!
//! This is the only place that converts. Every coordinate that crosses the tool
//! boundary is in screenshot pixels of the active display, origin top-left.

use crate::types::{OsError, Rect, Result};

/// Size limits for screenshots handed to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Longest allowed edge in pixels.
    pub max_edge: u32,
    /// Largest allowed pixel count (width * height).
    pub max_pixels: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_edge: 1568,
            max_pixels: 3_750_000,
        }
    }
}

impl Budget {
    /// Reject budgets that cannot produce a usable image.
    pub fn validate(&self) -> Result<()> {
        if !(64..=2576).contains(&self.max_edge) {
            return Err(OsError::InvalidArgument(format!(
                "max_edge {} outside 64..=2576",
                self.max_edge
            )));
        }
        if self.max_pixels < 4096 {
            return Err(OsError::InvalidArgument(format!(
                "max_pixels {} below 4096",
                self.max_pixels
            )));
        }
        Ok(())
    }
}

/// Size a `w` x `h` image is reduced to under `budget`. Never upscales, keeps aspect ratio.
pub fn fit_size(w: u32, h: u32, budget: &Budget) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (w.max(1), h.max(1));
    }
    let edge_scale = f64::from(budget.max_edge) / f64::from(w.max(h));
    let pixel_scale = (f64::from(budget.max_pixels) / (f64::from(w) * f64::from(h))).sqrt();
    let scale = edge_scale.min(pixel_scale).min(1.0);
    if scale >= 1.0 {
        return (w, h);
    }
    // Floor (with a tiny epsilon for exact products) so the pixel cap is never exceeded.
    let sw = ((f64::from(w) * scale + 1e-6).floor() as u32).max(1);
    let sh = ((f64::from(h) * scale + 1e-6).floor() as u32).max(1);
    (sw, sh)
}

/// Mapping between one display's physical rectangle and its screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayGeometry {
    /// Physical rectangle of the display on the virtual desktop.
    pub rect: Rect,
    /// Width of the screenshot the model sees.
    pub shot_w: u32,
    /// Height of the screenshot the model sees.
    pub shot_h: u32,
}

impl DisplayGeometry {
    /// Build the geometry for `rect` under `budget`.
    pub fn new(rect: Rect, budget: &Budget) -> Result<Self> {
        budget.validate()?;
        if rect.w == 0 || rect.h == 0 {
            return Err(OsError::InvalidArgument("display has zero size".into()));
        }
        let (shot_w, shot_h) = fit_size(rect.w, rect.h, budget);
        Ok(Self {
            rect,
            shot_w,
            shot_h,
        })
    }

    /// Screenshot pixel to physical virtual-desktop pixel (centre of the source pixel).
    pub fn to_physical(&self, x: i64, y: i64) -> Result<(i32, i32)> {
        if x < 0 || y < 0 || x >= i64::from(self.shot_w) || y >= i64::from(self.shot_h) {
            return Err(OsError::OutOfBounds(format!(
                "({x}, {y}) is outside the {}x{} screenshot",
                self.shot_w, self.shot_h
            )));
        }
        let px = axis_to_physical(x as u64, self.shot_w, self.rect.w);
        let py = axis_to_physical(y as u64, self.shot_h, self.rect.h);
        Ok((self.rect.x + px as i32, self.rect.y + py as i32))
    }

    /// Physical virtual-desktop pixel to screenshot pixel, clamped to the screenshot.
    pub fn to_screenshot(&self, px: i32, py: i32) -> (u32, u32) {
        let rx = i64::from(px) - i64::from(self.rect.x);
        let ry = i64::from(py) - i64::from(self.rect.y);
        let rx = rx.clamp(0, i64::from(self.rect.w) - 1) as u64;
        let ry = ry.clamp(0, i64::from(self.rect.h) - 1) as u64;
        let sx = (rx * u64::from(self.shot_w) / u64::from(self.rect.w)) as u32;
        let sy = (ry * u64::from(self.shot_h) / u64::from(self.rect.h)) as u32;
        (sx.min(self.shot_w - 1), sy.min(self.shot_h - 1))
    }

    /// Whether a physical point lies on this display.
    pub fn contains_physical(&self, px: i32, py: i32) -> bool {
        self.rect.contains(px, py)
    }

    /// Convert an inclusive screenshot region `[x0, y0, x1, y1]` to a physical rectangle
    /// (relative to the display origin), flooring the start and ceiling the end.
    pub fn region_to_physical(&self, region: [i64; 4]) -> Result<Rect> {
        let [x0, y0, x1, y1] = region;
        if x0 < 0 || y0 < 0 || x1 >= i64::from(self.shot_w) || y1 >= i64::from(self.shot_h) {
            return Err(OsError::OutOfBounds(format!(
                "region {region:?} is outside the {}x{} screenshot",
                self.shot_w, self.shot_h
            )));
        }
        if x1 < x0 || y1 < y0 {
            return Err(OsError::InvalidArgument(
                "region must satisfy x0 <= x1 and y0 <= y1".into(),
            ));
        }
        let sx = u64::from(self.rect.w);
        let sy = u64::from(self.rect.h);
        let (shw, shh) = (u64::from(self.shot_w), u64::from(self.shot_h));
        let left = x0 as u64 * sx / shw;
        let top = y0 as u64 * sy / shh;
        let right = ((x1 as u64 + 1) * sx).div_ceil(shw).min(sx);
        let bottom = ((y1 as u64 + 1) * sy).div_ceil(shh).min(sy);
        Ok(Rect {
            x: self.rect.x + left as i32,
            y: self.rect.y + top as i32,
            w: (right - left).max(1) as u32,
            h: (bottom - top).max(1) as u32,
        })
    }
}

fn axis_to_physical(s: u64, shot: u32, phys: u32) -> u64 {
    // Centre of the screenshot pixel, floored, clamped to the display.
    let p = (2 * s + 1) * u64::from(phys) / (2 * u64::from(shot));
    p.min(u64::from(phys) - 1)
}

/// Normalise a physical point onto SendInput's 0..=65535 absolute range over the virtual desktop.
pub fn normalize_absolute(px: i32, py: i32, virtual_desktop: &Rect) -> Result<(i32, i32)> {
    if !virtual_desktop.contains(px, py) {
        return Err(OsError::OutOfBounds(format!(
            "({px}, {py}) is outside the virtual desktop"
        )));
    }
    let norm = |v: i32, origin: i32, size: u32| -> i32 {
        if size <= 1 {
            return 0;
        }
        let rel = i64::from(v) - i64::from(origin);
        ((rel * 65535 + i64::from(size) / 2) / (i64::from(size) - 1)).clamp(0, 65535) as i32
    };
    Ok((
        norm(px, virtual_desktop.x, virtual_desktop.w),
        norm(py, virtual_desktop.y, virtual_desktop.h),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn fit_size_examples() {
        let b = Budget::default();
        assert_eq!(fit_size(3440, 1440, &b), (1568, 656));
        assert_eq!(fit_size(1920, 1080, &b), (1568, 882));
        assert_eq!(fit_size(1280, 720, &b), (1280, 720));
        assert_eq!(fit_size(1080, 1920, &b), (882, 1568));
    }

    #[test]
    fn fit_size_never_upscales_and_respects_pixel_cap() {
        let b = Budget::default();
        assert_eq!(fit_size(800, 600, &b), (800, 600));
        let tight = Budget {
            max_edge: 2576,
            max_pixels: 100_000,
        };
        let (w, h) = fit_size(4000, 3000, &tight);
        assert!(u64::from(w) * u64::from(h) <= 100_000);
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn budget_validation() {
        assert!(Budget::default().validate().is_ok());
        assert!(Budget {
            max_edge: 10,
            max_pixels: 10_000
        }
        .validate()
        .is_err());
        assert!(Budget {
            max_edge: 1000,
            max_pixels: 10
        }
        .validate()
        .is_err());
    }

    #[test]
    fn identity_at_full_size() {
        let g = DisplayGeometry::new(rect(0, 0, 1280, 720), &Budget::default()).unwrap();
        assert_eq!((g.shot_w, g.shot_h), (1280, 720));
        assert_eq!(g.to_physical(10, 20).unwrap(), (10, 20));
        assert_eq!(g.to_screenshot(10, 20), (10, 20));
    }

    #[test]
    fn round_trip_within_one_step_on_varied_layouts() {
        let layouts = [
            rect(0, 0, 1920, 1080),
            rect(0, 0, 3440, 1440),
            rect(-2560, 0, 2560, 1440),
            rect(1920, -300, 1080, 1920),
            rect(0, 0, 3840, 2160),
            rect(-1366, 120, 1366, 768),
        ];
        for r in layouts {
            let g = DisplayGeometry::new(r, &Budget::default()).unwrap();
            let step = (f64::from(r.w) / f64::from(g.shot_w)).ceil() as i64 + 1;
            for (x, y) in [
                (0, 0),
                (g.shot_w - 1, g.shot_h - 1),
                (g.shot_w / 2, g.shot_h / 2),
                (7, g.shot_h - 3),
            ] {
                let (px, py) = g.to_physical(i64::from(x), i64::from(y)).unwrap();
                assert!(g.contains_physical(px, py), "{r:?} {x},{y}");
                let (bx, by) = g.to_screenshot(px, py);
                assert!((i64::from(bx) - i64::from(x)).abs() <= 1, "{r:?}");
                assert!((i64::from(by) - i64::from(y)).abs() <= 1, "{r:?}");
                assert!(step >= 1);
            }
        }
    }

    #[test]
    fn corners_stay_on_the_display() {
        let r = rect(-1920, 0, 1920, 1080);
        let g = DisplayGeometry::new(r, &Budget::default()).unwrap();
        let (px, py) = g
            .to_physical(i64::from(g.shot_w) - 1, i64::from(g.shot_h) - 1)
            .unwrap();
        assert!((-1920..0).contains(&px));
        assert!(py < 1080);
    }

    #[test]
    fn out_of_bounds_points_are_rejected() {
        let g = DisplayGeometry::new(rect(0, 0, 1920, 1080), &Budget::default()).unwrap();
        assert!(g.to_physical(-1, 0).is_err());
        assert!(g.to_physical(0, i64::from(g.shot_h)).is_err());
        assert!(g.to_physical(i64::from(g.shot_w), 0).is_err());
    }

    #[test]
    fn to_screenshot_clamps() {
        let g = DisplayGeometry::new(rect(100, 100, 1920, 1080), &Budget::default()).unwrap();
        assert_eq!(g.to_screenshot(-500, -500), (0, 0));
        assert_eq!(g.to_screenshot(99999, 99999), (g.shot_w - 1, g.shot_h - 1));
    }

    #[test]
    fn region_mapping_covers_inclusive_corners() {
        let g = DisplayGeometry::new(rect(0, 0, 3136, 1764), &Budget::default()).unwrap();
        assert_eq!((g.shot_w, g.shot_h), (1568, 882));
        let r = g.region_to_physical([0, 0, 1567, 881]).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 3136, 1764));
        let small = g.region_to_physical([10, 10, 10, 10]).unwrap();
        assert_eq!((small.x, small.y, small.w, small.h), (20, 20, 2, 2));
        assert!(g.region_to_physical([5, 5, 4, 9]).is_err());
        assert!(g.region_to_physical([0, 0, 1568, 10]).is_err());
    }

    #[test]
    fn absolute_normalisation_hits_corners() {
        let vd = rect(-1920, 0, 3840, 1080);
        assert_eq!(normalize_absolute(-1920, 0, &vd).unwrap(), (0, 0));
        assert_eq!(normalize_absolute(1919, 1079, &vd).unwrap(), (65535, 65535));
        assert!(normalize_absolute(1920, 0, &vd).is_err());
    }

    #[test]
    fn zero_sized_display_is_an_error() {
        assert!(DisplayGeometry::new(rect(0, 0, 0, 100), &Budget::default()).is_err());
    }
}
