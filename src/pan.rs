//! Stereo balance: placing a stereo (or dual mono) signal between left and
//! right by turning one side down.
//!
//! A balance, not a pan law: the centre leaves both sides at 0 dB, so a
//! track sounds the same at 0 as without the control, and nothing is ever
//! turned up, so it can never clip what was clean. Moving away from the
//! centre lowers the far side linearly until it is silent at ±1.

/// Gains `(left, right)` of a balance -1..=1 (-1 left, 0 centre, 1 right):
/// the far side turned down, nothing turned up. Values beyond ±1 count as
/// ±1.
#[inline]
pub fn balance(balance: f32) -> (f32, f32) {
    let b = balance.clamp(-1.0, 1.0);
    ((1.0 - b).min(1.0), (1.0 + b).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// pisynth `synth-core/src/engine/render.rs`, copied verbatim.
    fn balance_gains(balance: f32) -> (f32, f32) {
        let b = balance.clamp(-1.0, 1.0);
        ((1.0 - b).min(1.0), (1.0 + b).min(1.0))
    }

    /// klopfer `dsp/src/voice.rs`, the same law written inline (`s.pan +
    /// m.pan` stands in for `pan`).
    fn klopfer_pan(pan: f32) -> (f32, f32) {
        let pan = pan.clamp(-1.0, 1.0);
        let pan_l = (1.0 - pan).min(1.0);
        let pan_r = (1.0 + pan).min(1.0);
        (pan_l, pan_r)
    }

    fn bits((l, r): (f32, f32)) -> (u32, u32) {
        (l.to_bits(), r.to_bits())
    }

    /// Every input the sweep checks: an even grid over -1.5..=1.5, every f32
    /// within 4096 steps of -1, 0 and 1 (where the clamp and the `min` act),
    /// and the special values.
    fn sweep() -> impl Iterator<Item = f32> {
        const STEPS: u32 = 1 << 21;
        let grid = (0..=STEPS).map(|i| -1.5 + 3.0 * (i as f32 / STEPS as f32));
        // Bit patterns 4096 below to 4096 above 0 and 1, both signs. Below
        // 0 the patterns wrap around to NaNs, which are dropped.
        let near = [0.0f32, 1.0]
            .into_iter()
            .flat_map(|x| (0..8192u32).map(move |i| x.to_bits().wrapping_add(i).wrapping_sub(4096)))
            .map(f32::from_bits)
            .filter(|y| y.is_finite())
            .flat_map(|y| [y, -y]);
        let special = [
            -1.5,
            1.5,
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::NAN,
        ];
        grid.chain(near).chain(special)
    }

    #[test]
    fn bit_identical_to_pisynth_and_klopfer() {
        let mut count = 0u32;
        for b in sweep() {
            assert_eq!(bits(balance(b)), bits(balance_gains(b)), "pisynth at {b:e}");
            assert_eq!(bits(balance(b)), bits(klopfer_pan(b)), "klopfer at {b:e}");
            count += 1;
        }
        assert!(count > 2_000_000, "{count}");
    }

    #[test]
    fn key_points() {
        assert_eq!(balance(0.0), (1.0, 1.0));
        assert_eq!(balance(-1.0), (1.0, 0.0));
        assert_eq!(balance(1.0), (0.0, 1.0));
        assert_eq!(balance(0.5), (0.5, 1.0));
        assert_eq!(balance(-0.25), (1.0, 0.75));
        // Beyond the range and NaN: clamped, never above 1.
        assert_eq!(balance(-1.5), (1.0, 0.0));
        assert_eq!(balance(7.0), (0.0, 1.0));
        assert_eq!(balance(f32::NAN), (1.0, 1.0));
    }

    #[test]
    fn never_turns_up() {
        for b in sweep() {
            let (l, r) = balance(b);
            assert!(
                (0.0..=1.0).contains(&l) && (0.0..=1.0).contains(&r),
                "{b:e}"
            );
            assert!(l == 1.0 || r == 1.0, "{b:e}: one side stays at 0 dB");
        }
    }
}
