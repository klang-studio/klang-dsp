//! Topology-preserving-transform (zero-delay feedback) state-variable filter
//! after Andrew Simper (Cytomic, "SvfLinearTrapOptimised2"): low-, band-,
//! high-pass and notch from one 2-pole structure, stable under fast cutoff
//! modulation.
//!
//! Split into coefficients ([`SvfCoefs`]), shared by all channels and voices
//! with the same settings, and per-channel state ([`Svf`]). One step yields
//! all responses at once ([`SvfOut`]); the caller picks one.

use core::f32::consts::PI;

/// Damping `k = 1/Q` of a Butterworth response (Q 0.707, no peak).
pub const BUTTERWORTH_K: f32 = core::f32::consts::SQRT_2;

/// Lowest cutoff [`warp`] accepts, in Hz.
pub const MIN_WARP_HZ: f32 = 10.0;

/// The frequency warp `g = tan(π f / rate)`, with the cutoff kept within
/// [`MIN_WARP_HZ`] and 0.45 × `sample_rate` (just below Nyquist).
#[inline]
pub fn warp(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let f = cutoff_hz.clamp(MIN_WARP_HZ, 0.45 * sample_rate);
    libm::tanf(PI * f / sample_rate)
}

/// Coefficients of one 2-pole stage, from the warped frequency `g` and the
/// damping `k = 1/Q`.
///
/// Linear interpolation between two sets ([`step_to`](Self::step_to),
/// [`advance`](Self::advance)) stays well behaved for the small changes
/// within a block and saves a division per sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SvfCoefs {
    pub a1: f32,
    pub a2: f32,
    pub a3: f32,
    pub k: f32,
}

impl SvfCoefs {
    #[inline]
    pub fn new(g: f32, k: f32) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            a1,
            a2,
            a3: g * a2,
            k,
        }
    }

    /// Coefficients for `cutoff_hz` at `sample_rate`, see [`warp`].
    #[inline]
    pub fn from_cutoff(cutoff_hz: f32, sample_rate: f32, k: f32) -> Self {
        Self::new(warp(cutoff_hz, sample_rate), k)
    }

    /// The per-sample step that reaches `to` after `1 / inv_n` samples.
    #[inline]
    pub fn step_to(&self, to: &Self, inv_n: f32) -> Self {
        Self {
            a1: (to.a1 - self.a1) * inv_n,
            a2: (to.a2 - self.a2) * inv_n,
            a3: (to.a3 - self.a3) * inv_n,
            k: (to.k - self.k) * inv_n,
        }
    }

    #[inline]
    pub fn advance(&mut self, step: &Self) {
        self.a1 += step.a1;
        self.a2 += step.a2;
        self.a3 += step.a3;
        self.k += step.k;
    }
}

/// State of one 2-pole stage for one channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Size of the stored energy, `|ic1| + |ic2|`: near 0 once the filter
    /// has stopped ringing.
    #[inline]
    pub fn magnitude(&self) -> f32 {
        self.ic1.abs() + self.ic2.abs()
    }

    /// Processes one sample.
    #[inline]
    pub fn tick(&mut self, v0: f32, c: &SvfCoefs) -> SvfOut {
        let SvfCoefs { a1, a2, a3, k } = *c;
        let v3 = v0 - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut { v0, v1, v2, k }
    }
}

/// All responses of one [`Svf::tick`]. The unused ones cost nothing once
/// inlined.
#[derive(Clone, Copy, Debug)]
pub struct SvfOut {
    v0: f32,
    v1: f32,
    v2: f32,
    k: f32,
}

impl SvfOut {
    #[inline]
    pub fn low(&self) -> f32 {
        self.v2
    }

    /// Band-pass with a peak gain of Q at the centre frequency.
    #[inline]
    pub fn band(&self) -> f32 {
        self.v1
    }

    /// Band-pass normalized to unity gain at the centre frequency.
    #[inline]
    pub fn band_normalized(&self) -> f32 {
        self.k * self.v1
    }

    #[inline]
    pub fn high(&self) -> f32 {
        self.v0 - self.k * self.v1 - self.v2
    }

    #[inline]
    pub fn notch(&self) -> f32 {
        self.v0 - self.k * self.v1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 96_000.0;

    /// Steady-state gain (dB) of one stage for a sine at `freq`.
    fn gain_db(response: fn(&SvfOut) -> f32, cutoff: f32, k: f32, freq: f32) -> f32 {
        let c = SvfCoefs::from_cutoff(cutoff, RATE, k);
        let mut svf = Svf::default();
        let n = (RATE * 0.5) as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            // Phase kept within one period: f32 sines of large arguments are noisy.
            let phase = (freq as f64 * i as f64 / RATE as f64).fract() as f32;
            let y = response(&svf.tick(libm::sinf(core::f32::consts::TAU * phase), &c));
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * libm::log10f(peak.max(1e-9))
    }

    #[test]
    fn responses() {
        let k = BUTTERWORTH_K;
        // Low-pass: flat below, -3 dB at the cutoff, -12 dB per octave above.
        assert!(gain_db(SvfOut::low, 1000.0, k, 100.0).abs() < 0.1);
        assert!((gain_db(SvfOut::low, 1000.0, k, 1000.0) + 3.0).abs() < 0.2);
        let octaves_up = gain_db(SvfOut::low, 1000.0, k, 8000.0);
        assert!((octaves_up + 36.0).abs() < 1.5, "{octaves_up}");
        // High-pass mirrors, normalized band-pass peaks at 0 dB, notch removes the centre.
        assert!(gain_db(SvfOut::high, 1000.0, k, 10_000.0).abs() < 0.1);
        assert!(gain_db(SvfOut::high, 1000.0, k, 125.0) < -35.0);
        assert!(gain_db(SvfOut::band_normalized, 1000.0, 0.2, 1000.0).abs() < 0.2);
        assert!(gain_db(SvfOut::notch, 1000.0, k, 1000.0) < -40.0);
        // The plain band-pass peaks at Q, a resonant low-pass too.
        let q = 10.0;
        assert!((gain_db(SvfOut::band, 1000.0, 1.0 / q, 1000.0) - 20.0).abs() < 0.3);
        assert!((gain_db(SvfOut::low, 1000.0, 1.0 / q, 1000.0) - 20.0).abs() < 0.3);
    }

    #[test]
    fn stays_finite_under_wild_modulation() {
        let mut svf = Svf::default();
        let mut x = 0.3f32;
        for i in 0..200_000 {
            // Cutoff jumping between the lower clamp and beyond Nyquist, Q 25.
            let cutoff = if i % 7 < 3 { RATE } else { 0.0 };
            x = -x;
            let y = svf
                .tick(x, &SvfCoefs::from_cutoff(cutoff, RATE, 1.0 / 25.0))
                .low();
            assert!(y.is_finite() && y.abs() < 100.0, "{y} at {i}");
        }
    }

    #[test]
    fn interpolation_reaches_target() {
        let from = SvfCoefs::from_cutoff(200.0, RATE, BUTTERWORTH_K);
        let to = SvfCoefs::from_cutoff(5000.0, RATE, 0.1);
        let n = 64;
        let step = from.step_to(&to, 1.0 / n as f32);
        let mut c = from;
        for _ in 0..n {
            c.advance(&step);
        }
        for (a, b) in [(c.a1, to.a1), (c.a2, to.a2), (c.a3, to.a3), (c.k, to.k)] {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn rings_out_and_resets() {
        let c = SvfCoefs::from_cutoff(1000.0, RATE, BUTTERWORTH_K);
        let mut svf = Svf::default();
        svf.tick(1.0, &c);
        assert!(svf.magnitude() > 0.0);
        for _ in 0..(RATE as usize / 10) {
            svf.tick(0.0, &c);
        }
        assert!(svf.magnitude() < 1e-5);
        svf.tick(1.0, &c);
        svf.reset();
        assert_eq!(svf.magnitude(), 0.0);
    }
}
