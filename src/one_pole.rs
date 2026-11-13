//! One-pole smoothing: `y += (x - y) · amount` with `amount = 1 - coeff`.
//!
//! The same filter shows up as parameter smoothing, envelope segments and
//! damping, each specified differently: by a time constant, by the time to
//! fall 60 dB or by a cutoff frequency. The functions here name which one is
//! meant and all return the pole `coeff` (0 = no smoothing, towards 1 =
//! slower).

use core::f32::consts::TAU;

/// `ln(1000)`: a segment with this time constant ratio covers 60 dB.
const LN_1000: f32 = 6.907_755;

/// Distance to the target below which [`settle`] snaps onto it.
pub const SETTLE_DISTANCE: f32 = 1e-5;

/// Pole for a time constant: after `time` the remaining distance is 1/e.
/// `step` is the interval between two updates, in the same unit as `time`
/// (`1.0` and a time in samples, or both in seconds).
#[inline]
pub fn time_coeff(step: f32, time: f32) -> f32 {
    libm::expf(-step / time)
}

/// Pole for an exponential segment that falls by 60 dB in `seconds`, updated
/// every sample. `seconds <= 0` jumps immediately.
#[inline]
pub fn t60_coeff(seconds: f32, sample_rate: f32) -> f32 {
    if seconds <= 0.0 {
        0.0
    } else {
        libm::expf(-LN_1000 / (seconds * sample_rate))
    }
}

/// Pole of a one-pole low-pass at `hz`, kept below 0.45 × `sample_rate`.
#[inline]
pub fn cutoff_coeff(hz: f32, sample_rate: f32) -> f32 {
    libm::expf(-TAU * hz.min(0.45 * sample_rate) / sample_rate)
}

/// `value` snapped to `target` once closer than [`SETTLE_DISTANCE`], so
/// smoothing arrives exactly and never lingers in subnormal numbers near a
/// target of 0.
#[inline]
pub fn settle(value: f32, target: f32) -> f32 {
    if (value - target).abs() < SETTLE_DISTANCE {
        target
    } else {
        value
    }
}

/// Moves `value` the fraction `amount` (`1 - coeff`, 0..=1) of the way to
/// `target`, see [`settle`].
#[inline]
pub fn chase(value: &mut f32, target: f32, amount: f32) {
    *value = settle(*value + (target - *value) * amount, target);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distance left after `n` updates from 1 towards 0.
    fn remaining(coeff: f32, n: usize) -> f32 {
        let mut y = 1.0f32;
        for _ in 0..n {
            y *= coeff;
        }
        y
    }

    #[test]
    fn time_constant_leaves_one_over_e() {
        let rate = 48_000.0;
        let left = remaining(time_coeff(1.0, 0.01 * rate), 480);
        assert!((left - (-1.0f32).exp()).abs() < 1e-3, "{left}");
        // Seconds per step give the same pole.
        assert!((time_coeff(1.0 / rate, 0.01) - time_coeff(1.0, 0.01 * rate)).abs() < 1e-6);
    }

    #[test]
    fn t60_falls_60_db() {
        let left = remaining(t60_coeff(0.5, 48_000.0), 24_000);
        assert!((20.0 * left.log10() + 60.0).abs() < 0.1, "{left}");
        assert_eq!(t60_coeff(0.0, 48_000.0), 0.0);
    }

    #[test]
    fn cutoff_gives_minus_3_db() {
        let (rate, hz) = (48_000.0, 100.0);
        let c = cutoff_coeff(hz, rate);
        // |H| = (1 - c) / |1 - c·e^-jw|
        let w = TAU * hz / rate;
        let den = ((1.0 - c * w.cos()).powi(2) + (c * w.sin()).powi(2)).sqrt();
        let db = 20.0 * ((1.0 - c) / den).log10();
        assert!((db + 3.0).abs() < 0.1, "{db}");
        assert_eq!(cutoff_coeff(1e9, rate), cutoff_coeff(0.45 * rate, rate));
    }

    #[test]
    fn chase_reaches_the_target_exactly() {
        for target in [0.0, 1.0, -0.5] {
            let mut x = 0.75;
            for _ in 0..2000 {
                chase(&mut x, target, 0.0645);
            }
            assert_eq!(x, target);
        }
    }
}
