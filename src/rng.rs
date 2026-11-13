//! Small, fast pseudo-random numbers (xorshift32) for noise, sample and hold
//! and per-note random values. Not for anything that needs real randomness.

/// Seed used by [`Rng::default`] and in place of 0, which xorshift would
/// never leave.
pub const DEFAULT_SEED: u32 = 0x9e37_79b9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(DEFAULT_SEED)
    }
}

impl Rng {
    /// A generator starting from `seed`; 0 becomes [`DEFAULT_SEED`].
    pub const fn new(seed: u32) -> Self {
        Self(if seed == 0 { DEFAULT_SEED } else { seed })
    }

    /// The current state, never 0.
    pub const fn state(&self) -> u32 {
        self.0
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `[0, 1)`, 24 bits of resolution.
    #[inline]
    pub fn unipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// Uniform in `[-1, 1)`, 24 bits of resolution.
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / (1u32 << 24) as f32) - 1.0
    }

    /// Uniform in `0..n`, `n > 0` (with a modulo bias that is negligible for
    /// small `n`).
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let mut rng = Rng::new(7);
        for _ in 0..10_000 {
            assert!((0.0..1.0).contains(&rng.unipolar()));
            assert!((-1.0..1.0).contains(&rng.bipolar()));
            assert!(rng.below(3) < 3);
        }
    }

    #[test]
    fn zero_seed_is_replaced() {
        assert_eq!(Rng::new(0), Rng::default());
        let mut rng = Rng::new(0);
        for _ in 0..1000 {
            assert_ne!(rng.next_u32(), 0);
        }
    }

    #[test]
    fn bipolar_is_zero_mean() {
        let mut rng = Rng::default();
        let n = 100_000;
        let mean = (0..n).map(|_| rng.bipolar() as f64).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.01, "{mean}");
    }
}
