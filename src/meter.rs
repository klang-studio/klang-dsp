//! Level meters: the peak of a block, peak meter ballistics in dB, and the
//! coefficient of a linear level that decays.
//!
//! Two ways to let a meter fall, for two kinds of caller:
//! - [`PeakMeter`] works in dB at display rate: it takes the peak since the
//!   last update and the time since then, holds peaks and latches clipping.
//! - [`decay_coeff`] is for a linear level kept up to date on the audio
//!   thread, `level = peak.max(level · coeff^n)` per block of `n` samples,
//!   for example in an atomic a GUI reads.
//!
//! Hold times and fall rates are the product's decision and come from the
//! caller.

use crate::units::{db_to_gain, gain_to_db};

/// Largest absolute sample of a block, 0 for an empty one, NaNs skipped;
/// the same as [`crate::buffer::peak`].
#[inline]
pub fn block_peak(block: &[f32]) -> f32 {
    crate::buffer::peak(block)
}

/// Multiplier for a linear level that falls by `db` dB in `seconds` when
/// applied `rate` times a second: once per sample at the sample rate, or
/// raised to the power `n` for a block of `n` samples. `seconds <= 0` drops
/// at once (0).
#[inline]
pub fn decay_coeff(db: f32, seconds: f32, rate: f32) -> f32 {
    if seconds <= 0.0 {
        0.0
    } else {
        libm::powf(db_to_gain(-db), 1.0 / (seconds * rate))
    }
}

/// Peak meter ballistics in dB, updated with the peak since the last update.
///
/// A new peak lifts two values at once: the [level](Self::level_db), which
/// falls again right away at `fall_db_per_s`, and the [held
/// peak](Self::held_db), which stays for `hold` seconds and then falls at the
/// same rate. A peak above full scale (> 1.0, i.e. > 0 dBFS) sets
/// [`clipped`](Self::clipped) until [`reset`](Self::reset). Neither value
/// goes below the floor of [`gain_to_db`] (-120 dB for silence), and the
/// held peak is never below the level.
///
/// The values depend only on the `peak` and `dt` passed in, no clock, and
/// the fall is the same (up to rounding) however the time is split into
/// updates.
/// `hold = f32::INFINITY` keeps the highest peak until `reset`, `hold = 0`
/// lets the held peak follow the level. For a level that falls at another
/// rate than the held peak, run a second meter with `hold = 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakMeter {
    hold: f32,
    fall_db_per_s: f32,
    level_db: f32,
    held_db: f32,
    /// Seconds before the held peak starts to fall.
    hold_left: f32,
    clipped: bool,
}

impl PeakMeter {
    /// A silent meter whose held peak stays `hold` seconds and then falls,
    /// like the level, by `fall_db_per_s` dB per second. Both `>= 0`.
    pub fn new(hold: f32, fall_db_per_s: f32) -> Self {
        let floor = gain_to_db(0.0);
        Self {
            hold,
            fall_db_per_s,
            level_db: floor,
            held_db: floor,
            hold_left: 0.0,
            clipped: false,
        }
    }

    /// Back to silence, the clip latch cleared; `hold` and the fall rate
    /// stay.
    pub fn reset(&mut self) {
        *self = Self::new(self.hold, self.fall_db_per_s);
    }

    /// Lets `dt` seconds pass (`dt < 0` counts as 0), then takes `peak`, the
    /// largest absolute sample since the last update (linear, as from
    /// [`block_peak`]).
    #[inline]
    pub fn update(&mut self, peak: f32, dt: f32) {
        let dt = dt.max(0.0);
        let db = gain_to_db(peak);
        self.level_db = (self.level_db - self.fall_db_per_s * dt).max(db);
        // The held peak falls only for the part of `dt` after its hold time.
        let falling = (dt - self.hold_left).max(0.0);
        self.hold_left = (self.hold_left - dt).max(0.0);
        let held = self.held_db - self.fall_db_per_s * falling;
        if db >= held {
            self.held_db = db;
            self.hold_left = self.hold;
        } else {
            self.held_db = held;
        }
        self.clipped |= peak > 1.0;
    }

    /// The level in dB: the latest peak, or an earlier, higher one falling.
    #[inline]
    pub fn level_db(&self) -> f32 {
        self.level_db
    }

    /// The held peak in dB, at least [`level_db`](Self::level_db).
    #[inline]
    pub fn held_db(&self) -> f32 {
        self.held_db
    }

    /// Whether a peak above full scale came in since the last
    /// [`reset`](Self::reset).
    #[inline]
    pub fn clipped(&self) -> bool {
        self.clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    /// The komponist design: hold 1.5 s, then fall 20 dB/s.
    fn design() -> PeakMeter {
        PeakMeter::new(1.5, 20.0)
    }

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} vs {expected}");
    }

    /// `n` updates of `dt` seconds without signal.
    fn silence(meter: &mut PeakMeter, n: usize, dt: f32) {
        for _ in 0..n {
            meter.update(0.0, dt);
        }
    }

    #[test]
    fn block_peaks() {
        assert_eq!(block_peak(&[]), 0.0);
        assert_eq!(block_peak(&[0.25, -0.5, 0.125]), 0.5);
        // klopfer `src/recording.rs` takes the peak of both channels at once.
        let mut rng = Rng::new(17);
        let left: Vec<f32> = (0..256).map(|_| rng.bipolar()).collect();
        let right: Vec<f32> = (0..256).map(|_| rng.bipolar() * 1.5).collect();
        let klopfer = left
            .iter()
            .chain(&right)
            .fold(0.0f32, |m, x| m.max(x.abs()));
        assert_eq!(block_peak(&left).max(block_peak(&right)), klopfer);
    }

    #[test]
    fn starts_silent_at_the_floor() {
        let meter = design();
        assert_eq!(meter.level_db(), gain_to_db(0.0));
        assert_eq!(meter.held_db(), gain_to_db(0.0));
        assert_eq!(meter.level_db(), -120.0);
        assert!(!meter.clipped());
    }

    #[test]
    fn holds_then_falls() {
        let dt = 1.0 / 64.0;
        let mut meter = design();
        meter.update(db_to_gain(-6.0), dt);
        near(meter.level_db(), -6.0);
        near(meter.held_db(), -6.0);
        // The level falls at once, the held peak waits 1.5 s.
        silence(&mut meter, 32, dt);
        near(meter.level_db(), -16.0);
        near(meter.held_db(), -6.0);
        silence(&mut meter, 64, dt);
        near(meter.level_db(), -36.0);
        assert_eq!(meter.held_db(), gain_to_db(db_to_gain(-6.0)), "1.5 s");
        silence(&mut meter, 1, dt);
        near(meter.held_db(), -6.0 - 20.0 * dt);
        silence(&mut meter, 31, dt);
        near(meter.level_db(), -46.0);
        near(meter.held_db(), -16.0);
        // Down to the floor and no further.
        silence(&mut meter, 64 * 10, dt);
        assert_eq!(meter.level_db(), -120.0);
        assert_eq!(meter.held_db(), -120.0);
    }

    #[test]
    fn higher_peaks_restart_the_hold_lower_ones_do_not() {
        let dt = 1.0 / 64.0;
        let mut meter = design();
        meter.update(db_to_gain(-12.0), dt);
        silence(&mut meter, 64, dt);
        // A lower peak lifts the level, not the held peak or its time: the
        // hold still ends 96 frames (1.5 s) after the first peak.
        meter.update(db_to_gain(-20.0), dt);
        near(meter.level_db(), -20.0);
        near(meter.held_db(), -12.0);
        silence(&mut meter, 31, dt);
        near(meter.held_db(), -12.0);
        silence(&mut meter, 1, dt);
        near(meter.held_db(), -12.0 - 20.0 * dt);
        // A higher one restarts the hold.
        meter.update(db_to_gain(-3.0), dt);
        silence(&mut meter, 96, dt);
        near(meter.held_db(), -3.0);
        silence(&mut meter, 1, dt);
        near(meter.held_db(), -3.0 - 20.0 * dt);
    }

    #[test]
    fn clip_latches_until_reset() {
        let mut meter = design();
        meter.update(1.0, 0.01);
        assert!(!meter.clipped(), "0 dBFS is not above full scale");
        meter.update(1.001, 0.01);
        assert!(meter.clipped());
        silence(&mut meter, 1000, 0.1);
        assert!(meter.clipped());
        assert_eq!(meter.level_db(), -120.0);
        meter.update(0.5, 0.01);
        meter.reset();
        assert_eq!(meter, design());
    }

    #[test]
    fn fall_does_not_depend_on_the_frame_rate() {
        let mut rng = Rng::new(23);
        for _ in 0..20 {
            let mut meter = design();
            meter.update(1.0, 0.0);
            // Irregular frame times, summed in f64 for the expected values.
            let mut t = 0.0f64;
            while t < 3.0 {
                let dt = 0.001 + 0.05 * rng.unipolar();
                meter.update(0.0, dt);
                t += dt as f64;
            }
            let level = (-20.0 * t).max(-120.0) as f32;
            let held = (-20.0 * (t - 1.5).max(0.0)).max(-120.0) as f32;
            near(meter.level_db(), level);
            near(meter.held_db(), held);
        }
    }

    #[test]
    fn same_calls_same_values_however_split() {
        // The same calls give the same bits; one update of 1/16 s or four of
        // 1/64 s with the peak in the last give the same values up to
        // rounding.
        let (mut coarse, mut again, mut fine) = (design(), design(), design());
        let mut rng = Rng::new(29);
        for _ in 0..400 {
            let peak = if rng.below(8) == 0 {
                1.2 * rng.unipolar()
            } else {
                0.0
            };
            coarse.update(peak, 1.0 / 16.0);
            again.update(peak, 1.0 / 16.0);
            for _ in 0..3 {
                fine.update(0.0, 1.0 / 64.0);
            }
            fine.update(peak, 1.0 / 64.0);
            assert_eq!(coarse, again);
            near(fine.level_db(), coarse.level_db());
            near(fine.held_db(), coarse.held_db());
            assert_eq!(fine.clipped(), coarse.clipped());
        }
    }

    #[test]
    fn hold_zero_follows_hold_infinite_keeps() {
        let mut follow = PeakMeter::new(0.0, 20.0);
        let mut keep = PeakMeter::new(f32::INFINITY, 20.0);
        let mut rng = Rng::new(31);
        let mut highest = 0.0f32;
        for _ in 0..2000 {
            let peak = rng.unipolar() * rng.unipolar();
            let dt = 0.002 + 0.03 * rng.unipolar();
            highest = highest.max(peak);
            follow.update(peak, dt);
            keep.update(peak, dt);
            assert_eq!(follow.held_db(), follow.level_db());
            assert_eq!(keep.held_db(), gain_to_db(highest));
            assert!(keep.held_db() >= keep.level_db());
        }
    }

    #[test]
    fn held_never_below_level_and_negative_dt_ignored() {
        let mut meter = design();
        let mut rng = Rng::new(37);
        for _ in 0..5000 {
            let peak = if rng.below(4) == 0 {
                rng.unipolar()
            } else {
                0.0
            };
            meter.update(peak, 0.05 * rng.unipolar());
            assert!(meter.held_db() >= meter.level_db());
            assert!(meter.level_db() >= -120.0);
        }
        let before = meter;
        meter.update(0.0, -1.0);
        assert_eq!(meter.level_db(), before.level_db());
        assert_eq!(meter.held_db(), before.held_db());
    }

    /// klopfer `src/recording.rs` (and `src/lib.rs`, there in ms): a level
    /// falling 20 dB in `secs`, applied once per sample.
    fn klopfer_meter_decay(secs: f32, sample_rate: f32) -> f32 {
        0.1f32.powf(1.0 / (secs * sample_rate))
    }

    #[test]
    fn decay_coeff_expresses_klopfers_meter() {
        for rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for secs in [0.3, 300.0 / 1000.0, 0.05, 1.5] {
                // klang-dsp's libm `powf` and std's may differ in the last bit.
                let (ours, theirs) = (
                    decay_coeff(20.0, secs, rate),
                    klopfer_meter_decay(secs, rate),
                );
                assert!(
                    ours.to_bits().abs_diff(theirs.to_bits()) <= 1,
                    "{rate} Hz, {secs} s: {ours} vs {theirs}"
                );
            }
        }
    }

    #[test]
    fn decay_coeff_falls_by_db_in_seconds() {
        let rate = 48_000.0;
        // Per sample, as klopfer applies it per block: `peak.max(old · c^n)`.
        let c = decay_coeff(20.0, 0.3, rate);
        let mut level = 1.0f32;
        for _ in 0..(0.3 * rate) as usize / 64 {
            level = 0.0f32.max(level * c.powi(64));
        }
        // A pole this close to 1 is only known to about 3e-8 in f32, which
        // adds up to some 0.004 dB over 14 400 samples (klopfer's too).
        let db = gain_to_db(level);
        assert!((db + 20.0).abs() < 0.01, "{db}");
        // Per display frame, and the same fall as a `PeakMeter` without hold.
        let c = decay_coeff(20.0, 1.0, 64.0);
        let mut meter = PeakMeter::new(0.0, 20.0);
        let mut level = 1.0f32;
        meter.update(level, 0.0);
        for _ in 0..48 {
            level *= c;
            meter.update(0.0, 1.0 / 64.0);
        }
        near(gain_to_db(level), -15.0);
        near(meter.level_db(), -15.0);
        assert_eq!(decay_coeff(20.0, 0.0, rate), 0.0);
    }
}
