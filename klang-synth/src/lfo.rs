//! The cyclic core of an LFO: a `u32` phase (one period = 2^32, see
//! [`klang_dsp::phase`]), wrap detection and the shape at the current phase.
//!
//! The core decides nothing about time or chance. The caller computes the phase
//! increment (rate, sample or block length, tempo sync, rate modulation and its
//! own rounding), draws the random values from its own generator in its own
//! order, and applies amount, delay, fade-in, unipolar scaling and retrigger.
//! [`CyclicLfo::value`] and [`CyclicLfo::advance`] are separate, so a caller can
//! read the value before or after advancing.
//!
//! Random values come in one of two patterns:
//!
//! - **Held value:** on a wrap the caller draws one value and [`set_held`]s it;
//!   [`Shape::SampleHold`] returns it until the next wrap.
//! - **Held/next pair:** the caller starts with [`set_random`] and on every wrap
//!   [`push_random`]es one new value, which moves the old next value into the
//!   held slot. [`Shape::SampleHold`] returns the held value and
//!   [`Shape::SmoothRandom`] glides from the held to the next value over the cycle,
//!   so it is continuous across the wrap.
//!
//! [`set_held`]: CyclicLfo::set_held
//! [`set_random`]: CyclicLfo::set_random
//! [`push_random`]: CyclicLfo::push_random

use klang_dsp::phase::to_unit;
use klang_dsp::wave;

/// LFO shapes; all span −1..=1 and start a cycle like [`klang_dsp::wave`] does.
///
/// No serialization: plugins map their own parameter enums or indices onto it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shape {
    #[default]
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    /// The held random value, constant for one cycle.
    SampleHold,
    /// From the held to the next random value with a smoothstep over one cycle.
    SmoothRandom,
}

/// Phase and random values of one LFO.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CyclicLfo {
    phase: u32,
    /// The random value of the current cycle.
    held: f32,
    /// The random value of the next cycle (held/next pattern only).
    next: f32,
}

impl CyclicLfo {
    /// Phase 0, both random values 0.
    pub const fn new() -> Self {
        Self {
            phase: 0,
            held: 0.0,
            next: 0.0,
        }
    }

    /// The current phase (one period = 2^32).
    #[inline]
    pub const fn phase(&self) -> u32 {
        self.phase
    }

    /// Moves to `phase` without touching the random values and without counting
    /// as a wrap.
    #[inline]
    pub fn set_phase(&mut self, phase: u32) {
        self.phase = phase;
    }

    /// Advances the phase by `inc` and returns whether a cycle ended (the phase
    /// wrapped past 2^32). An increment of 0 never wraps.
    #[inline]
    pub fn advance(&mut self, inc: u32) -> bool {
        let (phase, wrapped) = self.phase.overflowing_add(inc);
        self.phase = phase;
        wrapped
    }

    /// The random value of the current cycle.
    #[inline]
    pub const fn held(&self) -> f32 {
        self.held
    }

    /// The random value of the next cycle.
    #[inline]
    pub const fn next_random(&self) -> f32 {
        self.next
    }

    /// Held-value pattern: replaces the random value of the current cycle and
    /// leaves the next one alone.
    #[inline]
    pub fn set_held(&mut self, value: f32) {
        self.held = value;
    }

    /// Held/next pattern: sets the random values of the current and the next
    /// cycle, e.g. on a (re)start.
    #[inline]
    pub fn set_random(&mut self, held: f32, next: f32) {
        self.held = held;
        self.next = next;
    }

    /// Held/next pattern, on a wrap: the next value becomes the held one and
    /// `next` follows it.
    #[inline]
    pub fn push_random(&mut self, next: f32) {
        self.held = self.next;
        self.next = next;
    }

    /// The value of `shape` at the current phase, −1..=1.
    #[inline]
    pub fn value(&self, shape: Shape) -> f32 {
        let phase = self.phase;
        match shape {
            Shape::Sine => wave::sine(phase),
            Shape::Triangle => wave::triangle(phase),
            Shape::SawUp => wave::saw_up(phase),
            Shape::SawDown => wave::saw_down(phase),
            Shape::Square => wave::square(phase),
            Shape::SampleHold => self.held,
            Shape::SmoothRandom => {
                let t = to_unit(phase);
                let s = t * t * (3.0 - 2.0 * t);
                self.held + (self.next - self.held) * s
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klang_dsp::rng::Rng;

    const QUARTER: u32 = 1 << 30;
    const ALL: [Shape; 7] = [
        Shape::Sine,
        Shape::Triangle,
        Shape::SawUp,
        Shape::SawDown,
        Shape::Square,
        Shape::SampleHold,
        Shape::SmoothRandom,
    ];

    fn at(phase: u32) -> CyclicLfo {
        let mut lfo = CyclicLfo::new();
        lfo.set_phase(phase);
        lfo.set_random(0.3, -0.5);
        lfo
    }

    #[test]
    fn shape_key_points() {
        for (shape, expected) in [
            (Shape::Sine, [0.0, 1.0, 0.0, -1.0]),
            (Shape::Triangle, [0.0, 1.0, 0.0, -1.0]),
            (Shape::SawUp, [-1.0, -0.5, 0.0, 0.5]),
            (Shape::SawDown, [1.0, 0.5, 0.0, -0.5]),
            (Shape::Square, [1.0, 1.0, -1.0, -1.0]),
            (Shape::SampleHold, [0.3, 0.3, 0.3, 0.3]),
            // smoothstep(0, ¼, ½, ¾) = 0, 5/32, ½, 27/32 of the way from 0.3 to −0.5.
            (Shape::SmoothRandom, [0.3, 0.175, -0.1, -0.375]),
        ] {
            for (i, e) in expected.into_iter().enumerate() {
                let v = at(i as u32 * QUARTER).value(shape);
                assert!(
                    (v - e).abs() < 1e-5,
                    "{shape:?} at {i}/4: {v}, expected {e}"
                );
            }
        }
    }

    #[test]
    fn periodic_shapes_are_the_klang_dsp_waveforms() {
        let waves: [fn(u32) -> f32; 5] = [
            wave::sine,
            wave::triangle,
            wave::saw_up,
            wave::saw_down,
            wave::square,
        ];
        for (shape, f) in ALL.into_iter().zip(waves) {
            for i in 0..4096u32 {
                let phase = i.wrapping_mul(0x0010_0001);
                assert_eq!(at(phase).value(shape).to_bits(), f(phase).to_bits());
            }
        }
    }

    /// Volt's Smooth Random before the extraction, verbatim.
    fn volt_smooth(phase: u32, held: f32, next: f32) -> f32 {
        let t = to_unit(phase);
        let s = t * t * (3.0 - 2.0 * t);
        held + (next - held) * s
    }

    #[test]
    fn smooth_random_is_bit_identical_to_volt() {
        let mut rng = Rng::new(7);
        for i in 0..10_000u32 {
            let (held, next) = (rng.bipolar(), rng.bipolar());
            let phase = i.wrapping_mul(0x9e37_79b9);
            let mut lfo = CyclicLfo::new();
            lfo.set_phase(phase);
            lfo.set_random(held, next);
            assert_eq!(
                lfo.value(Shape::SmoothRandom).to_bits(),
                volt_smooth(phase, held, next).to_bits()
            );
        }
    }

    #[test]
    fn exactly_one_wrap_per_period() {
        let mut lfo = CyclicLfo::new();
        let wraps: [bool; 8] = core::array::from_fn(|_| lfo.advance(QUARTER));
        assert_eq!(
            wraps,
            [false, false, false, true, false, false, false, true]
        );
        assert_eq!(lfo.phase(), 0);

        // An increment that does not divide the period: one wrap per 2^32 phase units.
        for start in [0, 1, u32::MAX] {
            let inc = 12_345_679u32;
            let mut lfo = CyclicLfo::new();
            lfo.set_phase(start);
            let steps = 10_000u64;
            let wraps = (0..steps).filter(|_| lfo.advance(inc)).count() as u64;
            assert_eq!(wraps, (start as u64 + steps * inc as u64) >> 32);
        }
    }

    #[test]
    fn wrap_matches_a_phase_decrease() {
        // Hexa and sampler-core detect a wrap as `new < old`; the core agrees.
        let mut rng = Rng::new(3);
        let mut lfo = CyclicLfo::new();
        for _ in 0..10_000 {
            let inc = rng.next_u32();
            let old = lfo.phase();
            let wrapped = lfo.advance(inc);
            assert_eq!(wrapped, lfo.phase() < old);
            assert_eq!(lfo.phase(), old.wrapping_add(inc));
        }
    }

    #[test]
    fn zero_increment_stands_still() {
        let mut lfo = at(0x1234_5678);
        let before: [f32; 7] = ALL.map(|s| lfo.value(s));
        for _ in 0..1000 {
            assert!(!lfo.advance(0));
        }
        assert_eq!(lfo.phase(), 0x1234_5678);
        assert_eq!(ALL.map(|s| lfo.value(s)), before);
    }

    #[test]
    fn set_phase_is_not_a_wrap_and_keeps_random_values() {
        let mut lfo = at(3 * QUARTER);
        lfo.set_phase(0);
        assert_eq!((lfo.held(), lfo.next_random()), (0.3, -0.5));
        assert!(!lfo.advance(1));
    }

    #[test]
    fn random_patterns() {
        // Held value: only the held slot changes.
        let mut lfo = at(0);
        lfo.set_held(0.9);
        assert_eq!((lfo.held(), lfo.next_random()), (0.9, -0.5));
        assert_eq!(lfo.value(Shape::SampleHold), 0.9);

        // Held/next: a push moves next into held.
        lfo.set_random(0.1, 0.2);
        lfo.push_random(0.3);
        assert_eq!((lfo.held(), lfo.next_random()), (0.2, 0.3));
        assert_eq!(lfo.value(Shape::SampleHold), 0.2);
        assert_eq!(lfo.value(Shape::SmoothRandom), 0.2);
    }

    #[test]
    fn smooth_random_is_continuous_across_the_wrap() {
        let mut rng = Rng::new(11);
        let mut lfo = CyclicLfo::new();
        lfo.set_random(rng.bipolar(), rng.bipolar());
        // 1000 steps per cycle; the smoothstep's slope is 0 at both ends.
        let inc = (u32::MAX / 1000) + 1;
        let mut last = lfo.value(Shape::SmoothRandom);
        let mut wraps = 0;
        for _ in 0..10_000 {
            if lfo.advance(inc) {
                lfo.push_random(rng.bipolar());
                wraps += 1;
            }
            let v = lfo.value(Shape::SmoothRandom);
            // Largest step mid-cycle: 1.5 · |next − held| / 1000 ≤ 3e-3.
            assert!((v - last).abs() < 3.1e-3, "jump {last} → {v}");
            last = v;
        }
        assert_eq!(wraps, 10);
    }
}
