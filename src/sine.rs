//! Polynomial sine on a `u32` phase (see [`crate::phase`]). Branch-free
//! apart from a select, no tables, so it vectorizes well: SIMD versions use
//! the same coefficients `C1`..`C9` and stay bit-identical.

// Taylor coefficients of sin(πx), valid on x ∈ [-0.5, 0.5].
// Max error at the fold boundary ≈ π^11 / 11! / 2^11 ≈ 3.6e-6.
pub const C1: f32 = core::f32::consts::PI;
pub const C3: f32 = -5.167_712_8;
pub const C5: f32 = 2.550_164;
pub const C7: f32 = -0.599_264_5;
pub const C9: f32 = 0.082_145_89;

/// sin(2π · phase / 2^32).
#[inline]
pub fn sine(phase: u32) -> f32 {
    // Map to x ∈ [-1, 1), representing [-π, π).
    let x = (phase as i32) as f32 * (1.0 / 2_147_483_648.0);
    // Fold into [-0.5, 0.5] using sin(π(±1 - x)) = sin(πx).
    let folded = if x.abs() > 0.5 {
        1.0f32.copysign(x) - x
    } else {
        x
    };
    let x2 = folded * folded;
    folded * (C1 + x2 * (C3 + x2 * (C5 + x2 * (C7 + x2 * C9))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_bound_over_full_period() {
        let mut max_err = 0.0f32;
        // Step by an odd number to hit a spread of phases including near folds.
        for i in 0..(1u64 << 20) {
            let phase = (i * 4097) as u32;
            let reference = libm::sin(phase as f64 / 4_294_967_296.0 * core::f64::consts::TAU);
            let err = (sine(phase) as f64 - reference).abs() as f32;
            max_err = max_err.max(err);
        }
        assert!(max_err < 1e-5, "max error {max_err}");
    }

    #[test]
    fn key_points() {
        assert_eq!(sine(0), 0.0);
        assert!((sine(1 << 30) - 1.0).abs() < 1e-5);
        assert!((sine(3 << 30) + 1.0).abs() < 1e-5);
        assert!(sine(1 << 31).abs() < 1e-5);
    }
}
