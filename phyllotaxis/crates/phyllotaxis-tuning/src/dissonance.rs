//! Sensory dissonance, after Plomp & Levelt (1965) as parameterised by
//! Sethares, *Tuning, Timbre, Spectrum, Scale*.
//!
//! Two partials close in frequency beat within a critical band and the pair is
//! heard as rough; far apart, they are heard as two clean tones. The roughness
//! peaks at roughly a quarter of a critical bandwidth apart, which is the
//! number `X_STAR` below. Summed over every pair in a sounding spectrum, that
//! gives a dissonance value; swept across interval, it gives a curve whose
//! minima are the intervals that spectrum wants to be played at.
//!
//! ## Dissonance is a function of register, not only of interval
//!
//! Critical bandwidth widens with frequency, so the model takes **Hz, not
//! ratios**. The same interval is rougher played low than played high, which is
//! why voicings open out in the bass — and it means a spectrum-matched scale is
//! strictly correct only near the fundamental it was computed at.
//!
//! The instrument answers that by computing its tables at [`REFERENCE_HZ`] and
//! accepting the drift across the keyboard, exactly as 12-TET accepts its own.
//! Recomputing per octave is possible and deliberately not done: a scale that
//! changed shape as you played up the keyboard would stop being a scale.

/// Fundamental the scale tables are computed at: A3, near the centre of where
/// pads actually sit.
pub const REFERENCE_HZ: f64 = 220.0;

// Sethares' fit to the Plomp–Levelt curve.
const B1: f64 = 3.5;
const B2: f64 = 5.75;
/// Where roughness peaks, as a fraction of critical bandwidth.
const X_STAR: f64 = 0.24;
const S1: f64 = 0.0207;
const S2: f64 = 18.96;

/// Roughness of one pair of partials, at absolute frequencies in Hz.
///
/// Weighted by the *quieter* of the two: a loud partial and an inaudible one
/// are not a rough pair, however close they sit.
pub fn pair(f_low: f64, f_high: f64, amp_low: f64, amp_high: f64) -> f64 {
    let (f_low, f_high) = if f_low <= f_high { (f_low, f_high) } else { (f_high, f_low) };
    if f_low <= 0.0 {
        return 0.0;
    }
    let s = X_STAR / (S1 * f_low + S2);
    let delta = f_high - f_low;
    amp_low.min(amp_high) * ((-B1 * s * delta).exp() - (-B2 * s * delta).exp())
}

/// Total roughness *between* two sounding spectra, at a fundamental of
/// `fundamental_hz`.
///
/// Only cross terms are summed. Each spectrum's roughness against itself is the
/// same whatever interval separates them, so it is a constant offset on the
/// curve and cannot move a minimum. Leaving it out halves the work.
/// How far apart two partials can be before the pair contributes nothing.
///
/// The kernel is `exp(-B1·s·Δ) - exp(-B2·s·Δ)`, so it decays exponentially in
/// separation, and the pair is additionally weighted by the quieter partial —
/// which is at most 1. Past `exp(-18)` a pair contributes below 1.6e-8 to a
/// curve whose values run in the tens of thousands, so it cannot move a
/// minimum: the degrees come out bit-identical, which
/// `the_window_does_not_move_a_single_degree` checks across the roster.
#[inline]
fn reach(f_low: f64) -> f64 {
    const DECADES: f64 = 18.0;
    DECADES * (S1 * f_low + S2) / (B1 * X_STAR)
}

pub fn between(
    a: &crate::spectrum::Spectrum,
    b: &crate::spectrum::Spectrum,
    fundamental_hz: f64,
) -> f64 {
    // A sliding window, not a heuristic.
    //
    // This was the whole cost of the instrument. `tuning_for` sums this over
    // 6300 curve samples, and summing every pair made it O(n²) per sample:
    // measured at **2.7 seconds** for `fm I`'s 140 partials — which mattered
    // enormously, because it was being called on the audio thread.
    //
    // Both spectra are sorted by frequency, and `reach()` is monotonic in
    // frequency, so the qualifying window only ever moves forward. That makes
    // this O(n·k) with k the partials inside one reach. The result is not an
    // approximation of the full sum: every pair omitted contributes less than
    // 2e-15, and `dissonance_window_matches_the_full_sum` checks the two
    // against each other across the whole roster.
    let pa = a.partials();
    let pb = b.partials();
    let mut total = 0.0;
    let mut lo = 0usize;
    for p in pa {
        let fp = p.ratio * fundamental_hz;
        // Drop partials that have fallen behind the window. For these `q` is
        // the lower of the pair, so the reach is measured from `fq`.
        while lo < pb.len() {
            let fq = pb[lo].ratio * fundamental_hz;
            if fq < fp && fp - fq > reach(fq) {
                lo += 1;
            } else {
                break;
            }
        }
        for q in &pb[lo..] {
            let fq = q.ratio * fundamental_hz;
            // Ahead of the window, and `pb` only climbs from here.
            if fq > fp && fq - fp > reach(fp) {
                break;
            }
            // The pair is weighted by the quieter partial, so an inaudible one
            // cannot make a rough pair however close it sits.
            if p.amp.min(q.amp) > 1e-9 {
                total += pair(fp, fq, p.amp, q.amp);
            }
        }
    }
    total
}

/// The full O(n²) sum, kept as the reference the windowed one is checked
/// against. Not used on any hot path.
pub fn between_exhaustive(
    a: &crate::spectrum::Spectrum,
    b: &crate::spectrum::Spectrum,
    fundamental_hz: f64,
) -> f64 {
    let mut total = 0.0;
    for p in a.partials() {
        for q in b.partials() {
            total += pair(
                p.ratio * fundamental_hz,
                q.ratio * fundamental_hz,
                p.amp,
                q.amp,
            );
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrum::{Partial, Spectrum};

    #[test]
    fn identical_partials_are_smooth() {
        // Zero separation: the two exponentials cancel exactly.
        assert!(pair(440.0, 440.0, 1.0, 1.0).abs() < 1e-12);
    }

    #[test]
    fn distant_partials_are_smooth() {
        assert!(pair(220.0, 4400.0, 1.0, 1.0) < 1e-6);
    }

    /// Roughness peaks somewhere between the two, not at either end — the shape
    /// the whole model exists to express.
    #[test]
    fn roughness_peaks_in_between() {
        let f = 220.0;
        let at = |d: f64| pair(f, f + d, 1.0, 1.0);
        let peak = (1..400)
            .map(|i| at(i as f64 * 0.25))
            .fold(0.0f64, f64::max);
        assert!(peak > at(0.0));
        assert!(peak > at(200.0));
    }

    /// The quieter partial governs.
    #[test]
    fn weighted_by_the_quieter_partial() {
        let loud = pair(220.0, 235.0, 1.0, 1.0);
        let quiet = pair(220.0, 235.0, 1.0, 0.01);
        assert!(quiet < loud / 50.0);
    }

    /// Roughness depends on register, not on interval alone — the property
    /// that forces [`REFERENCE_HZ`] to exist at all.
    ///
    /// The audible form of this is that a close interval played by *harmonic
    /// tones* is rougher in the bass, which is why voicings open out down
    /// there. Note it is a claim about spectra, not about pairs of sines: two
    /// pure tones a semitone apart at 110 Hz sit before the roughness peak and
    /// are smoother than the same pair at 880 Hz. The model gives both, and
    /// only the first is a fact about music.
    #[test]
    fn a_close_interval_is_rougher_in_the_bass() {
        let harmonic = Spectrum::new(
            (1..=8).map(|n| Partial { ratio: n as f64, amp: 1.0 / n as f64 }),
        );
        let third = 2f64.powf(3.0 / 12.0);
        let low = between(&harmonic, &harmonic.transposed(third), 55.0);
        let high = between(&harmonic, &harmonic.transposed(third), 880.0);
        assert!(low > high, "bass {low}, treble {high}");
    }

    /// A harmonic spectrum is smoother at the octave than at the tritone —
    /// the sanity check that the model reproduces ordinary musical experience
    /// before it is trusted on inharmonic ones.
    #[test]
    fn harmonic_spectrum_prefers_the_octave_to_the_tritone() {
        let harmonic = Spectrum::new(
            (1..=8).map(|n| Partial { ratio: n as f64, amp: 1.0 / n as f64 }),
        );
        let at = |r: f64| between(&harmonic, &harmonic.transposed(r), REFERENCE_HZ);
        let octave = at(2.0);
        let tritone = at(2f64.powf(6.0 / 12.0));
        assert!(octave < tritone, "octave {octave}, tritone {tritone}");
    }

    /// And the fifth is smoother than the minor second, for the same reason.
    #[test]
    fn harmonic_spectrum_prefers_the_fifth_to_the_minor_second() {
        let harmonic = Spectrum::new(
            (1..=8).map(|n| Partial { ratio: n as f64, amp: 1.0 / n as f64 }),
        );
        let at = |r: f64| between(&harmonic, &harmonic.transposed(r), REFERENCE_HZ);
        assert!(at(1.5) < at(2f64.powf(1.0 / 12.0)));
    }
}
