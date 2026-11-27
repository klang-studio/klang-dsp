# klang-synth

Opt-in synth building blocks for the klang-studio plugins, on top of
[`klang-dsp`](../). A plugin links this crate at build time if it wants a block
and keeps its own DSP otherwise. It is neither a plugin SDK requirement nor a
runtime dependency between bundles: komponist loads plugins through CLAP and
does not link `klang-synth` itself.

Same rules as `klang-dsp`: `no_std`, no allocation, `f32`, math via `libm`, plain
DSP inputs only. No dependency on komponist, CLAP or UI types. Licensed
GPL-3.0-or-later.

## Boundaries

- **`klang-dsp`** owns general DSP primitives: phase arithmetic, waveform
  functions, random numbers, interpolation, smoothing, filter kernels.
- **`klang-synth`** owns reusable synth blocks built from them. It starts with a
  cyclic LFO core. Envelopes or a modulation accumulator join only when real reuse
  justifies the API.
- **Plugins** own signal flow, voice allocation, modulation targets and units,
  parameters, presets and UI.

## `CyclicLfo`

The phase, wrap and shape core of an LFO on a `u32` phase (one period = 2^32,
`klang_dsp::phase`). The caller

- computes the phase increment (rate, tempo sync, rate modulation, its own
  rounding) and passes it to `advance(inc) -> bool`, which reports a wrap,
- draws random values from its own generator in its own order and hands them in,
  either as one held value (`set_held` on a wrap) or as a held/next pair
  (`set_random` at the start, `push_random` on a wrap) for `Shape::SmoothRandom`,
- reads `value(shape)` before or after advancing, as its timing requires,
- applies amount, delay, fade-in, unipolar output, start phase (`set_phase`),
  retrigger and seeding.

```rust
use klang_dsp::{phase, rng::Rng};
use klang_synth::{CyclicLfo, Shape};

let mut rng = Rng::new(1);
let mut lfo = CyclicLfo::new();
lfo.set_random(rng.bipolar(), rng.bipolar());
let inc = phase::increment(2.0, 48_000.0);
for _ in 0..48_000 {
    if lfo.advance(inc) {
        lfo.push_random(rng.bipolar());
    }
    let _out = lfo.value(Shape::SmoothRandom);
}
```
