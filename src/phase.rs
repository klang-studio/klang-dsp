//! `u32` phase accumulator: the full `u32` range is one period, so wrap-around
//! is free and exact.

/// One period expressed in phase units (2^32).
pub const PHASE_PERIOD: f64 = 4_294_967_296.0;

/// [`PHASE_PERIOD`] as `f32`, for per-sample conversions.
pub const PERIOD_F32: f32 = 4_294_967_296.0;

/// A phase as a fraction of a period, 0..1.
#[inline]
pub fn to_unit(phase: u32) -> f32 {
    phase as f32 * (1.0 / PERIOD_F32)
}

/// A fraction of a period as a phase, the inverse of [`to_unit`]. Wraps:
/// whole periods are dropped and negative values count back from the end
/// (-0.25 is the same as 0.75).
#[inline]
pub fn from_unit(unit: f32) -> u32 {
    (unit as f64 * PHASE_PERIOD) as i64 as u32
}

/// Phase units per radian, used to turn a phase-modulation amount in radians
/// into a phase offset.
pub const PHASE_PER_RADIAN: f32 = (PHASE_PERIOD / core::f64::consts::TAU) as f32;

/// Phase increment per sample for `freq` Hz at `sample_rate` Hz.
///
/// Computed in `f64` to keep sub-cent accuracy; intended for control rate.
/// Frequencies above `sample_rate` wrap (alias) like any naive oscillator.
#[inline]
pub fn increment(freq: f32, sample_rate: f32) -> u32 {
    let periods = freq as f64 / sample_rate as f64;
    (periods.max(0.0) * PHASE_PERIOD + 0.5) as u64 as u32
}

/// Converts a phase offset in radians to phase units, wrapping correctly for
/// offsets beyond ±π.
#[inline]
pub fn from_radians(radians: f32) -> u32 {
    (radians * PHASE_PER_RADIAN) as i64 as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_exactly_after_one_period() {
        // 1/4 of the sample rate → exactly four samples per period.
        let inc = increment(12_000.0, 48_000.0);
        assert_eq!(inc, 1 << 30);
        let phase = (0..4).fold(0u32, |p, _| p.wrapping_add(inc));
        assert_eq!(phase, 0);
    }

    #[test]
    fn frequency_accuracy() {
        let sr = 48_000.0;
        for freq in [27.5, 440.0, 4186.0, 20_000.0] {
            let actual = increment(freq, sr) as f64 / PHASE_PERIOD * sr as f64;
            // Rounding error is at most half the resolution of sr / 2^32.
            let err = (actual - freq as f64).abs();
            assert!(
                err <= 0.5 * sr as f64 / PHASE_PERIOD,
                "{freq} Hz: error {err} Hz"
            );
        }
    }

    #[test]
    fn unit_round_trip_and_wrap() {
        assert_eq!(from_unit(0.0), 0);
        assert_eq!(from_unit(0.25), 1 << 30);
        assert_eq!(from_unit(0.5), 1 << 31);
        assert_eq!(from_unit(1.0), 0);
        assert_eq!(from_unit(2.75), 3 << 30);
        assert_eq!(from_unit(-0.25), 3 << 30);
        for u in [0.1f32, 0.333, 0.9] {
            assert!((to_unit(from_unit(u)) - u).abs() < 1e-7, "{u}");
        }
    }

    #[test]
    fn radians_wrap() {
        let close = |a: u32, b: u32| (a.wrapping_sub(b) as i32).abs() < 1 << 10;
        assert!(close(from_radians(core::f32::consts::PI), 1 << 31));
        assert!(close(from_radians(-core::f32::consts::PI), 1 << 31));
        // 2π + π/2 lands on a quarter period (within f32 precision).
        let q = from_radians(core::f32::consts::TAU + core::f32::consts::FRAC_PI_2);
        assert!((q as i64 - (1i64 << 30)).abs() < 1 << 10);
    }
}
