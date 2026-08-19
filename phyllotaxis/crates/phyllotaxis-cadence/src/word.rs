//! The Fibonacci word — Cadence's time mirror.
//!
//! `S(1) = "0"`, `S(2) = "01"`, `S(n) = S(n-1) S(n-2)`. Sturmian: aperiodic,
//! self-similar, and with the ratio of 0s to 1s converging on φ. It decides
//! **how long each chord lasts** rather than whether one happens — two
//! durations in an order that never repeats.
//!
//! ## The whole time state is one integer
//!
//! No buffer, no table, no accumulator: the n-th symbol has a closed form, so
//! the audio side allocates nothing and `n` alone is the §10 preset coordinate
//! and the thing STEP moves.
//!
//! ## Why `0 → long` is derived rather than chosen
//!
//! With the long step `L = φ²/√5 · T̄` and the short `S = φ/√5 · T̄`, the
//! duration of the n-th Fibonacci block is exactly `S · φⁿ`. Reverse the
//! assignment and that identity disappears. The word is not decorating a
//! rhythm — the rhythm *is* the word's own growth law.
//!
//! ## And it closes a ladder with the field
//!
//! `DESIGN.md` §5 makes the gesture `interval/φ`. Feed these two durations in
//! and the gesture of a long step **is** the short step, exactly, and the rest
//! after a long step is the gesture of a short one. The two articulations are
//! not two unrelated shapes; they are one shape at adjacent rungs.

/// `φ`, `1/φ`, `1/φ²` — the threshold is `1/φ²`.
pub const PHI: f64 = 1.618_033_988_749_895;
pub const INV_PHI: f64 = 0.618_033_988_749_895;
pub const INV_PHI2: f64 = 0.381_966_011_250_105;

/// Long step, as a multiple of the mean interval: `φ²/√5`.
pub const W_LONG: f64 = 1.170_820_393_249_937;
/// Short step: `φ/√5`.
pub const W_SHORT: f64 = 0.723_606_797_749_979;

/// The n-th symbol, 1-based. Working generator.
///
/// Exact to `n ≈ 10⁸`; the first disagreement with exact arithmetic is at
/// `n = 102_334_155 = F(40)`, which at these tempos is about eighty years of
/// continuous playing.
#[inline]
pub fn word(n: u64) -> u8 {
    if ((n as f64) * INV_PHI).fract() < INV_PHI2 { 1 } else { 0 }
}

/// The same symbol by exact integer arithmetic — the oracle the fast path is
/// tested against, never used at runtime.
///
/// `⌊nφ⌋ = (n + ⌊√(5n²)⌋) / 2`, and flooring the root first cannot cross the
/// halving boundary because `5n²` is never a perfect square for `n ≥ 1`.
pub fn word_exact(n: u64) -> u8 {
    fn floor_n_phi(n: u64) -> u64 {
        (n + (5 * n * n).isqrt()) / 2
    }
    2 - (floor_n_phi(n + 1) - floor_n_phi(n)) as u8
}

/// How long chord `n` lasts, given the mean interval.
#[inline]
pub fn interval_s(n: u64, mean_s: f32) -> f32 {
    mean_s * if word(n) == 0 { W_LONG as f32 } else { W_SHORT as f32 }
}

/// The first `count` symbols, for display and for tests.
pub fn prefix(count: usize) -> String {
    (1..=count as u64).map(|n| if word(n) == 1 { '1' } else { '0' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The word itself, hand-computed from the concatenation rule.
    #[test]
    fn the_first_twenty_symbols() {
        assert_eq!(prefix(20), "01001010010010100101");
        assert_eq!(prefix(30), "010010100100101001010010010100");
    }

    /// The threshold form and the concatenation form are the same object.
    #[test]
    fn the_fast_path_matches_exact_arithmetic() {
        for n in 1..200_000u64 {
            assert_eq!(word(n), word_exact(n), "disagreement at n = {n}");
        }
    }

    /// Density of 1s converges on `1/φ²`; of 0s on `1/φ`.
    #[test]
    fn the_symbol_density_is_golden() {
        let n = 100_000u64;
        let ones = (1..=n).filter(|&k| word(k) == 1).count() as f64 / n as f64;
        assert!((ones - INV_PHI2).abs() < 1e-4, "density of ones is {ones}");
    }

    /// **The derivation.** With `0 → long`, the n-th Fibonacci block lasts
    /// exactly `φⁿ` short steps. This is why the assignment is not a choice.
    #[test]
    fn a_fibonacci_block_lasts_phi_to_the_n() {
        // Block lengths are Fibonacci: |S(n)| = F(n).
        let (mut a, mut b) = (1u64, 2u64); // |S(1)|, |S(2)|
        let mut blocks = vec![a, b];
        while *blocks.last().unwrap() < 5000 {
            let next = a + b;
            blocks.push(next);
            a = b;
            b = next;
        }
        for (i, &len) in blocks.iter().enumerate().take(10) {
            let n = i + 1;
            let dur: f64 = (1..=len).map(|k| if word(k) == 0 { PHI } else { 1.0 }).sum();
            let expect = PHI.powi(n as i32);
            assert!(
                (dur - expect).abs() < 1e-6,
                "S({n}) lasts {dur} short steps, expected φ^{n} = {expect}"
            );
        }
    }

    /// **The ladder closes with the field.** `gesture(long) == short`, and
    /// `rest(long) == gesture(short)` — exactly, not approximately.
    #[test]
    fn the_gesture_of_a_long_step_is_a_short_step() {
        let mean = 1.618_034_f32;
        let long = mean * W_LONG as f32;
        let short = mean * W_SHORT as f32;
        let gesture = |iv: f32| iv / PHI as f32;
        let rest = |iv: f32| iv - gesture(iv);

        assert!((gesture(long) - short).abs() < 1e-5, "{} vs {}", gesture(long), short);
        assert!((rest(long) - gesture(short)).abs() < 1e-5);
        assert!((gesture(long) + rest(long) - long).abs() < 1e-5);
    }

    /// It never drifts against a metronome: any window of any length stays
    /// within `L - S = 1/√5` of the mean. That is what makes it a clock rather
    /// than a rubato.
    #[test]
    fn the_clock_never_drifts() {
        let bound = 1.0 / 5.0f64.sqrt();
        let dur = |n: u64| if word(n) == 0 { W_LONG } else { W_SHORT };
        let mut cum = vec![0.0f64];
        for n in 1..=2000u64 {
            cum.push(cum[(n - 1) as usize] + dur(n));
        }
        let mut worst = 0.0f64;
        for start in 0..cum.len() {
            for k in 1..=400.min(cum.len() - 1 - start) {
                let d = cum[start + k] - cum[start] - k as f64;
                worst = worst.max(d.abs());
            }
        }
        assert!(worst < bound, "window deviation {worst} exceeded {bound}");
        eprintln!("worst window deviation: {worst:.6} (bound {bound:.6})");
    }

    #[test]
    fn intervals_alternate_between_two_durations_only() {
        let mut seen: Vec<f32> = (1..=500u64).map(|n| interval_s(n, 2.0)).collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        seen.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(seen.len(), 2, "expected exactly two durations, got {seen:?}");
    }
}
