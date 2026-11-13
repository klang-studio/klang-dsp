//! Basic waveforms on a `u32` phase (see [`crate::phase`]), all spanning
//! -1..=1 and starting at 0 rising, like the sine (the saws and the square
//! jump there).
//!
//! Naive, not band-limited: meant for LFOs and other control signals. At
//! audio rate the jumps alias; oscillators need PolyBLEP or similar.

use crate::phase::to_unit;

/// sin(2π · phase), see [`crate::sine::sine`].
#[inline]
pub fn sine(phase: u32) -> f32 {
    crate::sine::sine(phase)
}

#[inline]
pub fn triangle(phase: u32) -> f32 {
    let u = to_unit(phase) + 0.25;
    1.0 - 4.0 * (u - libm::floorf(u) - 0.5).abs()
}

/// Rising from -1 to 1 over the period.
#[inline]
pub fn saw_up(phase: u32) -> f32 {
    2.0 * to_unit(phase) - 1.0
}

/// Falling from 1 to -1 over the period.
#[inline]
pub fn saw_down(phase: u32) -> f32 {
    1.0 - 2.0 * to_unit(phase)
}

/// 1 for the first half of the period, -1 for the second.
#[inline]
pub fn square(phase: u32) -> f32 {
    if to_unit(phase) < 0.5 { 1.0 } else { -1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUARTER: u32 = 1 << 30;

    #[test]
    fn key_points() {
        let at = |f: fn(u32) -> f32| [0, QUARTER, 2 * QUARTER, 3 * QUARTER].map(f);
        assert_eq!(at(triangle), [0.0, 1.0, 0.0, -1.0]);
        assert_eq!(at(saw_up), [-1.0, -0.5, 0.0, 0.5]);
        assert_eq!(at(saw_down), [1.0, 0.5, 0.0, -0.5]);
        assert_eq!(at(square), [1.0, 1.0, -1.0, -1.0]);
        let s = at(sine);
        assert!(
            s[0] == 0.0
                && (s[1] - 1.0).abs() < 1e-5
                && s[2].abs() < 1e-5
                && (s[3] + 1.0).abs() < 1e-5
        );
    }

    #[test]
    fn span_full_range() {
        for f in [sine, triangle, saw_up, saw_down, square] {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for i in 0..4096u32 {
                let y = f(i.wrapping_mul(1 << 20));
                lo = lo.min(y);
                hi = hi.max(y);
            }
            // The saws approach ±1 only up to one step of the grid (1/4096); the
            // polynomial sine may overshoot by its error (< 1e-5).
            assert!(lo >= -1.0 - 1e-5 && hi <= 1.0 + 1e-5, "{lo} {hi}");
            assert!(lo < -0.999 && hi > 0.999, "{lo} {hi}");
        }
    }
}
