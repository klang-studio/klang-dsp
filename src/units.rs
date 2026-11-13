//! Conversions between musical and technical units: semitones, MIDI notes,
//! frequencies, decibels.

/// Lowest gain [`gain_to_db`] reports, as a floor for silence (-120 dB).
pub const MIN_GAIN: f32 = 1e-6;

/// Frequency ratio of an interval in semitones (12 = an octave up).
#[inline]
pub fn semitones_to_ratio(semitones: f32) -> f32 {
    libm::exp2f(semitones * (1.0 / 12.0))
}

/// Interval in semitones of a frequency ratio, the inverse of
/// [`semitones_to_ratio`].
#[inline]
pub fn ratio_to_semitones(ratio: f32) -> f32 {
    12.0 * libm::log2f(ratio)
}

/// Frequency in Hz of a (fractional) MIDI note number, A4 = 69 = 440 Hz.
#[inline]
pub fn note_to_freq(note: f32) -> f32 {
    440.0 * libm::exp2f((note - 69.0) * (1.0 / 12.0))
}

/// (Fractional) MIDI note number of a frequency in Hz, the inverse of
/// [`note_to_freq`].
#[inline]
pub fn freq_to_note(freq: f32) -> f32 {
    69.0 + 12.0 * libm::log2f(freq * (1.0 / 440.0))
}

/// Linear gain of a level in dB.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    libm::powf(10.0, db / 20.0)
}

/// Level in dB of a linear gain, at least -120 dB (see [`MIN_GAIN`]), so
/// silence gives a finite number for meters and graphs.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * libm::log10f(gain.max(MIN_GAIN))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} vs {expected}");
    }

    #[test]
    fn pitch() {
        near(semitones_to_ratio(12.0), 2.0);
        near(semitones_to_ratio(-24.0), 0.25);
        near(semitones_to_ratio(7.0), 1.498_307);
        near(ratio_to_semitones(semitones_to_ratio(-5.5)), -5.5);
        near(note_to_freq(69.0), 440.0);
        near(note_to_freq(60.0), 261.625_58);
        near(freq_to_note(note_to_freq(37.25)), 37.25);
    }

    #[test]
    fn levels() {
        near(db_to_gain(0.0), 1.0);
        near(db_to_gain(-6.0), 0.501_187);
        near(db_to_gain(20.0), 10.0);
        near(gain_to_db(db_to_gain(-17.0)), -17.0);
        near(gain_to_db(0.0), -120.0);
    }
}
