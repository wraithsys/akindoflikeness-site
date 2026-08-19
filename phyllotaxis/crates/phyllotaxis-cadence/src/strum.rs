//! Strum — bipolar, φ-spaced offsetting of note events.
//!
//! Billy's spec: full negative is low-to-high, so the highest-pitched event is
//! offset most from the first; full positive is the reverse; and the gaps are
//! φ-spaced so the strum **accelerates** rather than arriving metronomically,
//! which is most of what makes one sound played rather than sequenced.
//!
//! Releases are mirrored: the same shape applied to the notes leaving. Five
//! staggered releases is how an instrument lets go of a chord; five
//! simultaneous ones is a gate closing.

use crate::word::PHI;

/// Absolute ceiling on the strum span, seconds.
///
/// Fibonacci 233 ms. Past this a strum stops reading as one gesture and starts
/// reading as an arpeggio, whatever the chord rate is.
pub const BUDGET_MAX_S: f32 = 0.233;

/// The strum budget for a chord that will last `interval_s`.
///
/// `Δ/φ⁵`. Derived rather than dialled: `DESIGN.md` §5 makes the gesture `Δ/φ`,
/// so this is `gesture/φ⁴` — four rungs below the shape the note is making, on
/// the same ladder as everything else. A fixed 10–30 ms guitar figure would be
/// inaudible against pad-length arrivals.
///
/// The **incoming** interval is used for both the release of the old chord and
/// the attack of the new one. The Fibonacci word makes intervals alternate, and
/// if the release ran on a larger budget than the attack, notes leaving could
/// overtake notes arriving.
#[inline]
pub fn budget_s(interval_s: f32) -> f32 {
    (interval_s / (PHI as f32).powi(5)).min(BUDGET_MAX_S)
}

/// Offsets for notes **arriving**, indexed by pitch rank (0 = lowest).
///
/// Continuous at `b = 0` with no special case: both terms vanish, so a block
/// chord is the limit rather than a branch.
pub fn attack_offsets(count: usize, bias: f32, budget_s: f32) -> Vec<f32> {
    let (down, up) = ((-bias).max(0.0), bias.max(0.0));
    let phi = PHI as f32;
    (0..count)
        .map(|i| {
            let asc = 1.0 - phi.powi(-(i as i32));
            let desc = 1.0 - phi.powi(-((count as i32 - 1) - i as i32));
            budget_s * (down * asc + up * desc)
        })
        .collect()
}

/// Offsets for notes **leaving**, indexed by pitch rank (0 = lowest).
///
/// The same shape reversed, so the chord lets go in the opposite order to the
/// one it arrived in — and normalised so the first departure is at zero.
pub fn release_offsets(count: usize, bias: f32, budget_s: f32) -> Vec<f32> {
    let (down, up) = ((-bias).max(0.0), bias.max(0.0));
    let phi = PHI as f32;
    let last = (count as i32 - 1).max(0);
    let floor = phi.powi(-last);
    (0..count)
        .map(|i| {
            let asc = phi.powi(-(last - i as i32)) - floor;
            let desc = phi.powi(-(i as i32)) - floor;
            budget_s * (down * asc + up * desc)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: &[f32]) -> Vec<f32> {
        v.iter().map(|s| (s * 1000.0 * 100.0).round() / 100.0).collect()
    }

    /// Hand-computed in the derivation, at B = 144 ms.
    ///
    /// The numbers are Fibonacci and that is not a coincidence: with a
    /// Fibonacci budget, `B(1 − φ⁻¹) = 55` and `B(1 − φ⁻²) = 89` land on the
    /// sequence exactly, because `1 − φ⁻ⁿ` is a ratio of Fibonacci numbers.
    #[test]
    fn four_notes_ascending() {
        let got = ms(&attack_offsets(4, -1.0, 0.144));
        assert_eq!(got, vec![0.0, 55.0, 89.0, 110.01]);
        let gaps: Vec<f32> = got.windows(2).map(|w| ((w[1] - w[0]) * 100.0).round() / 100.0).collect();
        assert_eq!(gaps, vec![55.0, 34.0, 21.01]); // Fibonacci, descending — accelerating
    }

    #[test]
    fn four_notes_descending_is_the_mirror() {
        assert_eq!(ms(&attack_offsets(4, 1.0, 0.144)), vec![110.01, 89.0, 55.0, 0.0]);
    }

    #[test]
    fn five_notes_full_polyphony() {
        assert_eq!(ms(&attack_offsets(5, -1.0, 0.144)), vec![0.0, 55.0, 89.0, 110.01, 122.99]);
    }

    #[test]
    fn half_bias_halves_the_span() {
        assert_eq!(ms(&attack_offsets(5, -0.5, 0.144)), vec![0.0, 27.5, 44.5, 55.0, 61.5]);
    }

    /// A release is the attack's gaps in reverse — decelerating where the
    /// attack accelerated.
    #[test]
    fn releases_mirror_the_attack() {
        let got = ms(&release_offsets(4, -1.0, 0.144));
        assert_eq!(got, vec![0.0, 21.01, 55.0, 110.01]);
    }

    /// No discontinuity at zero: the block chord is the limit, not a branch.
    #[test]
    fn bias_zero_is_a_block_chord_and_is_continuous() {
        assert_eq!(attack_offsets(4, 0.0, 0.144), vec![0.0; 4]);
        let tiny = attack_offsets(4, -0.001, 0.144);
        assert!(tiny.iter().all(|&t| t < 0.000_2), "{tiny:?}");
        // and it approaches the full strum smoothly
        let half = attack_offsets(4, -0.5, 0.144);
        let full = attack_offsets(4, -1.0, 0.144);
        for i in 0..4 {
            assert!((half[i] * 2.0 - full[i]).abs() < 1e-6);
        }
    }

    /// One note, or none, must not panic or produce a negative offset.
    #[test]
    fn degenerate_chords_are_safe() {
        assert_eq!(attack_offsets(0, -1.0, 0.144), Vec::<f32>::new());
        assert_eq!(attack_offsets(1, -1.0, 0.144), vec![0.0]);
        assert_eq!(release_offsets(1, 1.0, 0.144), vec![0.0]);
        for c in 0..6 {
            for &b in &[-1.0f32, -0.3, 0.0, 0.3, 1.0] {
                assert!(attack_offsets(c, b, 0.144).iter().all(|&t| t >= 0.0 && t.is_finite()));
                assert!(release_offsets(c, b, 0.144).iter().all(|&t| t >= 0.0 && t.is_finite()));
            }
        }
    }

    /// The budget scales with the chord rate and then stops.
    #[test]
    fn the_budget_is_derived_then_capped() {
        assert!((budget_s(1.597) - 0.144).abs() < 1e-3);
        assert_eq!(budget_s(100.0), BUDGET_MAX_S);
        // Always well inside the gesture it sits under.
        for iv in [0.5f32, 1.0, 2.0, 3.2, 8.0] {
            assert!(budget_s(iv) < iv / PHI as f32, "strum outran the gesture at {iv}s");
        }
    }
}
