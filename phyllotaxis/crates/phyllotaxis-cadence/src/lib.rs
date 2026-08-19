//! Cadence — the harmony engine.
//!
//! Three mechanisms, each derived and then adversarially re-checked before it
//! became code, because a wrong test vector locks in a wrong implementation:
//!
//! - [`word`] — the Fibonacci word, deciding how long each chord lasts.
//! - [`mirror`] — negative harmony, reflecting pitch about a fixed axis.
//! - [`strum`] — φ-spaced note-event offsetting, bipolar.
//!
//! The two "mirrors" turned out to be **one mechanism**. The schedule that
//! decides which chords get reflected is `frac(n(φ−1) + β) < m`: a Sturmian
//! set of density exactly `m`, aperiodic, with gaps bounded to three distinct
//! values by the three-distance theorem — so mirrored chords can never clump.
//! At `m = 1/φ² ≈ 0.382` that schedule *is* the Fibonacci word. Asking for
//! both was asking for one thing at a particular density.

pub mod mirror;
pub mod strum;
pub mod word;

/// Which chords get reflected: a golden rotation thresholded at `m`.
///
/// No RNG. Deterministic in `n` and the preset's `β`, so a preset is a
/// coordinate rather than a snapshot, per `DESIGN.md` §10.
#[inline]
pub fn mirror_this_chord(n: u64, beta: f64, amount: f64) -> bool {
    if amount <= 0.0 {
        return false;
    }
    if amount >= 1.0 {
        return true;
    }
    ((n as f64) * word::INV_PHI + beta).fract() < amount
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Density is exactly the control value — not approximately.
    #[test]
    fn the_mirror_schedule_has_the_density_asked_for() {
        for &m in &[0.1f64, 0.382, 0.5, 0.75] {
            let n = 20_000u64;
            let hits = (1..=n).filter(|&k| mirror_this_chord(k, 0.0, m)).count();
            let got = hits as f64 / n as f64;
            assert!((got - m).abs() < 2e-3, "asked {m}, got {got}");
        }
    }

    /// **The two mirrors are one mechanism.** At `m = 1/φ²` the schedule is
    /// the Fibonacci word itself.
    #[test]
    fn at_one_over_phi_squared_the_schedule_is_the_fibonacci_word() {
        for n in 1..10_000u64 {
            let scheduled = mirror_this_chord(n, 0.0, word::INV_PHI2);
            assert_eq!(
                scheduled,
                word::word(n) == 1,
                "schedule and word disagree at n = {n}"
            );
        }
    }

    /// Three-distance theorem: the gaps between mirrored chords take at most
    /// three distinct values, so the mirror cannot clump the way a coin would.
    #[test]
    fn mirrored_chords_never_clump() {
        let hits: Vec<u64> = (1..=4000u64).filter(|&k| mirror_this_chord(k, 0.0, 0.3)).collect();
        let mut gaps: Vec<u64> = hits.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.sort_unstable();
        gaps.dedup();
        assert!(gaps.len() <= 3, "expected at most three gap lengths, got {gaps:?}");
    }

    #[test]
    fn the_extremes_are_exact() {
        assert!(!mirror_this_chord(7, 0.3, 0.0));
        assert!(mirror_this_chord(7, 0.3, 1.0));
    }
}
