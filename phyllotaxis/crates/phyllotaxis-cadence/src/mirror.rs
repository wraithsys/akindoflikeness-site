//! Negative harmony — the pitch mirror.
//!
//! A reflection of log-frequency about an axis fixed by the key: `c ↦ A − c`
//! in cents, or `f ↦ f_axis²/f` in Hz. The primitive already exists in this
//! family — `melody.rs` reflects the golden walk off its bounds with exactly
//! `hz = hi*hi/hz`. Only the axis is new.
//!
//! Two properties do the work. It **preserves interval sizes and reverses
//! their direction**, so a mirrored chord has the same internal interval
//! multiset and only its contour inverts — which is why it sounds like the
//! same material seen from the other side rather than like a transposition.
//! And the choice of axis is **only a transposition** of the result, so the
//! argument about conventions is entirely an argument about which pitch the
//! tonic maps to; the tonic–dominant axis is the one where the tonic triad's
//! image stays rooted on the tonic.
//!
//! ## It was never scale-preserving, and that is the point
//!
//! In 12-TET, Ionian mirrors to Aeolian and Phrygian to Lydian. Negative
//! harmony leaves the scale on every application, so demanding it land on a
//! computed tuning's degrees is a requirement nobody imposes on the ordinary
//! case.
//!
//! What differs here is what "off" *means*. In 12-TET an off-scale note is
//! still on the chromatic grid and reads as chromaticism. In a tuning whose
//! degrees are the minima of a measured dissonance curve, off-lattice means
//! off the consonance minima — audibly rough rather than merely outside the
//! key. So the mirror gets exactly one addition: a **capture radius** that
//! snaps a reflection onto a degree when the miss is small enough to be heard
//! as mistuning, and leaves it alone when it is large enough to be its own
//! pitch. The threshold already exists — `DESIGN.md` §3's 50 ¢ separation
//! floor, below which "a difference is heard as a tuning error, not a step".

pub const OCTAVE_CENTS: f64 = 1200.0;
/// §3's existing constant, reused rather than invented.
pub const MIN_SEPARATION_CENTS: f64 = 50.0;
/// The abstract axis interval: a tempered fifth.
pub const FIFTH_CENTS: f64 = 700.0;

/// A baked mirror table for one tuning.
#[derive(Clone, Debug)]
pub struct Mirror {
    degrees: Vec<f64>,
    axis: f64,
    radius: f64,
    images: Vec<f64>,
    on_lattice: Vec<bool>,
}

impl Mirror {
    /// Build from a tuning's degrees in cents.
    ///
    /// Degrees equal to the octave are dropped: they are congruent to the root
    /// and several roster tunings carry an explicit 1200 that must not be
    /// counted as a second pitch class.
    pub fn new(cents: &[f64]) -> Self {
        let mut degrees: Vec<f64> = cents
            .iter()
            .copied()
            .filter(|&c| c < OCTAVE_CENTS - 1e-9)
            .collect();
        degrees.sort_by(|a, b| a.partial_cmp(b).expect("no NaN degrees"));
        degrees.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        if degrees.is_empty() {
            degrees.push(0.0);
        }

        let radius = (MIN_SEPARATION_CENTS / 2.0).min(Self::min_gap(&degrees) / 2.0);
        let axis = Self::choose_axis(&degrees);

        let mut images = Vec::with_capacity(degrees.len());
        let mut on_lattice = Vec::with_capacity(degrees.len());
        for &d in &degrees {
            let y = (axis - d).rem_euclid(OCTAVE_CENTS);
            match Self::nearest(&degrees, y, radius) {
                Some(snapped) => {
                    images.push(snapped);
                    on_lattice.push(true);
                }
                None => {
                    images.push(y);
                    on_lattice.push(false);
                }
            }
        }
        Self { degrees, axis, radius, images, on_lattice }
    }

    /// Smallest gap between adjacent degrees, **including the octave seam**.
    ///
    /// The seam is not a formality. `fm fb II` tops out at 1160 ¢, so its wrap
    /// gap is 40 ¢ — under the 50 ¢ floor that the in-octave extraction
    /// guarantees. Any lookup that ignores it is wrong on exactly that table.
    fn min_gap(degrees: &[f64]) -> f64 {
        if degrees.len() < 2 {
            return OCTAVE_CENTS;
        }
        let mut g = OCTAVE_CENTS - degrees[degrees.len() - 1] + degrees[0];
        for w in degrees.windows(2) {
            g = g.min(w[1] - w[0]);
        }
        g
    }

    /// The nearest degree to `y`, wrapping, if within `radius`.
    fn nearest(degrees: &[f64], y: f64, radius: f64) -> Option<f64> {
        let mut best: Option<(f64, f64)> = None;
        for &d in degrees {
            for image in [d - OCTAVE_CENTS, d, d + OCTAVE_CENTS] {
                let dist = (image - y).abs();
                if dist <= radius && best.map_or(true, |(_, b)| dist < b) {
                    best = Some((d, dist));
                }
            }
        }
        best.map(|(d, _)| d)
    }

    /// The axis interval: the tuning's own dominant if it has one, else a
    /// tempered fifth.
    ///
    /// The fallback is not always safe, which is why the native branch exists:
    /// `fm fb II` under a 700 ¢ axis maps 0 → 720 → 1160 → 720 and never
    /// returns, so the reflection is not an involution and the mirror is not a
    /// mirror. Its own near-fifth at 720 ¢ restores it.
    fn choose_axis(degrees: &[f64]) -> f64 {
        degrees
            .iter()
            .copied()
            .filter(|&d| d > 0.0)
            .min_by(|a, b| {
                (a - FIFTH_CENTS).abs().partial_cmp(&(b - FIFTH_CENTS).abs()).unwrap()
            })
            .filter(|d| (d - FIFTH_CENTS).abs() <= MIN_SEPARATION_CENTS)
            .unwrap_or(FIFTH_CENTS)
    }

    pub fn axis(&self) -> f64 {
        self.axis
    }
    pub fn radius(&self) -> f64 {
        self.radius
    }
    pub fn degrees(&self) -> &[f64] {
        &self.degrees
    }

    /// The reflection of a degree, in cents.
    pub fn reflect(&self, cents: f64) -> f64 {
        let c = cents.rem_euclid(OCTAVE_CENTS);
        match self.degrees.iter().position(|&d| (d - c).abs() < 1e-6) {
            Some(i) => self.images[i],
            // Not a degree of this tuning: reflect it and snap if it lands near one.
            None => {
                let y = (self.axis - c).rem_euclid(OCTAVE_CENTS);
                Self::nearest(&self.degrees, y, self.radius).unwrap_or(y)
            }
        }
    }

    /// Whether every image landed on a degree — i.e. the mirror stayed inside
    /// the tuning. Usually false, and that is correct.
    pub fn fully_on_lattice(&self) -> bool {
        self.on_lattice.iter().all(|&b| b)
    }

    /// Reflect a whole chord, **leaving the pedal alone**.
    ///
    /// The lowest voice is the key centre, not a chord tone: mirroring it moves
    /// the tonic, which is a modulation rather than a reflection. So it is
    /// exempt — and the exemption lives here, at the voice level, rather than
    /// inside [`reflect`]. Putting it in the table instead would send the tonic
    /// to itself while the dominant still mapped to the tonic, and the
    /// reflection would stop being an involution.
    pub fn reflect_chord(&self, cents: &[f64]) -> Vec<f64> {
        let mut out: Vec<f64> = cents.to_vec();
        let pedal = out
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).expect("no NaN pitches"))
            .map(|(i, _)| i);
        for (i, c) in out.iter_mut().enumerate() {
            if Some(i) != pedal {
                *c = self.reflect(*c);
            }
        }
        out
    }

    /// Is the reflection an involution on this tuning? Guards a bad axis, a
    /// missed seam, or a wrong radius in one assertion.
    pub fn is_involution(&self) -> bool {
        self.degrees
            .iter()
            .all(|&d| (self.reflect(self.reflect(d)) - d).abs() < 1e-6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phyllotaxis_tuning::{tuning_for, DEGREES_PER_SCALE, ROSTER};

    /// The defining property. If this fails the operation is not a mirror.
    #[test]
    fn reflection_is_an_involution_on_every_roster_tuning() {
        for e in ROSTER.iter() {
            let t = tuning_for(e.algorithm, e.ratio, 4.0, DEGREES_PER_SCALE);
            let m = Mirror::new(&t.cents());
            assert!(
                m.is_involution(),
                "{} {} is not an involution (axis {})",
                e.name, "", m.axis()
            );
        }
    }

    /// The case that forced the tuning-native axis branch: `fm fb II` tops out
    /// at 1160 ¢, and under a plain 700 ¢ axis its reflection never returns.
    #[test]
    fn a_tuning_with_a_tight_octave_seam_needs_its_own_axis() {
        let degrees = vec![0.0, 561.0, 720.0, 834.0, 894.0, 983.0, 1101.0, 1160.0];
        let m = Mirror::new(&degrees);
        assert!(m.is_involution(), "axis {} radius {}", m.axis(), m.radius());
        // The seam gap is 1200 - 1160 + 0 = 40 cents, under the 50 floor,
        // so the radius must shrink below the usual 25.
        assert!(m.radius() < 25.0, "radius {} ignored the octave seam", m.radius());
    }

    /// Interval sizes survive; direction inverts. This is what makes it sound
    /// like the same harmony from the other side.
    #[test]
    fn intervals_are_preserved_and_reversed() {
        let m = Mirror::new(&[0.0, 100.0, 300.0, 500.0, 700.0, 800.0, 1000.0]); // Phrygian
        let (a, b) = (300.0, 700.0);
        let (ra, rb) = (m.reflect(a), m.reflect(b));
        assert!(((rb - ra) + (b - a)).abs() < 1e-6, "{ra} {rb} vs {a} {b}");
    }

    /// 12-TET sanity: Phrygian mirrors to Lydian, keeping tonic and fifth.
    #[test]
    fn phrygian_mirrors_to_lydian() {
        let phrygian = [0.0, 100.0, 300.0, 500.0, 700.0, 800.0, 1000.0];
        let m = Mirror::new(&phrygian);
        assert_eq!(m.axis(), 700.0);
        let mut got: Vec<i64> = phrygian.iter().map(|&d| m.reflect(d).round() as i64).collect();
        got.sort_unstable();
        assert_eq!(got, vec![0, 200, 400, 600, 700, 900, 1100], "expected Lydian");
    }

    /// The mirror leaves the scale, and that is not a failure.
    #[test]
    fn leaving_the_lattice_is_normal() {
        let off: Vec<_> = ROSTER
            .iter()
            .map(|e| {
                let t = tuning_for(e.algorithm, e.ratio, 4.0, DEGREES_PER_SCALE);
                Mirror::new(&t.cents()).fully_on_lattice()
            })
            .collect();
        assert!(off.iter().any(|&on| !on), "no tuning left its own lattice — suspicious");
    }

    /// A lone tonic reflects to the dominant, which is correct: negative
    /// harmony sends the tonic there. The first version of this test asserted
    /// `reflect(0) == 0`, confusing the table's job with the pedal exemption.
    #[test]
    fn a_single_degree_tuning_reflects_to_the_axis() {
        let m = Mirror::new(&[0.0]);
        assert!(m.is_involution());
        assert_eq!(m.axis(), FIFTH_CENTS);
        assert_eq!(m.reflect(0.0), FIFTH_CENTS);
    }

    /// The pedal is exempt at the voice level, so the key centre survives.
    #[test]
    fn the_pedal_voice_is_not_mirrored() {
        let m = Mirror::new(&[0.0, 100.0, 300.0, 500.0, 700.0, 800.0, 1000.0]);
        let chord = [0.0, 300.0, 700.0];
        let out = m.reflect_chord(&chord);
        assert_eq!(out[0], 0.0, "the pedal moved — that is a modulation, not a mirror");
        assert_ne!(out[1], chord[1]);
        assert_ne!(out[2], chord[2]);
    }

    /// And exempting it inside the table instead would break the involution —
    /// the reason the exemption lives at the voice level.
    #[test]
    fn exempting_the_tonic_in_the_table_would_collide() {
        let m = Mirror::new(&[0.0, 100.0, 300.0, 500.0, 700.0, 800.0, 1000.0]);
        // Both the tonic and the dominant would have to map to the tonic.
        assert_eq!(m.reflect(700.0), 0.0);
        assert_eq!(m.reflect(0.0), 700.0);
    }
}
