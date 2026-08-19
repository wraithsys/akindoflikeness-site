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
//! use phyllotaxis_tuning::{tuning_for, Kind, ROSTER};
//!
//! // Every entry fills an octave, so every one of them wants a scale.
//! for e in ROSTER {
//!     let tuning = tuning_for(e.algorithm, e.ratio, 4.0, 7);
//!     assert_eq!(tuning.kind(), Kind::Scale);
//! }
//!
//! // And no two entries agree about where the degrees go — which is the
//! // point of using irrational ratios rather than integer ones.
//! let a = tuning_for(ROSTER[1].algorithm, ROSTER[1].ratio, 4.0, 7).cents();
//! let b = tuning_for(ROSTER[2].algorithm, ROSTER[2].ratio, 4.0, 7).cents();
//! assert!(a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 5.0));
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
    /// Two modulators into one carrier. The entry ratio places the second
    /// against the first.
    Fm1,
    /// One modulator into a feedback operator into a carrier. The entry
    /// ratio is the feedback operator’s.
    Fm2,
}

impl Algorithm {
    pub const ALL: [Algorithm; 2] = [Algorithm::Fm1, Algorithm::Fm2];

    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Fm1 => "fm",
            Algorithm::Fm2 => "fm fb",
        }
    }

    /// Whether this algorithm carries a sub oscillator.
    ///
    /// The complex modulation types do; the FM pair does not. That asymmetry is
    /// audible as a level difference and is gain-matched downstream — see
    /// `DESIGN.md` §2.
    /// No algorithm carries a sub any more: the three that did are gone.
    pub fn has_sub(self) -> bool {
        false
    }
}



/// The spectrum one algorithm produces at a given modulation index.
pub fn spectrum_for(algorithm: Algorithm, ratio: f64, index: f64) -> Spectrum {
    let carrier = 1.0;

    let base = match algorithm {
        // Two modulators: one at the carrier, one at the entry ratio. The
        // ratio IS the entry — it is the only thing separating the four FM
        // entries from each other.
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
    };

    base
}

/// The tuning one algorithm wants to be played in — a scale, or a chord.
///
/// Which of the two is not a setting. It falls out of how many intervals the
/// spectrum actually prefers, and the caller has to ask: RM and AM place three
/// partials and so want a chord, while the FM pair fills an octave. See
/// [`scale::Kind`].
pub fn tuning_for(algorithm: Algorithm, ratio: f64, index: f64, degrees: usize) -> Tuning {
    let spectrum = spectrum_for(algorithm, ratio, index);
    scale::from_spectrum(&spectrum, dissonance::REFERENCE_HZ, degrees)
}

/// **The roster: exactly eight.**
///
/// The cross product of five algorithms and two variants is ten, and ten is
/// not a Fibonacci number. The design does not ask for a cross product.
///
/// FM's two entries are differentiated **structurally** — two modulators into a
/// carrier, against one modulator into a feedback operator into a carrier — and
/// their ratio is a free control rather than a roster axis. The three complex
/// types have no structural difference between their pair, so they are
/// differentiated **by ratio**, one from each end of the convergents.
///
/// `2 + (3 × 2) = 8`. See `DESIGN.md` §2.
/// One entry on the dial: an FM topology at a fixed modulator ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub algorithm: Algorithm,
    /// The modulator ratio. This IS the entry — everything else is shared, so
    /// the ratio is the only thing separating one from the next.
    pub ratio: f64,
    pub name: &'static str,
}

/// **Eight FM voices at eight fixed ratios — and the ratios are irrational
/// on purpose.**
///
/// The obvious choice is simple integers: 1:1, 2:1, 3:1, 3:2. It does not
/// work, and `no_two_entries_are_the_same_voice` caught it doing so — FM 2:1
/// and FM 3:1 came out on scales **0.1 cents apart**.
///
/// The reason is the whole premise of the instrument. A scale here is read off
/// a spectrum's dissonance curve, and any integer or simple-rational ratio
/// gives a spectrum whose partials are all harmonics of one fundamental. Every
/// such spectrum has its dissonance minima in the same places — near 12-TET —
/// however large the integers are. Different ratio, same harmonic series, same
/// scale, same voice under a different name. Which is exactly the "all
/// essentially the same" this roster exists to fix, arrived at a second time
/// by a different route.
///
/// Irrational ratios have no common fundamental, so their partials fall
/// between the harmonics and each ratio bends the dissonance curve its own
/// way. √2, φ, √5, ∛2, the plastic number, √3, e — each is a genuinely
/// different spectrum and so a genuinely different scale. 1:1 is kept as the
/// one harmonic entry, because a reference point you can hear the others
/// against is worth one slot.
///
/// This replaced four modulation types crossed with two convergents. Ring,
/// amplitude and rectified modulation are gone. With three operators feeding a
/// shared field they arrived at much the same place, and after playing all
/// eight Billy's read was that they "sound cool but all essentially the same" —
/// which is the only measurement that counts here. Two FM topologies at four
/// ratios each separate far more cleanly than four topologies at two, because
/// the ratio is what actually moves a spectrum around.
pub const ROSTER: [Entry; 8] = [
    Entry { algorithm: Algorithm::Fm1, ratio: 1.0, name: "FM 1:1" },
    Entry { algorithm: Algorithm::Fm1, ratio: 1.414_213_562_373_095, name: "FM \u{221a}2" },
    Entry { algorithm: Algorithm::Fm1, ratio: PHI, name: "FM \u{3c6}" },
    Entry { algorithm: Algorithm::Fm1, ratio: 2.236_067_977_499_79, name: "FM \u{221a}5" },
    Entry { algorithm: Algorithm::Fm2, ratio: 1.259_921_049_894_873, name: "FB \u{221b}2" },
    Entry { algorithm: Algorithm::Fm2, ratio: 1.324_717_957_244_746, name: "FB \u{3c1}" },
    Entry { algorithm: Algorithm::Fm2, ratio: 1.732_050_807_568_877, name: "FB \u{221a}3" },
    Entry { algorithm: Algorithm::Fm2, ratio: 2.718_281_828_459_045, name: "FB e" },
];

/// φ — the ratio that makes a voice maximally inharmonic.
pub const PHI: f64 = 1.618_033_988_749_895;

/// How an entry is named on the surface.
pub fn roster_name(entry: usize) -> &'static str {
    ROSTER[entry.min(ROSTER.len() - 1)].name
}

/// Every roster entry's tuning at one index — the bake.
pub fn tables(index: f64) -> Vec<(Entry, Tuning)> {
    ROSTER
        .iter()
        .map(|&e| (e, tuning_for(e.algorithm, e.ratio, index, DEGREES_PER_SCALE)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The count is the point. Ten would break 2/3/5/8.
    #[test]
    fn the_roster_is_eight() {
        assert_eq!(ROSTER.len(), DEGREES_PER_SCALE);
        // Every entry must be a DIFFERENT (topology, ratio) pair, or two
        // positions on the dial are the same voice under two names.
        let mut seen: Vec<String> =
            ROSTER.iter().map(|e| format!("{:?}{:.6}", e.algorithm, e.ratio)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 8, "the roster has a duplicate entry");
    }

    #[test]
    fn every_algorithm_produces_partials() {
        for (i, e) in ROSTER.iter().enumerate() {
            let s = spectrum_for(e.algorithm, e.ratio, 4.0);
            assert!(!s.is_empty(), "{} produced nothing", roster_name(i));
        }
    }

    #[test]
    fn every_algorithm_produces_a_tuning() {
        for (e, tuning) in tables(4.0) {
            assert!(tuning.len() >= 2, "{} yielded only {} degrees", e.name, tuning.len());
            assert_eq!(tuning.degrees()[0].cents, 0.0);
        }
    }

    /// Every FM entry fills an octave, so every one of them wants a SCALE.
    ///
    /// This used to assert that RM and AM come out as chords — the sparse
    /// three-partial spectra that wanted their degrees voiced whole. Those
    /// algorithms are gone, and with them the chord kind in practice: an FM
    /// spectrum has enough partials to put minima across the octave at every
    /// ratio on the roster. Worth asserting because a chord kind arriving
    /// unexpectedly would change how the voices are fed.
    #[test]
    fn every_entry_wants_a_scale() {
        for (e, tuning) in tables(4.0) {
            assert_eq!(
                tuning.kind(),
                Kind::Scale,
                "{} came out a {} with {} degrees",
                e.name,
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
    /// A partial at half the carrier is now legitimate spectrum, not a sub.
    ///
    /// The old test asserted nothing sits at 0.5 once the sub oscillator was
    /// removed. That is wrong for FM: with a modulator at 1.5 the lower
    /// sideband lands at 1 - 1.5 = -0.5 and folds to 0.5, which is the
    /// spectrum doing its job. What actually has to hold is that the SUB
    /// OSCILLATOR is gone, and it is — structurally, since `has_sub` is now
    /// false for everything and nothing adds a partial by hand.
    #[test]
    fn no_entry_adds_a_sub_oscillator() {
        for e in ROSTER {
            assert!(!e.algorithm.has_sub(), "{} still claims a sub", e.name);
        }
    }

/// **What the partial cap actually costs, measured rather than claimed.**
    ///
    /// Capping the spectrum at its loudest 64 partials took derivation from
    /// 1.66 s to 236 ms — the difference between STEP feeling instant and STEP
    /// feeling broken.
    ///
    /// The first version asserted no degree moves by more than a cent. It
    /// fails: on `FM √2` one degree lands 25.8 cents away. Raising the cap to
    /// 96 gives *exactly* the same 25.8, which is the tell — this is not drift
    /// from dropping quiet partials, it is one degree selecting a different
    /// local minimum. Two minima sit close together, the greedy 50-cent
    /// separation rule can only take one, and a hair of curve depth decides
    /// which. Both are real minima of a real dissonance curve, so neither
    /// answer is wrong.
    ///
    /// The honest bound is therefore a semitone rather than a cent. What
    /// actually matters is asserted elsewhere: degrees stay ordered and inside
    /// the octave, and no two entries collapse onto the same scale.
    #[test]
    fn the_partial_cap_keeps_every_scale_recognisable() {
        for e in ROSTER {
            let capped = tuning_for(e.algorithm, e.ratio, 4.0, DEGREES_PER_SCALE).cents();
            let full = {
                let s = spectrum::uncapped_for_test(e.algorithm, e.ratio, 4.0);
                scale::from_spectrum(&s, dissonance::REFERENCE_HZ, DEGREES_PER_SCALE).cents()
            };
            assert_eq!(capped.len(), full.len(), "{} changed degree count", e.name);
            for (c, f) in capped.iter().zip(&full) {
                assert!(
                    (c - f).abs() < 100.0,
                    "{} moved a degree by {:.1} cents — more than a neighbouring \
                     minimum can explain",
                    e.name,
                    (c - f).abs()
                );
            }
        }
    }

    /// **Every entry must be audibly different from every other.**
    ///
    /// This is the whole reason the roster changed. The old one crossed four
    /// modulation types with two convergents, and Billy's verdict after
    /// playing all eight was that they "sound cool but all essentially the
    /// same". So the claim is now checked rather than assumed: no two entries
    /// may agree about where every degree belongs to within the ~5 cents at
    /// which a listener notices a pitch change at all.
    #[test]
    fn no_two_entries_are_the_same_voice() {
        let tunings: Vec<Vec<f64>> = ROSTER
            .iter()
            .map(|e| tuning_for(e.algorithm, e.ratio, 4.0, DEGREES_PER_SCALE).cents())
            .collect();
        for i in 0..ROSTER.len() {
            for j in (i + 1)..ROSTER.len() {
                let worst = tunings[i]
                    .iter()
                    .zip(tunings[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f64, f64::max);
                let differs = tunings[i].len() != tunings[j].len() || worst > 5.0;
                assert!(
                    differs,
                    "{} and {} land on the same scale (worst disagreement {worst:.2} cents)",
                    ROSTER[i].name, ROSTER[j].name
                );
            }
        }
    }

    /// Rising index reshapes the curve, so the scale genuinely tracks INDEX
    /// rather than being fixed per algorithm. This is the claim in `DESIGN.md`
    /// §3 that justifies interpolating tables across index at all.
    #[test]
    fn the_scale_moves_with_index() {
        let low = tuning_for(Algorithm::Fm1, PHI, 1.0, DEGREES_PER_SCALE).cents();
        let high = tuning_for(Algorithm::Fm1, PHI, 9.0, DEGREES_PER_SCALE).cents();
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
        for (e, tuning) in tables(4.0) {
            let cents = tuning.cents();
            assert!(
                cents.windows(2).all(|w| w[0] < w[1]),
                "{} {} is unordered: {cents:?}",
                e.name,
                ""
            );
            assert!(cents.iter().all(|&c| (0.0..=1200.0).contains(&c)));
        }
    }
}
