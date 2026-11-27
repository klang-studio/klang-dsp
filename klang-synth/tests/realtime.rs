//! The LFO core runs on the audio thread: no allocation in any of its calls.

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use klang_dsp::phase::increment;
use klang_dsp::rng::Rng;
use klang_synth::{CyclicLfo, Shape};

#[global_allocator]
static ALLOC: AllocDisabler = AllocDisabler;

const SHAPES: [Shape; 7] = [
    Shape::Sine,
    Shape::Triangle,
    Shape::SawUp,
    Shape::SawDown,
    Shape::Square,
    Shape::SampleHold,
    Shape::SmoothRandom,
];

#[test]
fn lfo_core_does_not_allocate() {
    let mut rng = Rng::new(5);
    let mut held = CyclicLfo::new();
    let mut pair = CyclicLfo::new();
    let inc = increment(7.0, 48_000.0 / 32.0);
    let sum = assert_no_alloc(|| {
        let mut sum = 0.0;
        held.set_phase(klang_dsp::phase::from_unit(0.25));
        held.set_held(rng.bipolar());
        let (h, n) = (rng.bipolar(), rng.bipolar());
        pair.set_random(h, n);
        for i in 0..10_000 {
            let shape = SHAPES[i % SHAPES.len()];
            // Read before advancing (sampler-core) ...
            sum += held.value(shape);
            if held.advance(inc) {
                held.set_held(rng.bipolar());
            }
            // ... and after (Hexa, Volt).
            if pair.advance(inc) {
                pair.push_random(rng.bipolar());
            }
            sum += pair.value(shape) + pair.held() + pair.next_random() + held.held();
            sum += held.phase() as f32;
        }
        sum
    });
    assert!(sum.is_finite());
}
