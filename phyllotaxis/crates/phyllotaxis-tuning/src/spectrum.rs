//! The partials each modulation type produces, in closed form.
//!
//! Every algorithm in the instrument has a spectrum that can be written down
//! rather than measured, which is what makes §3 of the design affordable: the
//! scale tables are computed offline from these lists, and the audio thread
//! never sees any of it.
//!
//! Frequencies are held as **ratios to the voice's fundamental**, not as Hz.
//! A dissonance curve is a function of interval, so nothing here needs to know
//! what note is being played.

/// Partials closer together than this are the same partial. One part in a
/// million is roughly a fiftieth of a cent — far below anything that survives
/// into a scale degree, and it keeps a folded sideband from appearing twice.
const MERGE_TOLERANCE: f64 = 1e-6;

/// Partials quieter than this contribute nothing to a dissonance curve but
/// cost a full pass over the pair list. −100 dB.
const PRUNE_FLOOR: f64 = 1e-5;

/// A single component: where it sits, and how loud it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Partial {
    /// Frequency as a multiple of the voice fundamental.
    pub ratio: f64,
    /// Linear amplitude.
    pub amp: f64,
}

/// One operator modulating another.
#[derive(Clone, Copy, Debug)]
pub struct Modulator {
    /// Frequency ratio to the carrier.
    pub ratio: f64,
    /// Modulation index.
    pub index: f64,
}

/// A list of partials, folded, merged and sorted.
#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    partials: Vec<Partial>,
}

impl Spectrum {
    /// Build from raw components, folding negative frequencies, dropping DC,
    /// merging coincidences and sorting by frequency.
    ///
    /// Negative frequencies fold to their magnitude with a phase inversion. The
    /// inversion is discarded here: a dissonance model is a function of
    /// magnitude and frequency alone, so coincident partials are summed in
    /// magnitude rather than as signed amplitudes. That over-states a
    /// cancelling pair, which is the conservative direction — it can only add a
    /// dissonance contribution that a phase-aware model would remove, never
    /// invent a consonance that is not there.
    pub fn new(raw: impl IntoIterator<Item = Partial>) -> Self {
        let mut partials: Vec<Partial> = raw
            .into_iter()
            .map(|p| Partial { ratio: p.ratio.abs(), amp: p.amp.abs() })
            // DC is removed by the blocker the design puts before the bus, so
            // it must not reach the dissonance curve either.
            .filter(|p| p.ratio > MERGE_TOLERANCE && p.amp > PRUNE_FLOOR)
            .collect();

        partials.sort_by(|a, b| a.ratio.partial_cmp(&b.ratio).expect("no NaN partials"));

        let mut merged: Vec<Partial> = Vec::with_capacity(partials.len());
        for p in partials {
            match merged.last_mut() {
                Some(last) if (p.ratio - last.ratio).abs() <= MERGE_TOLERANCE * last.ratio => {
                    last.amp += p.amp;
                }
                _ => merged.push(p),
            }
        }

        Self { partials: merged }
    }

    pub fn partials(&self) -> &[Partial] {
        &self.partials
    }

    pub fn is_empty(&self) -> bool {
        self.partials.is_empty()
    }

    /// Scale every amplitude so the loudest partial sits at unity.
    ///
    /// The dissonance model weights pairs by their quieter member, so an
    /// un-normalised spectrum would make the curve's *depth* depend on
    /// modulation depth. Only the positions of the minima are wanted.
    pub fn normalised(mut self) -> Self {
        let peak = self.partials.iter().fold(0.0f64, |m, p| m.max(p.amp));
        if peak > 0.0 {
            for p in &mut self.partials {
                p.amp /= peak;
            }
        }
        self
    }

    /// The same spectrum sounding an interval `ratio` away.
    pub fn transposed(&self, ratio: f64) -> Self {
        Self {
            partials: self
                .partials
                .iter()
                .map(|p| Partial { ratio: p.ratio * ratio, amp: p.amp })
                .collect(),
        }
    }

    /// Add a partial — the sub oscillator every complex algorithm carries.
    pub fn with_partial(self, ratio: f64, amp: f64) -> Self {
        Self::new(self.partials.into_iter().chain([Partial { ratio, amp }]))
    }

    /// Merge two spectra into one sounding body.
    pub fn merged(self, other: &Self) -> Self {
        Self::new(self.partials.into_iter().chain(other.partials.iter().copied()))
    }
}

/// **Frequency modulation** by any number of modulators.
///
/// Partials land at `carrier + Σ nᵢ·mᵢ` with amplitude `Π J_{nᵢ}(Iᵢ)` — the
/// product of one Bessel term per modulator. Two modulators is the widest the
/// instrument asks for (FM I), and the combination count stays small because
/// each modulator only reaches `significant_order` sidebands.
pub fn fm(carrier: f64, mods: &[Modulator]) -> Spectrum {
    let mut out = vec![Partial { ratio: carrier, amp: 1.0 }];

    for m in mods {
        let order = crate::bessel::significant_order(m.index);
        let mut next = Vec::with_capacity(out.len() * (2 * order as usize + 1));
        for base in &out {
            for n in -order..=order {
                let amp = base.amp * crate::bessel::j(n, m.index);
                if amp.abs() > PRUNE_FLOOR {
                    next.push(Partial {
                        ratio: base.ratio + n as f64 * m.ratio * carrier,
                        amp,
                    });
                }
            }
        }
        out = next;
    }

    Spectrum::new(out).normalised()
}

/// **Ring modulation**: sum and difference only, and no carrier.
///
/// The absent carrier is the whole character — it is why RM sounds hollow where
/// AM sounds like a tremolo, and why its difference tones are audible enough to
/// chorus against a detuned unison partner.
pub fn ring(carrier: f64, modulator: f64) -> Spectrum {
    Spectrum::new([
        Partial { ratio: carrier - modulator * carrier, amp: 0.5 },
        Partial { ratio: carrier + modulator * carrier, amp: 0.5 },
    ])
    .normalised()
}

/// **Amplitude modulation**: ring modulation with the carrier left in.
///
/// `depth` is the modulation depth; at 0 this is a bare carrier, at 1 the
/// sidebands reach half its amplitude.
pub fn amplitude(carrier: f64, modulator: f64, depth: f64) -> Spectrum {
    Spectrum::new([
        Partial { ratio: carrier, amp: 1.0 },
        Partial { ratio: carrier - modulator * carrier, amp: depth / 2.0 },
        Partial { ratio: carrier + modulator * carrier, amp: depth / 2.0 },
    ])
    .normalised()
}

/// **Full-wave rectification** of a sine at `source`.
///
/// `|sin(2πft)|` has the Fourier series `2/π − (4/π)·Σ cos(2π·2kft)/(4k²−1)`:
/// a large DC term, no fundamental, and only *even* harmonics of the source.
/// The DC is dropped by `Spectrum::new` because the design blocks it before the
/// bus — but it is the reason rectification eats headroom, and the reason the
/// blocker is not optional.
pub fn rectified(source: f64, harmonics: usize) -> Spectrum {
    use core::f64::consts::PI;
    let partials = (1..=harmonics).map(|k| {
        let k = k as f64;
        Partial {
            ratio: 2.0 * k * source,
            amp: 4.0 / (PI * (4.0 * k * k - 1.0)),
        }
    });
    Spectrum::new(partials).normalised()
}

/// **Half-wave rectification** of a sine at `source`.
///
/// `max(sin, 0)` has the series `1/π + ½·sin(2πft) − (2/π)·Σ cos(2π·2kft)/(4k²−1)`:
/// the fundamental *survives*, alongside the same even harmonics full-wave
/// rectification produces.
///
/// That surviving fundamental is the whole reason this function exists.
/// Full-wave rectification emits a harmonic series scaled by the source ratio,
/// so two full-wave modes at different ratios are the *same timbre
/// transposed* — measured at 0.02¢ apart, which is not an algorithm. Half-wave
/// against full-wave is a difference in spectral content rather than a
/// transposition, so the Rect pair stays two things. See `DESIGN.md` §2.
pub fn rectified_half(source: f64, harmonics: usize) -> Spectrum {
    use core::f64::consts::PI;
    let fundamental = core::iter::once(Partial { ratio: source, amp: 0.5 });
    let evens = (1..=harmonics).map(|k| {
        let k = k as f64;
        Partial {
            ratio: 2.0 * k * source,
            amp: 2.0 / (PI * (4.0 * k * k - 1.0)),
        }
    });
    Spectrum::new(fundamental.chain(evens)).normalised()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half-wave keeps the fundamental that full-wave discards. Without this
    /// the two Rect modes are one algorithm.
    #[test]
    fn half_wave_keeps_the_fundamental() {
        let half = rectified_half(1.0, 6);
        let full = rectified(1.0, 6);
        assert!(half.partials().iter().any(|p| (p.ratio - 1.0).abs() < 1e-9));
        assert!(full.partials().iter().all(|p| (p.ratio - 1.0).abs() > 1e-6));
    }

    #[test]
    fn fm_places_sidebands_at_multiples_of_the_modulator() {
        let s = fm(1.0, &[Modulator { ratio: 1.0, index: 2.0 }]);
        // Carrier 1.0, modulator 1.0: partials at every integer, and the
        // reflected lower sidebands fold onto them.
        for p in s.partials() {
            let nearest = p.ratio.round();
            assert!(
                (p.ratio - nearest).abs() < 1e-9,
                "partial at {} is not an integer multiple",
                p.ratio
            );
        }
    }

    #[test]
    fn fm_at_zero_index_is_a_bare_carrier() {
        let s = fm(1.0, &[Modulator { ratio: 1.5, index: 0.0 }]);
        assert_eq!(s.partials().len(), 1);
        assert!((s.partials()[0].ratio - 1.0).abs() < 1e-12);
    }

    #[test]
    fn ring_modulation_has_no_carrier() {
        let s = ring(1.0, 0.5);
        assert_eq!(s.partials().len(), 2);
        assert!(s.partials().iter().all(|p| (p.ratio - 1.0).abs() > 1e-6));
        // Sum and difference: 0.5 and 1.5.
        assert!((s.partials()[0].ratio - 0.5).abs() < 1e-12);
        assert!((s.partials()[1].ratio - 1.5).abs() < 1e-12);
    }

    #[test]
    fn amplitude_modulation_keeps_the_carrier() {
        let s = amplitude(1.0, 0.5, 1.0);
        assert_eq!(s.partials().len(), 3);
        assert!(s.partials().iter().any(|p| (p.ratio - 1.0).abs() < 1e-12));
    }

    /// AM at full depth is RM plus a carrier — the distinction the design turns
    /// on, verified rather than asserted.
    #[test]
    fn am_is_rm_plus_carrier() {
        let rm = ring(1.0, 0.5);
        let am = amplitude(1.0, 0.5, 1.0);
        for r in rm.partials() {
            assert!(
                am.partials().iter().any(|a| (a.ratio - r.ratio).abs() < 1e-12),
                "AM is missing the RM partial at {}",
                r.ratio
            );
        }
        assert_eq!(am.partials().len(), rm.partials().len() + 1);
    }

    /// Rectification emits even harmonics only, and no fundamental.
    #[test]
    fn rectification_is_even_harmonics_of_the_source() {
        let s = rectified(1.0, 6);
        assert!(s.partials().iter().all(|p| {
            let h = p.ratio.round();
            (p.ratio - h).abs() < 1e-9 && (h as i64) % 2 == 0
        }));
        assert!(s.partials().iter().all(|p| (p.ratio - 1.0).abs() > 1e-6));
    }

    /// DC never reaches the curve.
    #[test]
    fn dc_is_dropped() {
        let s = Spectrum::new([
            Partial { ratio: 0.0, amp: 1.0 },
            Partial { ratio: 1.0, amp: 1.0 },
        ]);
        assert_eq!(s.partials().len(), 1);
    }

    /// Folding: a sideband below zero returns as its magnitude.
    #[test]
    fn negative_frequencies_fold() {
        let s = Spectrum::new([
            Partial { ratio: -0.75, amp: 0.4 },
            Partial { ratio: 1.0, amp: 1.0 },
        ]);
        assert!(s.partials().iter().any(|p| (p.ratio - 0.75).abs() < 1e-12));
    }
}
