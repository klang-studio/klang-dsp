//! Block operations on sample buffers: mixing, gains, gain ramps, peaks.
//!
//! Plain loops over slices, written so the compiler can vectorise them, and
//! without fused multiply-adds, so every target computes the same bits.
//! Functions taking two slices expect equal lengths: debug builds assert it,
//! release builds process the common length.

/// Adds `part` to `sum` sample by sample.
#[inline]
pub fn add(sum: &mut [f32], part: &[f32]) {
    debug_assert_eq!(sum.len(), part.len());
    for (s, p) in sum.iter_mut().zip(part) {
        *s += p;
    }
}

/// Adds `part` times `gain` to `sum`.
#[inline]
pub fn add_scaled(sum: &mut [f32], part: &[f32], gain: f32) {
    debug_assert_eq!(sum.len(), part.len());
    for (s, p) in sum.iter_mut().zip(part) {
        *s += p * gain;
    }
}

/// Multiplies `buf` by `gain`.
#[inline]
pub fn scale(buf: &mut [f32], gain: f32) {
    for x in buf {
        *x *= gain;
    }
}

/// Multiplies `buf` by a gain moving linearly from `from` to `to`.
///
/// Sample `i` of `n` gets `from + (to - from) · (i + 1) / n`, the last one
/// exactly `to`. `from` is the gain just before the slice, so a ramp split
/// across blocks (each starting where the previous one ended) moves on every
/// sample and never repeats a value.
#[inline]
pub fn gain_ramp(buf: &mut [f32], from: f32, to: f32) {
    let Some((last, head)) = buf.split_last_mut() else {
        return;
    };
    let step = ramp_step(from, to, head.len() + 1);
    for (x, k) in head.iter_mut().zip(1u32..) {
        *x *= ramp_gain(from, step, k);
    }
    *last *= to;
}

/// Adds `part` times a gain ramp from `from` to `to` to `sum`, with the
/// gains of [`gain_ramp`].
#[inline]
pub fn add_ramped(sum: &mut [f32], part: &[f32], from: f32, to: f32) {
    debug_assert_eq!(sum.len(), part.len());
    let n = sum.len().min(part.len());
    if n == 0 {
        return;
    }
    let step = ramp_step(from, to, n);
    for ((s, p), k) in sum[..n - 1].iter_mut().zip(&part[..n - 1]).zip(1u32..) {
        *s += p * ramp_gain(from, step, k);
    }
    sum[n - 1] += part[n - 1] * to;
}

/// Largest absolute sample value, 0 for an empty slice. NaNs are skipped.
#[inline]
pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0, |m, x| m.max(x.abs()))
}

/// Gain change per sample of a ramp over `n` samples.
#[inline]
fn ramp_step(from: f32, to: f32, n: usize) -> f32 {
    (to - from) / n as f32
}

/// Gain of the `k`-th sample of a ramp (from 1). Computed from the count
/// rather than summed up, so the error does not grow along the block and the
/// loop has no dependency from one sample to the next. A `u32` count, as
/// `usize` → `f32` does not vectorise on x86-64.
#[inline]
fn ramp_gain(from: f32, step: f32, k: u32) -> f32 {
    from + step * k as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    /// pisynth `synth-core/src/synth.rs`, copied verbatim.
    fn pisynth_add(sum: &mut [f32], part: &[f32]) {
        sum.iter_mut().zip(part).for_each(|(s, p)| *s += p);
    }

    const LENGTHS: [usize; 8] = [0, 1, 2, 3, 7, 64, 129, 1000];

    fn noise(rng: &mut Rng, n: usize) -> Vec<f32> {
        (0..n).map(|_| rng.bipolar() * 2.0).collect()
    }

    fn bits(buf: &[f32]) -> Vec<u32> {
        buf.iter().map(|x| x.to_bits()).collect()
    }

    /// The gains [`gain_ramp`] applies, read off a buffer of ones.
    fn ramp(n: usize, from: f32, to: f32) -> Vec<f32> {
        let mut buf = vec![1.0; n];
        gain_ramp(&mut buf, from, to);
        buf
    }

    #[test]
    fn add_is_bit_identical_to_pisynth() {
        let mut rng = Rng::new(3);
        for n in LENGTHS {
            let (sum, part) = (noise(&mut rng, n), noise(&mut rng, n));
            let (mut ours, mut theirs) = (sum.clone(), sum);
            add(&mut ours, &part);
            pisynth_add(&mut theirs, &part);
            assert_eq!(bits(&ours), bits(&theirs), "{n}");
        }
    }

    #[test]
    fn add_scale_add_scaled() {
        let mut sum = [1.0, -0.5, 0.25];
        add(&mut sum, &[0.5, 0.5, -0.25]);
        assert_eq!(sum, [1.5, 0.0, 0.0]);
        add_scaled(&mut sum, &[1.0, -2.0, 4.0], 0.5);
        assert_eq!(sum, [2.0, -1.0, 2.0]);
        scale(&mut sum, -0.25);
        assert_eq!(sum, [-0.5, 0.25, -0.5]);
        // Gain 1 adds the same bits as `add`.
        let mut rng = Rng::new(5);
        for n in LENGTHS {
            let (sum, part) = (noise(&mut rng, n), noise(&mut rng, n));
            let (mut a, mut b) = (sum.clone(), sum);
            add(&mut a, &part);
            add_scaled(&mut b, &part, 1.0);
            assert_eq!(bits(&a), bits(&b), "{n}");
        }
    }

    #[test]
    fn ramp_key_points() {
        assert_eq!(ramp(4, 0.0, 1.0), [0.25, 0.5, 0.75, 1.0]);
        assert_eq!(ramp(4, 1.0, 0.0), [0.75, 0.5, 0.25, 0.0]);
        assert_eq!(ramp(2, -1.0, 3.0), [1.0, 3.0]);
        assert_eq!(ramp(1, 0.3, 0.7), [0.7]);
        assert!(ramp(0, 0.3, 0.7).is_empty());
        let mut buf = [2.0, -4.0, 8.0, 1.0];
        gain_ramp(&mut buf, 0.0, 0.5);
        assert_eq!(buf, [0.25, -1.0, 3.0, 0.5]);
    }

    #[test]
    fn ramp_ends_exactly_on_target_and_stays_between() {
        for (from, to) in [(0.1, 0.7), (0.7, 0.1), (1.0, 0.316_227_8), (0.0, 1e-3)] {
            for n in 1..=300 {
                let g = ramp(n, from, to);
                assert_eq!(g[n - 1], to, "{from} → {to}, {n}");
                let (lo, hi) = (from.min(to), from.max(to));
                assert!(
                    g.iter().all(|x| (lo..=hi).contains(x)),
                    "{from} → {to}, {n}"
                );
                // Monotonic: every step goes the same way.
                let rising = to > from;
                assert!(
                    g.windows(2).all(|w| (w[1] > w[0]) == rising),
                    "{from} → {to}, {n}"
                );
            }
        }
    }

    #[test]
    fn ramp_without_change_is_scale() {
        let mut rng = Rng::new(9);
        for n in LENGTHS {
            let buf = noise(&mut rng, n);
            let (mut a, mut b) = (buf.clone(), buf);
            gain_ramp(&mut a, 0.6, 0.6);
            scale(&mut b, 0.6);
            assert_eq!(bits(&a), bits(&b), "{n}");
        }
    }

    #[test]
    fn ramps_join_across_blocks() {
        // 0 → 1 over 64 samples, once whole and once as two blocks of 32.
        let whole = ramp(64, 0.0, 1.0);
        let mut split = ramp(32, 0.0, 0.5);
        split.extend(ramp(32, 0.5, 1.0));
        for (i, (a, b)) in whole.iter().zip(&split).enumerate() {
            assert!((a - b).abs() < 1e-6, "{i}: {a} vs {b}");
            assert!((a - (i + 1) as f32 / 64.0).abs() < 1e-6, "{i}: {a}");
        }
    }

    #[test]
    fn add_ramped_adds_the_ramped_part() {
        let mut rng = Rng::new(11);
        for n in LENGTHS {
            let (sum, part) = (noise(&mut rng, n), noise(&mut rng, n));
            let mut ramped = part.clone();
            gain_ramp(&mut ramped, 0.9, 0.2);
            let (mut a, mut b) = (sum.clone(), sum);
            add_ramped(&mut a, &part, 0.9, 0.2);
            add(&mut b, &ramped);
            assert_eq!(bits(&a), bits(&b), "{n}");
        }
    }

    #[test]
    fn peak_of_block() {
        assert_eq!(peak(&[]), 0.0);
        assert_eq!(peak(&[0.5, -0.75, 0.25]), 0.75);
        assert_eq!(peak(&[-0.0, 0.0]), 0.0);
        assert_eq!(peak(&[f32::NAN, -0.5, f32::NAN]), 0.5);
        assert_eq!(peak(&[f32::NEG_INFINITY, 1.0]), f32::INFINITY);
        let mut rng = Rng::new(13);
        for n in LENGTHS {
            let buf = noise(&mut rng, n);
            let expected = buf.iter().map(|x| x.abs()).fold(0.0, f32::max);
            assert_eq!(peak(&buf), expected);
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn unequal_lengths_are_caught_in_debug_builds() {
        add(&mut [0.0; 4], &[1.0; 3]);
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn unequal_lengths_process_the_common_length_in_release_builds() {
        let mut sum = [0.0; 4];
        add(&mut sum, &[1.0; 3]);
        add_ramped(&mut sum, &[1.0; 2], 0.0, 1.0);
        assert_eq!(sum, [1.5, 2.0, 1.0, 0.0]);
    }
}
