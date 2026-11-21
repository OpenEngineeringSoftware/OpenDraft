//! Hue, saturation and luminance: the HSL colour model of AutoCAD's True
//! Color tab, converted to and from the RGB a colour is stored as.

/// A colour as hue (degrees, from 0 up to 360), saturation and luminance
/// (each 0 to 1). Luminance 0.5 with saturation 1 is the pure hue; 1 is
/// white and 0 black whatever the hue.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hsl {
    pub hue: f64,
    pub saturation: f64,
    pub luminance: f64,
}

impl Hsl {
    /// The HSL of an RGB colour. A grey has no hue of its own and gets 0.
    pub fn from_rgb((r, g, b): (u8, u8, u8)) -> Self {
        let [r, g, b] = [r, g, b].map(|c| f64::from(c) / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let luminance = (max + min) / 2.0;
        let d = max - min;
        if d <= 0.0 {
            return Self {
                hue: 0.0,
                saturation: 0.0,
                luminance,
            };
        }
        let saturation = d / (1.0 - (2.0 * luminance - 1.0).abs());
        let sector = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        Self {
            hue: sector * 60.0,
            saturation,
            luminance,
        }
    }

    /// The RGB of this colour, each channel rounded. Values out of range are
    /// held to it (hue wraps round).
    pub fn to_rgb(self) -> (u8, u8, u8) {
        let hue = self.hue.rem_euclid(360.0);
        let saturation = self.saturation.clamp(0.0, 1.0);
        let luminance = self.luminance.clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * luminance - 1.0).abs()) * saturation;
        let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
        let m = luminance - c / 2.0;
        let (r, g, b) = match (hue / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let channel = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        (channel(r), channel(g), channel(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hsl(hue: f64, saturation: f64, luminance: f64) -> Hsl {
        Hsl {
            hue,
            saturation,
            luminance,
        }
    }

    #[test]
    fn pure_hues_white_black_and_grey() {
        assert_eq!(hsl(0.0, 1.0, 0.5).to_rgb(), (255, 0, 0));
        assert_eq!(hsl(120.0, 1.0, 0.5).to_rgb(), (0, 255, 0));
        assert_eq!(hsl(240.0, 1.0, 0.5).to_rgb(), (0, 0, 255));
        assert_eq!(hsl(60.0, 1.0, 1.0).to_rgb(), (255, 255, 255));
        assert_eq!(hsl(60.0, 1.0, 0.0).to_rgb(), (0, 0, 0));
        assert_eq!(hsl(200.0, 0.0, 0.5).to_rgb(), (128, 128, 128));
        assert_eq!(Hsl::from_rgb((128, 128, 128)).saturation, 0.0);
    }

    #[test]
    fn index_125_is_a_dull_mid_dark_teal() {
        // 63,127,111: hue 165, saturation 34%, luminance 37%.
        let c = Hsl::from_rgb((63, 127, 111));
        assert!((c.hue - 165.0).abs() < 0.5);
        assert_eq!((c.saturation * 100.0).round(), 34.0);
        assert_eq!((c.luminance * 100.0).round(), 37.0);
    }

    #[test]
    fn out_of_range_values_are_held_and_hue_wraps() {
        assert_eq!(hsl(360.0, 2.0, 0.5).to_rgb(), (255, 0, 0));
        assert_eq!(hsl(-120.0, 1.0, 0.5).to_rgb(), (0, 0, 255));
        assert_eq!(hsl(0.0, 1.0, 1.5).to_rgb(), (255, 255, 255));
    }

    #[test]
    fn every_rgb_comes_back_as_itself() {
        for r in (0..=255u8).step_by(5) {
            for g in (0..=255u8).step_by(5) {
                for b in (0..=255u8).step_by(5) {
                    let rgb = (r, g, b);
                    assert_eq!(Hsl::from_rgb(rgb).to_rgb(), rgb, "{rgb:?}");
                }
            }
        }
    }
}
