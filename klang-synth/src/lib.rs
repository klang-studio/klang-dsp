//! Opt-in synth building blocks on top of [`klang_dsp`].
//!
//! `klang-dsp` owns the general primitives (phase arithmetic, waveforms, random
//! numbers, filters); this crate combines them into small reusable blocks that a
//! plugin may use or ignore at build time. Plugins keep their signal flow,
//! parameters, units and presets. The same rules as in `klang-dsp` apply: `no_std`,
//! no allocation, `f32`, math via `libm`, plain DSP inputs only.
//!
//! The first block is [`CyclicLfo`], the phase, wrap and shape core of an LFO.
#![cfg_attr(not(test), no_std)]

pub mod lfo;

pub use lfo::{CyclicLfo, Shape};
