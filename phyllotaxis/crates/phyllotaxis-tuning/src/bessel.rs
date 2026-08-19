//! Bessel functions of the first kind — the sideband amplitudes of FM.
//!
//! A carrier at `c` modulated at `m` with index `I` produces partials at
//! `c + n·m` for every integer `n`, each with amplitude `J_n(I)`. So the
//! spectrum of every FM algorithm in the instrument is a Bessel evaluation,
//! and the whole tuning table is downstream of this file being right.
//!
//! The power series is used rather than a recurrence, because the indices this
//! instrument reaches are small (`I` tops out around 12) and the series is
//! exact there. It is built **iteratively** — each term from the last — so
//! neither `(x/2)^n` nor `n!` is ever formed on its own and nothing overflows
//! at orders where the result is still meaningful.

/// Terms of the series before giving up. Convergence is factorial, so this is
/// reached only by arguments far outside anything the instrument asks for.
const MAX_TERMS: usize = 128;

/// Below this a term contributes nothing an `f32` audio path could hear.
const TERM_FLOOR: f64 = 1e-18;

/// `J_n(x)`, the Bessel function of the first kind of integer order.
pub fn j(n: i32, x: f64) -> f64 {
    // J_{-n}(x) = (-1)ⁿ J_n(x). Lower sidebands are the upper ones reflected,
    // which is why an FM spectrum is symmetric in magnitude about the carrier.
    if n < 0 {
        let positive = j(-n, x);
        return if (-n) % 2 == 0 { positive } else { -positive };
    }

    let n = n as u32;
    let half = x / 2.0;

    // First term: (x/2)ⁿ / n!, accumulated a factor at a time.
    let mut term = 1.0f64;
    for k in 1..=n {
        term *= half / k as f64;
    }

    let mut sum = term;
    let half_sq = half * half;
    for k in 0..MAX_TERMS {
        term *= -half_sq / (((k + 1) as f64) * ((n as usize + k + 1) as f64));
        sum += term;
        if term.abs() < TERM_FLOOR {
            break;
        }
    }
    sum
}

/// How many sidebands either side of the carrier are worth generating at this
/// index.
///
/// The usual engineering rule is Carson's `I + 1`, which is a bandwidth
/// estimate — it keeps the energy that matters for *transmission*. That is the
/// wrong criterion here: a partial dropped from the spectrum is a dissonance
/// contribution that never happens, and so a scale degree that never appears.
/// The rule is widened to `I + 4√I + 6`, which holds the truncation loss below
/// −120 dB across the whole index range the instrument reaches.
///
/// The cost is bounded because generation prunes as it goes: sidebands below
/// the amplitude floor are discarded before they can multiply into the next
/// modulator's set.
pub fn significant_order(index: f64) -> i32 {
    let index = index.abs();
    index.ceil() as i32 + (4.0 * index.sqrt()).ceil() as i32 + 6
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Published values, to six places.
    #[test]
    fn matches_known_values() {
        let cases = [
            (0, 0.0, 1.0),
            (1, 0.0, 0.0),
            (0, 1.0, 0.765_197_686),
            (1, 1.0, 0.440_050_586),
            (1, 2.0, 0.576_724_808),
            (2, 2.0, 0.352_834_029),
            (0, 5.0, -0.177_596_771),
            (3, 10.0, 0.058_379_379),
        ];
        for (n, x, expected) in cases {
            let got = j(n, x);
            assert!(
                (got - expected).abs() < 1e-8,
                "J_{n}({x}) = {got}, expected {expected}"
            );
        }
    }

    /// The first zero of J_0 is the Bessel constant 2.404825557…
    #[test]
    fn finds_first_zero_of_j0() {
        assert!(j(0, 2.404_825_557).abs() < 1e-8);
    }

    /// Reflection: lower sidebands mirror upper ones, inverting on odd orders.
    #[test]
    fn negative_orders_reflect() {
        for n in 1..8 {
            let x = 3.7;
            let expected = if n % 2 == 0 { j(n, x) } else { -j(n, x) };
            assert!((j(-n, x) - expected).abs() < 1e-12);
        }
    }

    /// Energy is conserved across the sidebands: Σ J_n(x)² = 1. This is the
    /// property that makes FM index a timbre control rather than a volume one,
    /// and it is the strongest single check on the series.
    ///
    /// Summed over *all* orders it is exact; summed over the orders
    /// [`significant_order`] generates, the shortfall is the energy this crate
    /// throws away. The tolerance is therefore a statement about the truncation
    /// rather than about the series: below −120 dB, which no partial omitted at
    /// that level could move a dissonance minimum by.
    #[test]
    fn truncated_sidebands_keep_effectively_all_the_energy() {
        for &x in &[0.5, 1.0, 3.0, 7.5, 12.0] {
            let order = significant_order(x);
            let total: f64 = (-order..=order).map(|n| j(n, x).powi(2)).sum();
            assert!((total - 1.0).abs() < 1e-12, "Σ J_n({x})² = {total}");
        }
    }

    /// Taken far enough, the identity is exact — which separates a truncation
    /// shortfall from an error in the series itself.
    #[test]
    fn sidebands_conserve_energy_exactly_when_untruncated() {
        for &x in &[0.5, 1.0, 3.0, 7.5, 12.0] {
            let order = significant_order(x) + 24;
            let total: f64 = (-order..=order).map(|n| j(n, x).powi(2)).sum();
            assert!((total - 1.0).abs() < 1e-12, "Σ J_n({x})² = {total}");
        }
    }
}
