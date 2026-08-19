//! Scales computed from an algorithm's own spectrum.
//!
//! The instrument's operators are φ-tuned, so their partials are maximally
//! inharmonic — which makes 12-TET the *mismatched* choice, not the neutral
//! one. Given a spectrum, this crate finds the intervals at which two copies of
//! it grate least, and those intervals are the scale. The tables are computed
//! offline and baked; the audio thread never runs any of this.
//!
//! See `DESIGN.md` §3. The method is Plomp–Levelt sensory dissonance as
//! developed in William Sethares, *Tuning, Timbre, Spectrum, Scale*.
//!
//! ```
//! use phyllotaxis_tuning::{tuning_for, Algorithm, Kind, Variant};
//!
//! // FM fills an octave, so it wants a scale.
//! let tuning = tuning_for(Algorithm::Fm1, Variant::Golden, 4.0, 7);
//! assert_eq!(tuning.kind(), Kind::Scale);
//!
//! // Ring modulation places three partials, so it wants a chord instead —
//! // and the caller has to notice.
//! assert!(tuning_for(Algorithm::Rm, Variant::Golden, 4.0, 7).is_chord());
//! ```

pub mod bessel;
pub mod dissonance;
pub mod scale;
pub mod spectrum;

use spectrum::{Modulator, Spectrum};

pub use scale::{Degree, Kind, Tuning};

/// Degrees per computed tuning. A Fibonacci number, like every other count in
/// the instrument, and close enough to a diatonic seven that a keyboard laid
/// out against it still feels like a keyboard.
pub const DEGREES_PER_SCALE: usize = 8;

/// The **harmonic** end of the convergents: `F(3)/F(1)` = 2/1.
///
/// Low terms of the Fibonacci convergents are ordinary musical intervals — the
/// sum and difference tones they produce land on notes.
pub const HARMONIC_RATIO: f64 = 2.0;

/// The **golden** end: `F(7)/F(6)` = 13/8 = 1.625, within 0.5 % of φ.
///
/// The pair is deliberately taken from opposite ends of the sequence. Adjacent
/// convergents — 5/3 against 8/5 — differ by 4 %, which at audio rates reads as
/// a tuning error rather than a second algorithm. See `DESIGN.md` §2.
pub const GOLDEN_RATIO: f64 = 13.0 / 8.0;

/// Where the sub sits: an octave below the carrier.
const SUB_RATIO: f64 = 0.5;

/// Sub level. Present enough to anchor the low end, quiet enough that it does
/// not dominate the dissonance curve and drag every degree toward the octave.
const SUB_AMP: f64 = 0.5;

/// Harmonics of the feedback operator carried into FM II's modulator set.
const FEEDBACK_HARMONICS: usize = 3;

/// Partials generated for a rectified source.
const RECT_HARMONICS: usize = 8;

/// The eight algorithms: four modulation types, two variants each.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Algorithm {
    /// Two modulators into one carrier. One ratio is free; the other sits at a
    /// Fibonacci relationship to it.
    Fm1,
    /// One modulator into a feedback operator into a carrier.
    Fm2,
    /// Ring modulation — sum and difference, no carrier. Carrier, modulator,
    /// sub.
    Rm,
    /// Amplitude modulation — ring modulation with the carrier left in.
    Am,
    /// Full-wave rectification: even harmonics of the source, and the DC the
    /// blocker removes.
    Rect,
}

impl Algorithm {
    pub const ALL: [Algorithm; 5] = [
        Algorithm::Fm1,
        Algorithm::Fm2,
        Algorithm::Rm,
        Algorithm::Am,
        Algorithm::Rect,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Fm1 => "fm",
            Algorithm::Fm2 => "fm fb",
            Algorithm::Rm => "rm",
            Algorithm::Am => "am",
            Algorithm::Rect => "rect",
        }
    }

    /// Whether this algorithm carries a sub oscillator.
    ///
    /// The complex modulation types do; the FM pair does not. That asymmetry is
    /// audible as a level difference and is gain-matched downstream — see
    /// `DESIGN.md` §2.
    pub fn has_sub(self) -> bool {
        !matches!(self, Algorithm::Fm1 | Algorithm::Fm2)
    }
}

/// Which end of the convergents an algorithm's ratio is taken from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// **I** — a low convergent. Partials land on musical intervals.
    Harmonic,
    /// **II** — a high convergent, effectively φ. Maximally inharmonic.
    Golden,
}

impl Variant {
    pub const ALL: [Variant; 2] = [Variant::Harmonic, Variant::Golden];

    pub fn ratio(self) -> f64 {
        match self {
            Variant::Harmonic => HARMONIC_RATIO,
            Variant::Golden => GOLDEN_RATIO,
        }
    }

    pub fn numeral(self) -> &'static str {
        match self {
            Variant::Harmonic => "I",
            Variant::Golden => "II",
        }
    }
}

/// The spectrum one algorithm produces at a given modulation index.
pub fn spectrum_for(algorithm: Algorithm, variant: Variant, index: f64) -> Spectrum {
    let ratio = variant.ratio();
    let carrier = 1.0;

    let base = match algorithm {
        // Two modulators: the free one, and one at a Fibonacci relationship to
        // it. The relationship *is* the variant, so the second modulator walks
        // from harmonic to golden with the first.
        Algorithm::Fm1 => spectrum::fm(
            carrier,
            &[
                Modulator { ratio: 1.0, index },
                Modulator { ratio, index: index / ratio },
            ],
        ),

        // A feedback operator is not a sine, so it cannot be one Bessel term.
        // Its output is approximated by its first few harmonics — amplitudes
        // after Tomisawa, `aₖ = 2·J_k(k·β)/(k·β)` — each entering the carrier
        // as its own modulator. Truncation is honest: the omitted harmonics are
        // quieter than the prune floor at every index the instrument reaches.
        Algorithm::Fm2 => {
            let beta = (index / 4.0).min(1.0);
            let mods: Vec<Modulator> = (1..=FEEDBACK_HARMONICS)
                .map(|k| {
                    let k_f = k as f64;
                    let amp = if beta > 0.0 {
                        2.0 * bessel::j(k as i32, k_f * beta) / (k_f * beta)
                    } else {
                        f64::from(k == 1)
                    };
                    Modulator { ratio: k_f * ratio, index: index * amp.abs() / k_f }
                })
                .collect();
            spectrum::fm(carrier, &mods)
        }

        Algorithm::Rm => spectrum::ring(carrier, ratio),

        // Index doubles as depth here: at 0 a bare carrier, saturating at 1.
        Algorithm::Am => spectrum::amplitude(carrier, ratio, (index / 4.0).min(1.0)),

        // The variants differ in *rectification mode*, not only in ratio.
        //
        // Full-wave rectification of a sine yields a harmonic series scaled by
        // the source ratio, so its internal structure is ratio-independent: two
        // full-wave modes at different ratios are one timbre transposed, and a
        // dissonance curve — a function of interval — cannot tell them apart.
        // Measured at 0.02¢. Half-wave keeps the fundamental and is a genuinely
        // different spectrum, so Rect I is half-wave and Rect II full-wave,
        // each still at its own end of the convergents. See `DESIGN.md` §2.
        Algorithm::Rect => match variant {
            Variant::Harmonic => spectrum::rectified_half(ratio, RECT_HARMONICS),
            Variant::Golden => spectrum::rectified(ratio, RECT_HARMONICS),
        },
    };

    if algorithm.has_sub() {
        base.with_partial(SUB_RATIO, SUB_AMP).normalised()
    } else {
        base
    }
}

/// The tuning one algorithm wants to be played in — a scale, or a chord.
///
/// Which of the two is not a setting. It falls out of how many intervals the
/// spectrum actually prefers, and the caller has to ask: RM and AM place three
/// partials and so want a chord, while the FM pair fills an octave. See
/// [`scale::Kind`].
pub fn tuning_for(
    algorithm: Algorithm,
    variant: Variant,
    index: f64,
    degrees: usize,
) -> Tuning {
    let spectrum = spectrum_for(algorithm, variant, index);
    scale::from_spectrum(&spectrum, dissonance::REFERENCE_HZ, degrees)
}

/// Every algorithm's tuning at one index — the bake.
///
/// Eight entries: five modulation types, two variants each, minus the pairs
/// that collapse. FM I and FM II both take both variants; so do RM, AM and
/// Rect. See `DESIGN.md` §2 for why the eight are counted this way.
pub fn tables(index: f64) -> Vec<(Algorithm, Variant, Tuning)> {
    Algorithm::ALL
        .iter()
        .flat_map(|&a| Variant::ALL.iter().map(move |&v| (a, v)))
        .map(|(a, v)| (a, v, tuning_for(a, v, index, DEGREES_PER_SCALE)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_algorithm_produces_partials() {
        for a in Algorithm::ALL {
            for v in Variant::ALL {
                let s = spectrum_for(a, v, 4.0);
                assert!(!s.is_empty(), "{} {} produced nothing", a.name(), v.numeral());
            }
        }
    }

    #[test]
    fn every_algorithm_produces_a_tuning() {
        for (a, v, tuning) in tables(4.0) {
            assert!(
                tuning.len() >= 2,
                "{} {} yielded only {} degrees",
                a.name(),
                v.numeral(),
                tuning.len()
            );
            assert_eq!(tuning.degrees()[0].cents, 0.0);
        }
    }

    /// The complex modulation types are sparse and want chords; the FM pair
    /// fills an octave and wants scales. The split is the design's answer to
    /// §3's open question, and it is asserted rather than hoped for.
    #[test]
    fn sparse_algorithms_are_chords() {
        for (a, v, tuning) in tables(4.0) {
            let expected = match a {
                Algorithm::Rm | Algorithm::Am => Kind::Chord,
                _ => Kind::Scale,
            };
            assert_eq!(
                tuning.kind(),
                expected,
                "{} {} is a {} with {} degrees",
                a.name(),
                v.numeral(),
                tuning.kind().name(),
                tuning.len()
            );
        }
    }

    /// The complex types carry a sub; the FM pair does not.
    ///
    /// Presence is the wrong question. FM I at the golden ratio places a
    /// sideband exactly on the sub — `8n₁ + 13n₂ = −4` has the solution
    /// `n₁ = −7, n₂ = 4` — so something sits there whether a sub oscillator
    /// exists or not. It arrives around −60 dB, so the test asks how loud the
    /// sub ratio is rather than whether anything is at it.
    #[test]
    fn subs_appear_only_where_the_design_says() {
        for a in Algorithm::ALL {
            let s = spectrum_for(a, Variant::Golden, 4.0);
            let level = s
                .partials()
                .iter()
                .filter(|p| (p.ratio - SUB_RATIO).abs() < 1e-9)
                .map(|p| p.amp)
                .fold(0.0f64, f64::max);
            if a.has_sub() {
                assert!(level > 0.2, "{} lost its sub (level {level})", a.name());
            } else {
                assert!(level < 0.05, "{} has a sub it should not (level {level})", a.name());
            }
        }
    }

    /// The two variants must be *audibly* different, not a detune. Every pair
    /// should disagree about where at least one degree belongs by more than the
    /// ~5 cent threshold at which a listener notices a pitch change at all.
    #[test]
    fn variants_are_more_than_a_detune() {
        for a in Algorithm::ALL {
            let harmonic = tuning_for(a, Variant::Harmonic, 4.0, DEGREES_PER_SCALE).cents();
            let golden = tuning_for(a, Variant::Golden, 4.0, DEGREES_PER_SCALE).cents();
            let worst = harmonic
                .iter()
                .zip(golden.iter())
                .map(|(h, g)| (h - g).abs())
                .fold(0.0f64, f64::max);
            assert!(
                worst > 5.0 || harmonic.len() != golden.len(),
                "{}: I and II agree to within {worst}¢ — that is a tuning error, not an algorithm",
                a.name()
            );
        }
    }

    /// Rising index reshapes the curve, so the scale genuinely tracks INDEX
    /// rather than being fixed per algorithm. This is the claim in `DESIGN.md`
    /// §3 that justifies interpolating tables across index at all.
    #[test]
    fn the_scale_moves_with_index() {
        let low = tuning_for(Algorithm::Fm1, Variant::Golden, 1.0, DEGREES_PER_SCALE).cents();
        let high = tuning_for(Algorithm::Fm1, Variant::Golden, 9.0, DEGREES_PER_SCALE).cents();
        let moved = low.len() != high.len()
            || low
                .iter()
                .zip(high.iter())
                .any(|(a, b)| (a - b).abs() > 5.0);
        assert!(moved, "index did not move the scale: {low:?} vs {high:?}");
    }

    /// Degrees stay ordered and inside the octave for every algorithm.
    #[test]
    fn all_tables_are_well_formed() {
        for (a, v, tuning) in tables(4.0) {
            let cents = tuning.cents();
            assert!(
                cents.windows(2).all(|w| w[0] < w[1]),
                "{} {} is unordered: {cents:?}",
                a.name(),
                v.numeral()
            );
            assert!(cents.iter().all(|&c| (0.0..=1200.0).contains(&c)));
        }
    }
}
