//! Sky colors for the time of day, behind each event's time: night navy, dawn
//! purple and rose, daytime blue, late-afternoon gold, sunset orange, dusk
//! purple. Every color is dark enough for white text to stay readable.

use chrono::{NaiveTime, Timelike};
use iced::Color;

/// (hour of day, RGB) keyframes; colors between them are blended.
const SKY: &[(f32, [u8; 3])] = &[
    (0.0, [0x14, 0x1a, 0x33]),  // midnight
    (5.0, [0x1d, 0x22, 0x46]),  // before dawn
    (6.0, [0x55, 0x38, 0x6e]),  // dawn
    (7.0, [0x9a, 0x48, 0x52]),  // sunrise
    (9.0, [0x2f, 0x66, 0xa8]),  // morning
    (13.0, [0x24, 0x63, 0xae]), // midday
    (16.0, [0x8c, 0x5a, 0x16]), // late afternoon
    (18.0, [0xa3, 0x44, 0x22]), // sunset
    (19.5, [0x63, 0x33, 0x6a]), // dusk
    (21.0, [0x24, 0x27, 0x4f]), // evening
    (24.0, [0x14, 0x1a, 0x33]), // midnight again
];

pub fn color(time: NaiveTime) -> Color {
    let hour = time.num_seconds_from_midnight() as f32 / 3600.0;
    let i = SKY
        .windows(2)
        .position(|w| hour < w[1].0)
        .unwrap_or(SKY.len() - 2);
    let ((h0, c0), (h1, c1)) = (SKY[i], SKY[i + 1]);
    let t = (hour - h0) / (h1 - h0);
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) / 255.0;
    Color::from_rgb(mix(c0[0], c1[0]), mix(c0[1], c1[1]), mix(c0[2], c1[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color) -> f32 {
        let channel = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }

    #[test]
    fn white_text_is_readable_all_day() {
        for minute in (0..24 * 60).step_by(5) {
            let time = NaiveTime::from_hms_opt(minute / 60, minute % 60, 0).unwrap();
            let contrast = 1.05 / (luminance(color(time)) + 0.05);
            assert!(contrast >= 4.5, "{time}: contrast {contrast:.2}");
        }
    }

    #[test]
    fn hits_the_keyframes_and_wraps_at_midnight() {
        let at = |h, m| color(NaiveTime::from_hms_opt(h, m, 0).unwrap());
        assert_eq!(at(0, 0), Color::from_rgb8(0x14, 0x1a, 0x33));
        assert_eq!(at(13, 0), Color::from_rgb8(0x24, 0x63, 0xae));
        let late = at(23, 59);
        assert!(
            (late.r - at(0, 0).r).abs() < 0.01,
            "23:59 is nearly midnight"
        );
    }
}
