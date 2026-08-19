//! Three operators, eight algorithms — the voice the tuning tables describe.
//!
//! The binding constraint on this crate is not sound quality, it is
//! **agreement**: every partial `phyllotaxis-tuning` predicts has to actually
//! come out of here, because the scales were computed from those predictions.
//! If the DSP and the model disagree, the spectrum-matched tuning is matched to
//! a spectrum nothing produces, and the whole headline claim is decoration.
//!
//! So the tests in this crate render the voice, take its spectrum, and check it
//! against `spectrum_for()` peak by peak. That test is the contract between the
//! two crates and is the reason this crate exists in its own right.
//!
//! Allocation-free on the audio path, deterministic, and no `f64` anywhere in
//! `tick()`.

pub mod op;
pub mod spectral;

use core::f32::consts::TAU;
use op::{flush, DcBlocker, Operator};
use phyllotaxis_tuning::{spectrum_for, Algorithm, ROSTER, PHI};

/// Where the sub sits, and how loud — matching `phyllotaxis-tuning`.
const SUB_RATIO: f32 = 0.5;
const SUB_AMP: f32 = 0.5;

/// Everything the voice needs to know. Plain data, `Copy`, no allocation.
#[derive(Clone, Copy, Debug)]
pub struct VoiceParams {
    pub algorithm: Algorithm,
    /// The modulator ratio — the entry itself.
    pub ratio: f32,
    /// Modulation index. Drives sideband count in FM, depth in AM,
    /// self-feedback in FM II.
    pub index: f32,
    /// The free modulator ratio. FM I's second modulator is placed at a
    /// Fibonacci relationship to this one; every other algorithm uses the
    /// variant's own ratio directly.
    pub free_ratio: f32,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::Fm1,
            ratio: PHI as f32,
            index: 4.0,
            free_ratio: 1.0,
        }
    }
}

/// One voice. Three operators and the state the algorithms need between them.
#[derive(Clone, Debug)]
pub struct Voice {
    sample_rate: f32,
    params: VoiceParams,
    root_hz: f32,

    car: Operator,
    m1: Operator,
    m2: Operator,
    sub: Operator,

    /// FM II's self-modulating operator remembers its own last output.
    fb_state: f32,
    dc: DcBlocker,
    gain: f32,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        let mut v = Self {
            sample_rate,
            params: VoiceParams::default(),
            root_hz: 110.0,
            car: Operator::new(),
            m1: Operator::new(),
            m2: Operator::new(),
            sub: Operator::new(),
            fb_state: 0.0,
            dc: DcBlocker::default(),
            gain: 1.0,
        };
        v.retune();
        v
    }

    pub fn params(&self) -> VoiceParams {
        self.params
    }

    pub fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
        self.retune();
    }

    pub fn set_root_hz(&mut self, hz: f32) {
        self.root_hz = hz;
        self.retune();
    }

    /// Move only the modulation depth, leaving everything else alone.
    ///
    /// The pool breathes this at control rate; the full `set_params` path
    /// would recompute the spectrum-derived gain from a predicted partial
    /// list every block. The gain stays anchored at the panel index on
    /// purpose: the drift is a few percent of spectrum, not a change of
    /// voice, and level-matching against a moving target would turn
    /// breathing into pumping.
    pub fn set_index(&mut self, index: f32) {
        self.params.index = index;
    }

    /// Spread the unison pair's starting phases so a two-voice stack does not
    /// begin as one voice at double amplitude.
    pub fn set_phase_offset(&mut self, phase01: f32) {
        self.car.set_phase(phase01);
        self.m1.set_phase(phase01 * 0.5);
        self.m2.set_phase(phase01 * 0.25);
        self.sub.set_phase(phase01);
    }

    pub fn reset(&mut self) {
        self.car.reset();
        self.m1.reset();
        self.m2.reset();
        self.sub.reset();
        self.fb_state = 0.0;
        self.dc.reset();
    }

    fn variant_ratio(&self) -> f32 {
        self.params.ratio
    }

    fn retune(&mut self) {
        let sr = self.sample_rate;
        let f0 = self.root_hz;
        let vr = self.variant_ratio();
        let free = self.params.free_ratio;

        self.car.set_hz(f0, sr);
        self.sub.set_hz(f0 * SUB_RATIO, sr);

        match self.params.algorithm {
            // Two modulators: the free one, and one at a Fibonacci
            // relationship to it. The relationship *is* the variant, so the
            // second modulator walks from harmonic to golden with the first.
            Algorithm::Fm1 => {
                self.m1.set_hz(f0 * free, sr);
                self.m2.set_hz(f0 * free * vr, sr);
            }
            // The feedback operator sits at the variant ratio; m2 is unused.
            Algorithm::Fm2 => {
                self.m1.set_hz(f0 * vr, sr);
                self.m2.set_hz(f0 * vr, sr);
            }
            _ => {
                self.m1.set_hz(f0 * vr, sr);
                self.m2.set_hz(f0 * vr, sr);
            }
        }

        self.gain = Self::gain_for(self.params);
    }

    /// Level matching across the roster.
    ///
    /// Six algorithms carry a sub and two do not; FM II at high index spreads
    /// its energy over hundreds of sidebands while RM sounds two partials.
    /// Left alone the algorithm selector doubles as a volume control, which
    /// `DESIGN.md` §2 names as a defect.
    ///
    /// The correction is **derived, not tabled**: the predicted spectrum's RMS
    /// is `sqrt(Σ aᵢ² / 2)`, and the reciprocal of that is the gain. It comes
    /// from the same partial list the tuning tables come from, so it cannot
    /// drift away from them. This normalises a *fault*; nothing here touches
    /// character.
    fn gain_for(params: VoiceParams) -> f32 {
        let spectrum = spectrum_for(params.algorithm, params.ratio as f64, params.index as f64);
        let energy: f64 = spectrum.partials().iter().map(|p| p.amp * p.amp).sum();
        if energy <= 0.0 {
            return 1.0;
        }
        (1.0 / (energy / 2.0).sqrt()) as f32 * 0.25
    }

    /// One sample.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let p = self.params;
        // Phase modulation takes its depth in cycles, not radians.
        let depth = p.index / TAU;

        let raw = match p.algorithm {
            Algorithm::Fm1 => {
                let a = self.m1.tick(0.0);
                let b = self.m2.tick(0.0);
                // The second modulator's index is scaled by its own ratio, so
                // raising it does not simply make the higher operator louder.
                let pm = a * depth + b * depth / self.variant_ratio();
                self.car.tick(pm)
            }

            Algorithm::Fm2 => {
                // A self-modulating operator: it reads its own last output.
                // beta saturates at 1.0 — past that it does not get brighter,
                // it goes chaotic, and this instrument is not that instrument.
                let beta = (p.index / 4.0).min(1.0);
                let fb = self.m1.tick(self.fb_state * beta / TAU);
                self.fb_state = flush(fb);
                self.car.tick(fb * depth)
            }

        };

        // DC is not optional for a rectified source, and costs nothing on the
        // others.
        flush(self.dc.process(raw) * self.gain)
    }

    /// Fill a block. The audio-thread entry point — no allocation, no branch
    /// per sample beyond the algorithm match.
    pub fn process(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = self.tick();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const F0: f32 = 110.0;
    const N: usize = 32_768;

    fn render(params: VoiceParams) -> Vec<f32> {
        let mut v = Voice::new(SR);
        v.set_root_hz(F0);
        v.set_params(params);
        let mut buf = vec![0.0; N];
        // Settle the DC blocker and the feedback state before measuring.
        let mut warm = [0.0f32; 4096];
        v.process(&mut warm);
        v.process(&mut buf);
        buf
    }

    /// THE CONTRACT. Every strong partial the tuning model predicts has to be
    /// present in what the DSP actually emits. The scales are computed from
    /// those predictions, so a disagreement here means the instrument is tuned
    /// to a spectrum nothing produces.
    #[test]
    fn the_dsp_emits_the_spectrum_the_tables_were_computed_from() {
        let bin_hz = SR as f64 / N as f64;
        let mut worst = (100.0f64, String::new());

        for algorithm in Algorithm::ALL {
            for &ratio in &[1.5f32, 2.0, 3.0, PHI as f32] {
                let params = VoiceParams { algorithm, ratio, index: 4.0, free_ratio: 1.0 };
                let mag = spectral::magnitude(&render(params));
                let predicted = spectrum_for(algorithm, ratio as f64, 4.0);

                let strong: Vec<_> = predicted
                    .partials()
                    .iter()
                    .filter(|p| p.amp > 0.2)
                    .filter(|p| {
                        let hz = p.ratio * F0 as f64;
                        hz > 30.0 && hz < 18_000.0
                    })
                    .collect();
                assert!(!strong.is_empty(), "{} {} predicted nothing strong",
                        algorithm.name(), ratio);

                let found = strong
                    .iter()
                    .filter(|p| {
                        let bin = (p.ratio * F0 as f64 / bin_hz).round() as usize;
                        spectral::peak_near(&mag, bin, 4, 0.02)
                    })
                    .count();
                let pct = 100.0 * found as f64 / strong.len() as f64;
                if pct < worst.0 {
                    worst = (pct, format!("{} {}", algorithm.name(), ratio));
                }
                assert!(
                    pct >= 80.0,
                    "{} {}: only {found}/{} predicted partials present ({pct:.0}%)",
                    algorithm.name(), ratio, strong.len()
                );
            }
        }
        eprintln!("weakest agreement: {} at {:.0}%", worst.1, worst.0);
    }

    /// Nothing may leave the voice that is not finite, and nothing may leave it
    /// louder than the bus can take.
    #[test]
    fn output_is_finite_and_bounded_for_every_algorithm() {
        for e in ROSTER {
            for &index in &[0.0f32, 1.0, 4.0, 9.0, 12.0] {
                let buf = render(VoiceParams {
                    algorithm: e.algorithm,
                    ratio: e.ratio as f32,
                    index,
                    free_ratio: 1.0,
                });
                let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                assert!(
                    buf.iter().all(|s| s.is_finite()),
                    "{} at index {index} produced a non-finite sample",
                    e.name
                );
                assert!(peak < 4.0, "{} at index {index} peaked at {peak}", e.name);
            }
        }
    }

    /// Gain matching: no algorithm may be more than a few dB from its
    /// neighbours, or the selector is a volume control.
    #[test]
    fn the_roster_is_level_matched() {
        let mut rms = Vec::new();
        for e in ROSTER {
            let buf = render(VoiceParams {
                algorithm: e.algorithm,
                ratio: e.ratio as f32,
                index: 4.0,
                free_ratio: 1.0,
            });
            let r = (buf.iter().map(|s| (s * s) as f64).sum::<f64>() / buf.len() as f64).sqrt();
            rms.push((e.name.to_string(), r));
        }
        let lo = rms.iter().map(|(_, r)| *r).fold(f64::INFINITY, f64::min);
        let hi = rms.iter().map(|(_, r)| *r).fold(0.0f64, f64::max);
        let spread_db = 20.0 * (hi / lo).log10();
        for (n, r) in &rms {
            eprintln!("{n:<10} {:.1} dBFS", 20.0 * r.log10());
        }
        assert!(spread_db < 12.0, "roster spans {spread_db:.1} dB — the selector is a volume control");
    }

    /// Same settings, same samples, forever.
    #[test]
    fn rendering_is_deterministic() {
        let p = VoiceParams { algorithm: Algorithm::Fm2, ratio: PHI as f32, index: 7.0, free_ratio: 1.0 };
        assert_eq!(render(p), render(p));
    }
}
