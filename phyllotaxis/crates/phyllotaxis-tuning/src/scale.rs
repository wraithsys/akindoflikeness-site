//! Turning a dissonance curve into a tuning.
//!
//! Sweep one spectrum against a transposed copy of itself across an octave and
//! the result is a landscape: intervals where the two bodies grate, and
//! intervals where their partials line up. The minima *are* the scale — not
//! chosen, found. For a harmonic spectrum they land on 12-TET's familiar
//! intervals, which is the check that the method is sound; for the φ-tuned
//! spectra this instrument makes, they land somewhere else, which is the point.

use crate::dissonance;
use crate::spectrum::Spectrum;

/// Sweep resolution. A fifth of a cent is well under the ~5 cent threshold at
/// which a pitch difference is noticed, so the grid never limits a degree's
/// accuracy — and parabolic refinement takes it further still.
pub const RESOLUTION_CENTS: f64 = 0.2;

/// How far past the octave to sweep, so a minimum sitting *on* the octave is
/// interior to the search rather than falling off its edge.
const OVERSHOOT_CENTS: f64 = 60.0;

/// Closest two degrees may sit. Half a semitone.
///
/// Without this a scale is not guaranteed to be *playable*. An inharmonic
/// spectrum's dissonance curve often falls away toward the octave rather than
/// dipping cleanly, and ranking minima by depth alone then returns a cluster:
/// the golden FM spectrum yields six degrees inside 57¢, which is one degree
/// and five mistunings of it. The same argument as the variant pair in §2 — a
/// difference smaller than this is heard as a tuning error, not as a step.
const MIN_SEPARATION_CENTS: f64 = 50.0;

pub const CENTS_PER_OCTAVE: f64 = 1200.0;

/// One degree of a computed tuning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Degree {
    /// Position above the root, in cents.
    pub cents: f64,
    /// Curve value at the minimum. Lower is smoother.
    pub dissonance: f64,
    /// How far the curve climbs out of this dip before it reaches a deeper
    /// one. This is what the degree was selected on — see [`from_spectrum`].
    pub prominence: f64,
}

impl Degree {
    /// The degree as a frequency ratio.
    pub fn ratio(self) -> f64 {
        2f64.powf(self.cents / CENTS_PER_OCTAVE)
    }

    /// Distance to the nearest 12-TET degree, in cents — how far this tuning
    /// sits from the grid a listener arrives expecting.
    pub fn detune_from_12tet(self) -> f64 {
        let semitones = self.cents / 100.0;
        (self.cents - semitones.round() * 100.0).abs()
    }
}

/// Fewest degrees that make a scale rather than a chord.
///
/// Five — a Fibonacci number, and the same five as the voice count, which is
/// not a coincidence worth hiding: a tuning with fewer degrees than the
/// instrument has voices can be sounded all at once.
pub const MIN_SCALE_DEGREES: usize = 5;

/// What a spectrum turned out to want.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Enough degrees to move through. Played as a scale.
    Scale,
    /// Too few to step through — the spectrum prefers two or three intervals
    /// and is indifferent to the rest.
    ///
    /// This is not a degenerate scale to be padded out. A sparse spectrum
    /// genuinely has little to collide, so its dissonance curve is shallow
    /// everywhere; the handful of minima it does have are the intervals it
    /// wants **sounded together**. Cadence voices a chord tuning as a chord and
    /// does not step through it. See `DESIGN.md` §3.
    Chord,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Scale => "scale",
            Kind::Chord => "chord",
        }
    }
}

/// A tuning computed from a spectrum — a scale, or a chord.
///
/// The distinction is carried in the type rather than left to a length check at
/// each call site, because the two are voiced differently and a caller that
/// forgot which it had would silently arpeggiate a chord.
#[derive(Clone, Debug)]
pub struct Tuning {
    degrees: Vec<Degree>,
    kind: Kind,
}

impl Tuning {
    /// Whether this spectrum wants a scale or a chord.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn is_chord(&self) -> bool {
        self.kind == Kind::Chord
    }

    /// Degrees in pitch order, root first.
    pub fn degrees(&self) -> &[Degree] {
        &self.degrees
    }

    pub fn len(&self) -> usize {
        self.degrees.len()
    }

    pub fn is_empty(&self) -> bool {
        self.degrees.is_empty()
    }

    /// Degrees as cents, for baking into a table.
    pub fn cents(&self) -> Vec<f64> {
        self.degrees.iter().map(|d| d.cents).collect()
    }
}

/// The dissonance curve of a spectrum against itself, sampled across an octave.
///
/// Returns `(cents, dissonance)` pairs. This is what the centrepiece can draw:
/// the landscape the scale was read off, with the degrees sitting in its
/// valleys.
pub fn curve(spectrum: &Spectrum, fundamental_hz: f64) -> Vec<(f64, f64)> {
    let steps = ((CENTS_PER_OCTAVE + OVERSHOOT_CENTS) / RESOLUTION_CENTS) as usize;
    (0..=steps)
        .map(|i| {
            let cents = i as f64 * RESOLUTION_CENTS;
            let ratio = 2f64.powf(cents / CENTS_PER_OCTAVE);
            let transposed = spectrum.transposed(ratio);
            (cents, dissonance::between(spectrum, &transposed, fundamental_hz))
        })
        .collect()
}

/// How far the curve climbs out of the dip at `i` before it reaches a deeper
/// one — the topographic prominence of a minimum, read upside down.
///
/// Walk outward in both directions, keeping the highest point seen, and stop on
/// the first sample lower than the dip itself. The smaller of the two climbs is
/// the prominence: a dip you can leave cheaply in either direction is a ripple,
/// and one walled in on both sides is an interval. The measure is local, which
/// is exactly why it survives a curve with a strong overall trend.
///
/// Linear per minimum, and there are tens of them against 6300 samples — beside
/// [`curve`], which is O(n²) in partials at every one of those samples, this is
/// free.
fn prominence(values: &[f64], i: usize) -> f64 {
    let floor = values[i];

    let mut left = floor;
    for j in (0..i).rev() {
        if values[j] < floor {
            break;
        }
        left = left.max(values[j]);
    }

    let mut right = floor;
    for j in i + 1..values.len() {
        if values[j] < floor {
            break;
        }
        right = right.max(values[j]);
    }

    left.min(right) - floor
}

/// Read a tuning off a spectrum: the `max_degrees` most pronounced dips in the
/// octave, root included.
///
/// **Minima are ranked by prominence, not by curve value, and the difference is
/// the whole scale.** Ranking by value was the original choice, on the argument
/// that a shallow dip at a genuinely low dissonance is a usable interval while
/// a deep dip out of a rough region is not. That argument assumes the curve is
/// a landscape. It is not — it is a hill. Averaged per hundred cents, FM φ at
/// index 4 falls from 9.82 near the unison to 6.42 at the octave, a 34 % slope,
/// while the ripples riding on it are worth about 2 %. Physically this is
/// expected: transposing a copy upward pulls its partials out of the original's
/// critical bands, so roughness decays with interval size whether or not
/// anything aligns.
///
/// On a slope that steep, "lowest value" is a proxy for "furthest right".
/// Ranking by it filled the scale downward from the octave at exactly
/// [`MIN_SEPARATION_CENTS`] — FM φ came out `0 778 833 943 997 1069 1146 1200`,
/// seven degrees of eight above 778¢ with a 778¢ hole beneath them and six
/// consecutive gaps near the separation floor. Those are not steps, they are
/// mistuned unisons, and against the octave-displaced registers in the engine
/// they beat. It is the bulk of what "out of tune" meant here. Across the
/// roster at 32 modulation indices it put the first degree above 400¢ in 240
/// tunings of 256; prominence does it in 40.
///
/// Prominence asks the question the slope cannot answer: how far must the curve
/// climb out of this dip before it finds a deeper one. That is a local measure,
/// so a real partial coincidence in the crowded bottom of the octave competes
/// on equal terms with the smooth ground near the top. The same spectrum now
/// gives `0 438 560 724 833 943 1106 1200`, and the harmonic control case
/// returns just intonation — 386, 498, 583, 702, 884, 969 — which is the check
/// that the method reads a spectrum rather than the shape of its envelope.
///
/// Ranked candidates are then taken greedily subject to
/// [`MIN_SEPARATION_CENTS`], so the result is a scale someone can play rather
/// than a cluster around the curve's lowest region.
///
/// A spectrum may yield fewer than `max_degrees`. That is information, not a
/// failure: a sparse spectrum has few partials to collide, so its curve is
/// shallow everywhere and it genuinely does not prefer many intervals. Below
/// [`MIN_SCALE_DEGREES`] the result is returned as a [`Kind::Chord`] and voiced
/// as one.
pub fn from_spectrum(spectrum: &Spectrum, fundamental_hz: f64, max_degrees: usize) -> Tuning {
    if spectrum.is_empty() || max_degrees == 0 {
        return Tuning { degrees: Vec::new(), kind: Kind::Chord };
    }

    let samples = curve(spectrum, fundamental_hz);
    let values: Vec<f64> = samples.iter().map(|&(_, d)| d).collect();
    let mut minima = Vec::new();

    for i in 1..values.len() - 1 {
        let (before, at, after) = (values[i - 1], values[i], values[i + 1]);
        let is_minimum = at < before && at <= after;
        if !is_minimum || samples[i].0 <= 0.0 || samples[i].0 > CENTS_PER_OCTAVE {
            continue;
        }

        // Parabolic refinement through the three samples: the true vertex is
        // rarely on a grid point, and a degree is worth placing properly.
        let denom = before - 2.0 * at + after;
        let offset = if denom.abs() > f64::EPSILON {
            0.5 * (before - after) / denom
        } else {
            0.0
        };
        let cents = (samples[i].0 + offset * RESOLUTION_CENTS).clamp(0.0, CENTS_PER_OCTAVE);
        minima.push(Degree { cents, dissonance: at, prominence: prominence(&values, i) });
    }

    minima.sort_by(|a, b| b.prominence.partial_cmp(&a.prominence).expect("no NaN prominence"));

    // The root is a degree by definition — perfect coincidence, and the
    // smoothest interval there is. It also seeds the separation check, which is
    // what keeps a degree from landing a few cents above the tonic.
    let mut degrees = vec![Degree { cents: 0.0, dissonance: 0.0, prominence: f64::INFINITY }];
    for candidate in minima {
        if degrees.len() >= max_degrees {
            break;
        }
        let crowded = degrees
            .iter()
            .any(|d| (d.cents - candidate.cents).abs() < MIN_SEPARATION_CENTS);
        if !crowded {
            degrees.push(candidate);
        }
    }
    degrees.sort_by(|a, b| a.cents.partial_cmp(&b.cents).expect("no NaN cents"));

    let kind = if degrees.len() >= MIN_SCALE_DEGREES { Kind::Scale } else { Kind::Chord };
    Tuning { degrees, kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrum::{Partial, Spectrum};

    fn harmonic(n: usize) -> Spectrum {
        Spectrum::new((1..=n).map(|k| Partial { ratio: k as f64, amp: 1.0 / k as f64 }))
    }

    /// The load-bearing test for the whole method: a harmonic spectrum must
    /// produce something recognisably like the scale Western music actually
    /// uses. If this fails, nothing computed for the inharmonic algorithms can
    /// be trusted either.
    #[test]
    fn a_harmonic_spectrum_yields_familiar_intervals() {
        let tuning = from_spectrum(&harmonic(8), dissonance::REFERENCE_HZ, 8);
        let cents = tuning.cents();

        // The just fifth (702¢), fourth (498¢) and octave (1200¢) should all be
        // there, within a few cents.
        for &expected in &[498.0, 702.0, 1200.0] {
            let found = cents
                .iter()
                .any(|&c| (c - expected).abs() < 8.0);
            assert!(found, "no degree near {expected}¢ in {cents:?}");
        }
    }

    /// Every degree sits close to the 12-TET grid — the historical claim the
    /// model is reproducing.
    ///
    /// Six harmonics, deliberately. Take eight and the 7th produces a minimum
    /// at 7/4 = 968.8¢, which is 31¢ from the grid — not an error but the
    /// septimal seventh, an interval 12-TET genuinely does not contain. The
    /// grid approximates a 5-limit spectrum, and only that.
    #[test]
    fn harmonic_degrees_sit_near_12tet() {
        let tuning = from_spectrum(&harmonic(6), dissonance::REFERENCE_HZ, 6);
        for d in tuning.degrees() {
            assert!(
                d.detune_from_12tet() < 20.0,
                "{}¢ is {}¢ from the grid",
                d.cents,
                d.detune_from_12tet()
            );
        }
    }

    /// A φ-tuned inharmonic spectrum must *not* land on the grid — otherwise
    /// the feature does nothing and the scale roster is decoration.
    #[test]
    fn a_golden_spectrum_leaves_the_grid() {
        const PHI: f64 = 1.618_033_988_749_895;
        let golden = Spectrum::new(
            (0..6).map(|k| Partial { ratio: PHI.powi(k), amp: 1.0 / (k + 1) as f64 }),
        );
        let tuning = from_spectrum(&golden, dissonance::REFERENCE_HZ, 6);
        let worst = tuning
            .degrees()
            .iter()
            .map(|d| d.detune_from_12tet())
            .fold(0.0f64, f64::max);
        assert!(worst > 20.0, "golden spectrum stayed on the grid: {:?}", tuning.cents());
    }

    #[test]
    fn the_root_is_always_present_and_first() {
        let tuning = from_spectrum(&harmonic(6), dissonance::REFERENCE_HZ, 5);
        assert!(!tuning.is_empty());
        assert_eq!(tuning.degrees()[0].cents, 0.0);
    }

    /// No two degrees closer than half a semitone — the property that makes the
    /// result a scale rather than a cluster.
    #[test]
    fn degrees_are_separated() {
        const PHI: f64 = 1.618_033_988_749_895;
        let golden = Spectrum::new(
            (0..7).map(|k| Partial { ratio: PHI.powi(k), amp: 1.0 / (k + 1) as f64 }),
        );
        for spectrum in [harmonic(8), golden] {
            let cents = from_spectrum(&spectrum, dissonance::REFERENCE_HZ, 8).cents();
            assert!(
                cents.windows(2).all(|w| w[1] - w[0] >= MIN_SEPARATION_CENTS),
                "degrees are crowded: {cents:?}"
            );
        }
    }

    #[test]
    fn degrees_are_ordered_and_within_the_octave() {
        let tuning = from_spectrum(&harmonic(8), dissonance::REFERENCE_HZ, 8);
        let cents = tuning.cents();
        assert!(cents.windows(2).all(|w| w[0] < w[1]), "{cents:?}");
        assert!(cents.iter().all(|&c| (0.0..=CENTS_PER_OCTAVE).contains(&c)));
    }

    #[test]
    fn requesting_no_degrees_yields_nothing() {
        assert!(from_spectrum(&harmonic(4), dissonance::REFERENCE_HZ, 0).is_empty());
    }

    /// A rich spectrum wants a scale; a spectrum with almost nothing in it
    /// wants a chord. The threshold is a real distinction, not a formality.
    #[test]
    fn sparse_spectra_are_chords_and_rich_ones_are_scales() {
        let rich = from_spectrum(&harmonic(8), dissonance::REFERENCE_HZ, 8);
        assert_eq!(rich.kind(), Kind::Scale);
        assert!(rich.len() >= MIN_SCALE_DEGREES);

        // Two partials a fifth apart: almost nothing to collide.
        let sparse = Spectrum::new([
            Partial { ratio: 1.0, amp: 1.0 },
            Partial { ratio: 1.5, amp: 1.0 },
        ]);
        let tuning = from_spectrum(&sparse, dissonance::REFERENCE_HZ, 8);
        assert_eq!(tuning.kind(), Kind::Chord);
        assert!(tuning.len() < MIN_SCALE_DEGREES);
    }
}
