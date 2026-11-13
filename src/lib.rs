//! Realtime DSP primitives shared by the klang-studio projects.
//!
//! The rules follow the strictest user (bare metal): `no_std`, no allocation,
//! `f32`, math via `libm`. Only building blocks live here, no parameter
//! structs or engines: how a knob maps onto a coefficient is the product's
//! decision. Something moves in once at least two projects need it.
#![cfg_attr(not(test), no_std)]

pub mod biquad;
pub mod buffer;
pub mod crush;
pub mod one_pole;
pub mod pan;
pub mod phase;
pub mod rng;
pub mod sine;
pub mod svf;
pub mod units;
pub mod wave;
