//! Bit depth and sample rate reduction ("bitcrusher" building blocks).

/// Rounds to `2^(bits - 1)` steps per unit, i.e. steps of `2 / 2^bits` across
/// -1..1. Mid-tread: 0 is a level, so 1 bit gives -1, 0 and 1. `bits` may be
/// fractional for smooth modulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantizer {
    levels: f32,
    inv_levels: f32,
}

impl Quantizer {
    pub fn new(bits: f32) -> Self {
        let levels = libm::exp2f(bits - 1.0);
        Self {
            levels,
            inv_levels: 1.0 / levels,
        }
    }

    #[inline]
    pub fn apply(&self, x: f32) -> f32 {
        libm::roundf(x * self.levels) * self.inv_levels
    }
}

/// Sample and hold for `N` channels at a lower rate: takes a new sample
/// whenever its clock passes a full period, holds it in between.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleHold<const N: usize> {
    held: [f32; N],
    /// 0..1, a new sample at 1.
    clock: f32,
}

impl<const N: usize> Default for SampleHold<N> {
    fn default() -> Self {
        Self {
            held: [0.0; N],
            clock: 1.0,
        }
    }
}

impl<const N: usize> SampleHold<N> {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Advances the clock by `advance` (held rate / sample rate, at most 1
    /// for one new sample per call) and returns the held samples. `sample`
    /// gives the new ones and only runs when they are taken.
    #[inline]
    pub fn tick(&mut self, advance: f32, sample: impl FnOnce() -> [f32; N]) -> [f32; N] {
        self.clock += advance;
        if self.clock >= 1.0 {
            self.clock -= 1.0;
            self.held = sample();
        }
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantizes_to_bit_depth() {
        // 2 bits: steps of 0.5
        let q = Quantizer::new(2.0);
        assert_eq!(
            [0.3, -0.8, 0.0, 1.0].map(|x| q.apply(x)),
            [0.5, -1.0, 0.0, 1.0]
        );
        // 1 bit: -1, 0, 1
        let q = Quantizer::new(1.0);
        assert_eq!([0.4, 0.6, -0.7].map(|x| q.apply(x)), [0.0, 1.0, -1.0]);
    }

    #[test]
    fn holds_at_the_reduced_rate() {
        let mut hold = SampleHold::<1>::default();
        let out: [f32; 8] = core::array::from_fn(|i| hold.tick(0.25, || [i as f32])[0]);
        // The first call takes a sample (the clock starts full), then every fourth.
        assert_eq!(out, [0.0, 0.0, 0.0, 3.0, 3.0, 3.0, 3.0, 7.0]);
    }

    #[test]
    fn full_rate_passes_every_sample() {
        let mut hold = SampleHold::<2>::default();
        for i in 0..10 {
            let x = [i as f32, -(i as f32)];
            assert_eq!(hold.tick(1.0, || x), x);
        }
    }
}
