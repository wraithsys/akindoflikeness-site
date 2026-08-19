//! The five-integer signature each roster entry moves by.
//!
//! Five components are summed to make the movement, and their rates are the
//! signature's integers normalised against the largest. Reused from
//! `fibonacci-synth`'s `breath.rs`, extended from five sequences to eight.
//!
//! ## What "commensurate" actually means, stated precisely
//!
//! `breath.rs` says harmonic mode's integers are commensurate "so its movement
//! repeats", and that the others, built on irrational limits, never do. The
//! first half is right and the second half is not: **every** integer signature
//! repeats, because every ratio between integers is rational. What differs is
//! the *period*, and it has a closed form.
//!
//! With rates `rᵢ = aᵢ / max(a)`, all five phases realign after
//!
//! ```text
//!     T = max(a) / gcd(a)      base cycles
//! ```
//!
//! Harmonic — 30, 24, 18, 12, 6 — has `gcd = 6`, so `T = 5`: it comes back
//! around five base cycles later, which at a drone's rate is often enough to
//! hear. Fibonacci — 34, 21, 13, 8, 5 — has `gcd = 1`, so `T = 34`, six times
//! longer and past the point anyone tracks it. The distinction is real and the
//! original prose was reaching for it; it is a short period against a long one,
//! not periodicity against its absence.
//!
//! That makes the property **computable**, so the roster's one deliberately
//! repeating entry is asserted by a test rather than by a comment.

use phyllotaxis_tuning::{Algorithm, Variant};

/// Rungs of φ between a voice's frequency and its movement. Fibonacci 13, the
/// same 13 as `breath.rs`.
pub const RATE_RUNGS: i32 = 13;

/// Amplitude falls by φ per component, so the first term dominates.
///
/// This is what makes a *mirrored* signature mean anything: reversed integers
/// are the same set of rates, and with equal amplitudes the two would produce
/// an identical movement. Weighted, one leads with its fastest component and
/// the other with its slowest — a flutter against a heave.
const AMP_FALLOFF: f32 = 1.618_034;

/// One entry's movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature {
    pub terms: [u32; 5],
    pub family: &'static str,
}

impl Signature {
    /// Base cycles before all five components realign: `max / gcd`.
    pub fn period(&self) -> u32 {
        let max = self.terms.iter().copied().max().unwrap_or(1);
        max / gcd_all(&self.terms)
    }

    /// Component rates, normalised against the largest term.
    pub fn rates(&self) -> [f32; 5] {
        let max = self.terms.iter().copied().max().unwrap_or(1) as f32;
        core::array::from_fn(|i| self.terms[i] as f32 / max)
    }

    /// Component amplitudes, falling by φ and summing to one.
    pub fn amps(&self) -> [f32; 5] {
        let raw: [f32; 5] = core::array::from_fn(|i| AMP_FALLOFF.powi(-(i as i32)));
        let total: f32 = raw.iter().sum();
        core::array::from_fn(|i| raw[i] / total)
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn gcd_all(xs: &[u32]) -> u32 {
    xs.iter().copied().fold(0, gcd).max(1)
}

/// The signature for a roster entry.
///
/// Seven sequences with `gcd = 1` and one without. **The commensurate one goes
/// to fm I**, and not by taste: `phyllotaxis-tuning`'s coincidence count
/// measures fm I at 27 exactly-coincident partial pairs and *zero* beating
/// pairs, where every other entry beats. It is the one voice on the roster
/// whose partials reinforce instead of shimmering — so it is the one whose
/// movement is allowed to come back around. The entry you would expect to
/// repeat is the only one that does, and this time that is derived rather than
/// noticed.
pub fn signature_for(algorithm: Algorithm, variant: Variant) -> Signature {
    match (algorithm, variant) {
        // Harmonic — the only short period on the roster. T = 5.
        (Algorithm::Fm1, _) => Signature { terms: [30, 24, 18, 12, 6], family: "harmonic" },
        // Fibonacci. T = 34.
        (Algorithm::Fm2, _) => Signature { terms: [34, 21, 13, 8, 5], family: "fibonacci" },
        // Lucas, and Lucas reversed — same limit, opposite weighting. T = 29.
        (Algorithm::Rm, Variant::Harmonic) => Signature { terms: [29, 18, 11, 7, 4], family: "lucas" },
        (Algorithm::Rm, Variant::Golden) => Signature { terms: [4, 7, 11, 18, 29], family: "lucas mirrored" },
        // Padovan and Perrin — both limit on the plastic number, different
        // integers, so the pair is related the way the RM pair's ratios are.
        (Algorithm::Am, Variant::Harmonic) => Signature { terms: [28, 21, 16, 12, 9], family: "padovan" },
        (Algorithm::Am, Variant::Golden) => Signature { terms: [29, 22, 17, 12, 10], family: "perrin" },
        // Tribonacci (T = 44, the longest) and Pell, on the silver ratio.
        (Algorithm::Rect, Variant::Harmonic) => Signature { terms: [44, 24, 13, 7, 4], family: "tribonacci" },
        (Algorithm::Rect, Variant::Golden) => Signature { terms: [29, 12, 5, 2, 1], family: "pell" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phyllotaxis_tuning::ROSTER;

    /// Exactly one entry repeats soon enough to hear, and it is fm I.
    #[test]
    fn one_signature_is_commensurate_and_it_is_the_reinforcing_one() {
        let short: Vec<_> = ROSTER
            .iter()
            .filter(|&&(a, v)| signature_for(a, v).period() < 10)
            .collect();
        assert_eq!(short.len(), 1, "expected exactly one short period, got {short:?}");
        assert_eq!(short[0].0, Algorithm::Fm1);
        assert_eq!(signature_for(Algorithm::Fm1, Variant::Golden).period(), 5);
    }

    /// Every other signature runs long enough that nobody tracks it.
    #[test]
    fn the_rest_are_long() {
        for &(a, v) in ROSTER.iter() {
            if a == Algorithm::Fm1 { continue; }
            let s = signature_for(a, v);
            assert!(s.period() >= 28, "{} has period {}", s.family, s.period());
        }
    }

    /// A mirrored signature must not collapse into its original.
    ///
    /// This is not hypothetical. In `fibonacci-synth` it *does* collapse:
    /// `golden` and `golden mirrored` normalise to the same rate multiset, and
    /// `begin()` resets all five phases to one value, so after any note the two
    /// modes are bit-identical — measured at a mean absolute difference of
    /// 2.3e-17. Its distinctness test only ever compares the opening state and
    /// so never fires.
    ///
    /// Two things here prevent it, and both are load-bearing: amplitude falls
    /// by φ with component *index*, so reversing the terms changes which rate
    /// is loudest; and `strike()` does not touch phase, so a note does not
    /// erase the per-voice offset either.
    #[test]
    fn mirroring_changes_the_movement() {
        let fwd = signature_for(Algorithm::Rm, Variant::Harmonic);
        let rev = signature_for(Algorithm::Rm, Variant::Golden);
        assert_eq!(fwd.period(), rev.period());
        let (fr, rr) = (fwd.rates(), rev.rates());
        let amps = fwd.amps();
        // The amplitude-weighted mean rate differs, which is the audible part.
        let mean = |r: [f32; 5]| r.iter().zip(amps).map(|(a, b)| a * b).sum::<f32>();
        assert!((mean(fr) - mean(rr)).abs() > 0.25, "{} vs {}", mean(fr), mean(rr));
    }

    /// The same collapse, caught at the level it actually happens: identical
    /// rate multisets must still produce different movement over real time.
    #[test]
    fn a_mirrored_pair_does_not_render_identically() {
        use crate::{Field, FieldParams};
        let p = FieldParams::default();
        let mut a = Field::new(48_000.0, Algorithm::Rm, Variant::Harmonic);
        let mut b = Field::new(48_000.0, Algorithm::Rm, Variant::Golden);
        for f in [&mut a, &mut b] {
            f.set_interval(2.0);
            f.strike();
        }
        let mut diff = 0.0f64;
        for _ in 0..48_000 * 4 {
            diff += (a.tick(220.0, &p) - b.tick(220.0, &p)).abs() as f64;
        }
        let mean = diff / (48_000.0 * 4.0);
        assert!(mean > 1e-3, "mirrored pair collapsed: mean difference {mean:e}");
    }

    #[test]
    fn every_roster_entry_has_a_distinct_signature() {
        let mut seen: Vec<[u32; 5]> = ROSTER.iter().map(|&(a, v)| signature_for(a, v).terms).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn amplitudes_sum_to_one() {
        let s = signature_for(Algorithm::Fm2, Variant::Golden);
        assert!((s.amps().iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }
}
