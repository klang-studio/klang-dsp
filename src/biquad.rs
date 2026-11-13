//! Biquad filters with the coefficient formulas from Robert Bristow-Johnson's
//! "Audio EQ Cookbook".
//!
//! The constructors do not clamp: the caller keeps `freq` below Nyquist and
//! `q` above 0, with the limits its product needs.
//!
//! Two structures with the same five multiplies per sample:
//! - [`Biquad`], transposed direct form II: two state registers and good
//!   precision, for coefficients that stay put.
//! - [`BiquadDf1`], direct form I: for coefficients that change while the
//!   signal runs (parameter glides, automation). TDF-II registers hold
//!   coefficient-weighted sums, so stepping the coefficients makes audible
//!   jumps (seen in pisynth's EQ: 0.25 on a 100 Hz sine during a high-shelf
//!   gain glide). DF-I keeps only past inputs and outputs, whose meaning does
//!   not depend on the coefficients. In f32 its headroom suits moderate
//!   gains (±15 dB is fine).

use core::f32::consts::{SQRT_2, TAU};

/// Coefficients normalized to `a0 = 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiquadCoefs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefs {
    /// Passes the signal unchanged.
    pub const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// Coefficients `b` and `a` divided by `a[0]`.
    #[inline]
    pub fn normalized(b: [f32; 3], a: [f32; 3]) -> Self {
        let inv = 1.0 / a[0];
        Self {
            b0: b[0] * inv,
            b1: b[1] * inv,
            b2: b[2] * inv,
            a1: a[1] * inv,
            a2: a[2] * inv,
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn low_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        let b1 = 1.0 - cos;
        Self::normalized(
            [b1 * 0.5, b1, b1 * 0.5],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    pub fn high_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        let b1 = -(1.0 + cos);
        Self::normalized(
            [-b1 * 0.5, b1, -b1 * 0.5],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// Band-pass with 0 dB gain at `freq`.
    pub fn band_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        Self::normalized([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub fn notch(freq: f32, q: f32, sample_rate: f32) -> Self {
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        Self::normalized(
            [1.0, -2.0 * cos, 1.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// Flat magnitude, phase turning by 360° around `freq`.
    pub fn all_pass(freq: f32, q: f32, sample_rate: f32) -> Self {
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        Self::normalized(
            [1.0 - alpha, -2.0 * cos, 1.0 + alpha],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// Bell boosting or cutting `gain_db` around `freq`.
    pub fn peaking(freq: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = libm::powf(10.0, gain_db / 40.0);
        let (cos, sin) = angle(freq, sample_rate);
        let alpha = sin / (2.0 * q);
        Self::normalized(
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        )
    }

    /// Low shelf with slope S = 1, `gain_db` below `freq`.
    pub fn low_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let (a, cos, two_sqrt_a_alpha) = shelf(freq, gain_db, sample_rate);
        let (ap1, am1) = (a + 1.0, a - 1.0);
        Self::normalized(
            [
                a * (ap1 - am1 * cos + two_sqrt_a_alpha),
                2.0 * a * (am1 - ap1 * cos),
                a * (ap1 - am1 * cos - two_sqrt_a_alpha),
            ],
            [
                ap1 + am1 * cos + two_sqrt_a_alpha,
                -2.0 * (am1 + ap1 * cos),
                ap1 + am1 * cos - two_sqrt_a_alpha,
            ],
        )
    }

    /// High shelf with slope S = 1, `gain_db` above `freq`.
    pub fn high_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let (a, cos, two_sqrt_a_alpha) = shelf(freq, gain_db, sample_rate);
        let (ap1, am1) = (a + 1.0, a - 1.0);
        Self::normalized(
            [
                a * (ap1 + am1 * cos + two_sqrt_a_alpha),
                -2.0 * a * (am1 + ap1 * cos),
                a * (ap1 + am1 * cos - two_sqrt_a_alpha),
            ],
            [
                ap1 - am1 * cos + two_sqrt_a_alpha,
                2.0 * (am1 - ap1 * cos),
                ap1 - am1 * cos - two_sqrt_a_alpha,
            ],
        )
    }
}

/// `(cos w0, sin w0)` at `freq`.
#[inline]
fn angle(freq: f32, sample_rate: f32) -> (f32, f32) {
    let w0 = TAU * freq / sample_rate;
    (libm::cosf(w0), libm::sinf(w0))
}

/// `(A, cos w0, 2·sqrt(A)·alpha)` of a shelf with S = 1.
#[inline]
fn shelf(freq: f32, gain_db: f32, sample_rate: f32) -> (f32, f32, f32) {
    let a = libm::powf(10.0, gain_db / 40.0);
    let (cos, sin) = angle(freq, sample_rate);
    // alpha = sin/2 · sqrt((A + 1/A)(1/S − 1) + 2) with S = 1.
    let alpha = sin * 0.5 * SQRT_2;
    (a, cos, 2.0 * libm::sqrtf(a) * alpha)
}

/// Transposed direct form II state for one channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Biquad {
    s1: f32,
    s2: f32,
}

impl Biquad {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Processes one sample.
    #[inline]
    pub fn tick(&mut self, x: f32, c: &BiquadCoefs) -> f32 {
        let y = c.b0 * x + self.s1;
        self.s1 = c.b1 * x - c.a1 * y + self.s2;
        self.s2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Direct form I state for one channel: the last two inputs and outputs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BiquadDf1 {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadDf1 {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Processes one sample.
    #[inline]
    pub fn tick(&mut self, x: f32, c: &BiquadCoefs) -> f32 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        (self.x2, self.x1, self.y2, self.y1) = (self.x1, x, self.y1, y);
        y
    }

    /// Zeroes a tail decaying from silent input before it becomes
    /// subnormal. Cheap enough to call once per block.
    #[inline]
    pub fn flush_denormals(&mut self) {
        if self.y1.abs() < 1e-20 && self.y2.abs() < 1e-20 && self.x1 == 0.0 && self.x2 == 0.0 {
            (self.y1, self.y2) = (0.0, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// Steady-state gain (dB) for a sine at `freq`, from the RMS: near
    /// Nyquist the samples miss the peaks.
    fn gain_db(c: &BiquadCoefs, freq: f32) -> f32 {
        let mut f = Biquad::default();
        let n = RATE as usize;
        let (mut sum_in, mut sum_out) = (0.0f64, 0.0f64);
        for i in 0..n {
            // Phase kept within one period: f32 sines of large arguments are noisy.
            let phase = (freq as f64 * i as f64 / RATE as f64).fract() as f32;
            let x = libm::sinf(TAU * phase);
            let y = f.tick(x, c);
            if i > n / 2 {
                sum_in += (x * x) as f64;
                sum_out += (y * y) as f64;
            }
        }
        10.0 * libm::log10f((sum_out / sum_in).max(1e-18) as f32)
    }

    fn near(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() < tolerance,
            "{actual} vs {expected}"
        );
    }

    #[test]
    fn pass_and_stop_bands() {
        let q = core::f32::consts::FRAC_1_SQRT_2;
        let lp = BiquadCoefs::low_pass(1000.0, q, RATE);
        near(gain_db(&lp, 50.0), 0.0, 0.1);
        near(gain_db(&lp, 1000.0), -3.0, 0.2);
        assert!(gain_db(&lp, 16_000.0) < -45.0);
        let hp = BiquadCoefs::high_pass(1000.0, q, RATE);
        near(gain_db(&hp, 16_000.0), 0.0, 0.1);
        near(gain_db(&hp, 1000.0), -3.0, 0.2);
        assert!(gain_db(&hp, 60.0) < -45.0);
        let bp = BiquadCoefs::band_pass(1000.0, 2.0, RATE);
        near(gain_db(&bp, 1000.0), 0.0, 0.1);
        assert!(gain_db(&bp, 100.0) < -20.0);
        assert!(gain_db(&BiquadCoefs::notch(1000.0, 2.0, RATE), 1000.0) < -40.0);
        for freq in [100.0, 1000.0, 10_000.0] {
            near(
                gain_db(&BiquadCoefs::all_pass(1000.0, 2.0, RATE), freq),
                0.0,
                0.1,
            );
        }
    }

    #[test]
    fn peaking_and_shelves() {
        let peak = BiquadCoefs::peaking(1000.0, 1.0, 12.0, RATE);
        near(gain_db(&peak, 1000.0), 12.0, 0.1);
        near(gain_db(&peak, 50.0), 0.0, 0.3);
        near(
            gain_db(&BiquadCoefs::peaking(1000.0, 1.0, -9.0, RATE), 1000.0),
            -9.0,
            0.1,
        );
        let low = BiquadCoefs::low_shelf(500.0, 6.0, RATE);
        near(gain_db(&low, 30.0), 6.0, 0.1);
        near(gain_db(&low, 15_000.0), 0.0, 0.1);
        let high = BiquadCoefs::high_shelf(2000.0, -6.0, RATE);
        near(gain_db(&high, 18_000.0), -6.0, 0.3);
        near(gain_db(&high, 50.0), 0.0, 0.1);
    }

    #[test]
    fn df1_matches_tdf2() {
        let c = BiquadCoefs::high_shelf(3000.0, 9.0, RATE);
        let (mut a, mut b) = (Biquad::default(), BiquadDf1::default());
        for i in 0..1000 {
            let x = (i as f32 * 0.13).sin() + 0.3 * (i as f32 * 1.7).sin();
            near(a.tick(x, &c), b.tick(x, &c), 1e-4);
        }
    }

    #[test]
    fn df1_flushes_silent_tail() {
        let c = BiquadCoefs::low_pass(1000.0, 0.7, RATE);
        let mut f = BiquadDf1::default();
        f.tick(1.0, &c);
        for _ in 0..2000 {
            f.tick(0.0, &c);
        }
        f.flush_denormals();
        assert_eq!(f, BiquadDf1::default());
    }

    #[test]
    fn zero_gain_and_identity_pass_unchanged() {
        let flat = BiquadCoefs::peaking(1000.0, 1.0, 0.0, RATE);
        let mut f = Biquad::default();
        for i in 0..100 {
            let x = (i as f32 * 0.37).sin();
            near(f.tick(x, &flat), x, 1e-5);
        }
        let mut f = Biquad::default();
        assert_eq!(f.tick(0.25, &BiquadCoefs::IDENTITY), 0.25);
        assert!(BiquadCoefs::IDENTITY.is_identity());
    }
}
