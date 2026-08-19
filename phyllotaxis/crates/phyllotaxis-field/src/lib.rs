//! The field: always-on amplitude movement, and no envelope anywhere.
//!
//! A note here is not a shape. The field is always moving; a note is the field
//! **moving more for a while**. That is why there is no attack level to reach
//! and no sustain to fall back to — and why an ADSR would be wrong rather than
//! merely unfashionable. See `DESIGN.md` §5.
//!
//! Three things carried from `fibonacci-synth`'s `breath.rs` unchanged in
//! reasoning, and four reworked for polyphony:
//!
//! **Carried.** The floor reaches *up* by a ratio, so turning the level down
//! gives the movement more room instead of squashing it. The gesture is the
//! golden section of the interval being played at, so it always fits in the
//! gap. `curve` is one control from logarithmic through linear to exponential.
//!
//! **Reworked.** The floor is per-voice while held rather than the master. The
//! interval is fed in rather than measured, because Cadence knows it exactly.
//! `attack` is a *fraction of the gesture*, never a time. And each voice runs
//! its components on a golden phase offset, so a chord breathes as five bodies
//! rather than pumping as one.
//!
//! ## Phase accumulates; it is never derived from elapsed time
//!
//! Every component integrates `rate × dt`. Computing `elapsed × rate` instead
//! would be cheaper and is what the reference this design borrowed from does —
//! safely, because its rates never change. Here rates follow frequency, so that
//! method would snap every component on every pitch change by an amount growing
//! with session length. `fibonacci-synth` shipped that bug once: moving RIP
//! snapped the whole shell, and the snap got worse all session.

/// The event envelope: rise, fall, and a tail that approaches zero without
/// ever arriving.
///
/// **There is no floor and no resting level.** There used to be: amplitude was
/// `depth * (1/φ² + (1 - 1/φ²) * shaped)`, so when a gesture ended the voice
/// parked at 38% of depth and stayed there for as long as the instrument ran.
/// That is what made it drone rather than sound. Events happen; between them
/// this tends to nothing.
///
/// ## Why a power law and not an exponential
///
/// An exponential has a fixed half-life. It is a straight line in dB, so it
/// fades on a schedule and you can hear the schedule — and to stop it running
/// to literal zero you have to pick a cutoff, which is exactly the hardcoded
/// imperceptible value this is meant to avoid.
///
/// A power law, `1 / (1 + t/τ)^p`, is heavy-tailed. It decays strictly more
/// slowly than *any* exponential in the long run, it approaches zero
/// asymptotically and never reaches it, and nothing has to be clamped. The
/// justification is perceptual rather than aesthetic: loudness follows
/// Stevens' power law — loudness ∝ intensity^0.3 — so a power-law amplitude
/// decay is close to linear in perceived loudness, which is what "still
/// fading" actually sounds like. A dB-linear fade sounds like it is being
/// turned off.
///
/// `p` is the whole point and it is a control, not a constant: **low is
/// slower**. It is the slowest-descent dial, and where it should sit is a
/// question for ears.
///
/// τ is derived rather than dialled, so every control means something you can
/// hear: `decay` is the time to fall from the peak to `knee`, and τ follows
/// from wanting the curve to pass through that point.
#[inline]
pub fn envelope(since: f32, gesture_s: f32, p: &FieldParams) -> f32 {
    let attack_s = (gesture_s * p.attack.clamp(0.0, 1.0)).max(1e-4);

    if since < attack_s {
        // Logarithmic rise: quick off the mark, easing into the peak, which is
        // what softens the front of a pad. Linear reads as a ramp and an
        // exponential rise reads as a swell arriving late.
        const K: f32 = 9.0;
        let x = (since / attack_s).clamp(0.0, 1.0);
        return (1.0 + K * x).ln() / (1.0 + K).ln();
    }

    let knee = p.knee.clamp(0.02, 0.95);
    let tail = p.tail.clamp(0.05, 6.0);
    let decay = p.decay.max(0.02);

    // Put the curve through (decay, knee): knee = 1/(1 + decay/τ)^tail.
    let tau = decay / (knee.powf(-1.0 / tail) - 1.0).max(1e-6);
    let t = since - attack_s;
    1.0 / (1.0 + t / tau).powf(tail)
}

pub mod signature;

use core::f32::consts::TAU;
use phyllotaxis_tuning::{Algorithm, Variant};
use signature::{signature_for, Signature, RATE_RUNGS};

const PHI: f32 = 1.618_033_9;

/// Where depth rests between notes, as a fraction of `depth` — `1/φ²`. A note
/// interpolates from here up to full, so a strike is φ² deeper than rest and
/// nothing saturates.
const REST_FRACTION: f32 = 1.0 / (PHI * PHI);

/// How much of the gap a gesture occupies: `1/φ`, leaving `1/φ²` at rest.
const GESTURE_FRACTION: f32 = 1.0 / PHI;

/// A release is the same gesture, smaller — not a different mechanism.
const RELEASE_BOOST: f32 = 1.0 / PHI;

/// `curve`'s exponent range as a power of φ. 0.5 is linear; toward 0 it holds
/// then falls away; toward 1 it drops fast and lingers near nothing.
const CURVE_RANGE: f32 = 4.0;

/// Gesture length when nothing has established an interval — Fibonacci 987 ms.
///
/// A **fallback**, not a ceiling. `breath.rs` uses this value for both, and
/// copying its clamp brought the conflation across: any interval above 1.597 s
/// silently stopped producing `interval/φ` and produced 0.987 s instead, so at
/// a 3.2 s chord rate the gesture was 0.987 s where the design says 1.978 s.
/// That is the golden-section derivation — the entire reason there is no ADSR
/// here — quietly not happening at every rate a pad actually plays at.
const LONE_NOTE_S: f32 = 0.987;

const MIN_INTERVAL_S: f32 = 0.05;
const MAX_INTERVAL_S: f32 = 8.0;

/// Movement rate bounds, in Hz, derived from the playable range rather than
/// from a drone knob: `A0 / φ¹³` to `C8 / φ¹³`. The clamp exists so a very high
/// note cannot walk the movement up toward audio rate, which would be a
/// different effect wearing this one's name.
const RATE_MIN_HZ: f32 = 27.5 / 521.002;
const RATE_MAX_HZ: f32 = 4186.0 / 521.002;

#[derive(Clone, Copy, Debug)]
pub struct FieldParams {
    /// Where the tail takes over, as a fraction of the peak. Billy's ~40%.
    pub knee: f32,
    /// How deeply the five-component field modulates the event. 0 is none.
    pub depth: f32,
    /// Seconds from the peak down to `knee`.
    pub decay: f32,
    /// Power-law exponent for the tail. LOW IS SLOWER.
    pub tail: f32,
    /// Fraction of the gesture spent rising. **Not a time.**
    ///
    /// As a fraction it inherits the interval-derived length, so it is right at
    /// 8 Hz and at 0.1 Hz alike, and "attack longer than the note" is not a
    /// reachable state. 0 is an instantaneous strike; 1 is a gesture that is
    /// all swell and arrives as the next chord does.
    pub attack: f32,
}

impl Default for FieldParams {
    fn default() -> Self {
        Self { knee: 0.40, depth: 0.7, decay: 2.5, tail: 0.9, attack: 0.35 }
    }
}

/// One voice's movement.
#[derive(Clone, Debug)]
pub struct Field {
    sample_rate: f32,
    signature: Signature,
    rates: [f32; 5],
    amps: [f32; 5],
    phases: [f32; 5],

    interval_s: f32,
    since: f32,
    boost_level: f32,
    active: bool,
}

impl Field {
    pub fn new(sample_rate: f32, algorithm: Algorithm, variant: Variant) -> Self {
        let signature = signature_for(algorithm, variant);
        Self {
            sample_rate,
            signature,
            rates: signature.rates(),
            amps: signature.amps(),
            phases: [0.0; 5],
            interval_s: LONE_NOTE_S / GESTURE_FRACTION,
            since: LONE_NOTE_S,
            boost_level: 0.0,
            active: false,
        }
    }

    pub fn signature(&self) -> Signature {
        self.signature
    }

    pub fn set_entry(&mut self, algorithm: Algorithm, variant: Variant) {
        self.signature = signature_for(algorithm, variant);
        self.rates = self.signature.rates();
        self.amps = self.signature.amps();
    }

    /// Offset this voice's components so a chord breathes as several bodies.
    ///
    /// `frac(n/φ)` — the same golden rotation the pitch source uses, applied to
    /// phase. Five voices running one signature in phase pump as a single
    /// object, which is the failure this whole module exists to avoid.
    pub fn set_voice_index(&mut self, n: usize) {
        let base = ((n as f32) * (PHI - 1.0)).fract();
        for (i, p) in self.phases.iter_mut().enumerate() {
            *p = (base + i as f32 * 0.2).fract();
        }
    }

    /// The chord-change interval, fed from Cadence rather than measured.
    pub fn set_interval(&mut self, secs: f32) {
        self.interval_s = secs.clamp(MIN_INTERVAL_S, MAX_INTERVAL_S);
    }

    /// How long the current gesture lasts: the golden section of the interval,
    /// so it always fits inside the gap — **at every interval**, which is the
    /// point of deriving it rather than dialling it.
    ///
    /// No upper clamp. `interval_s` is already bounded to `MAX_INTERVAL_S`, so
    /// the gesture is bounded by construction at `8/φ ≈ 4.94 s`; adding a
    /// second ceiling on top of that does not protect anything, it just breaks
    /// the derivation above 1.597 s.
    pub fn gesture_s(&self) -> f32 {
        (self.interval_s * GESTURE_FRACTION).max(MIN_INTERVAL_S * GESTURE_FRACTION)
    }

    /// A note begins. Not an attack — the movement simply deepens.
    pub fn strike(&mut self) {
        self.since = 0.0;
        self.boost_level = 1.0;
        self.active = true;
    }

    /// A note ends: the same gesture at `1/φ`, and it does not restart the
    /// movement. The note letting go is a change in what is already happening.
    pub fn release(&mut self) {
        if self.boost(self.since) < RELEASE_BOOST {
            self.since = 0.0;
            self.boost_level = RELEASE_BOOST;
        }
    }

    fn curve_exponent(curve: f32) -> f32 {
        PHI.powf(CURVE_RANGE * (curve.clamp(0.0, 1.0) - 0.5))
    }

    /// The gesture's shape at `t` seconds in, before `attack` is applied.
    fn boost(&self, t: f32) -> f32 {
        let g = self.gesture_s();
        if t >= g { 0.0 } else { self.boost_level * (1.0 - t / g) }
    }

    /// The movement rate for a frequency, clamped to the playable range.
    pub fn rate_hz(freq: f32) -> f32 {
        (freq.max(0.0) / PHI.powi(RATE_RUNGS)).clamp(RATE_MIN_HZ, RATE_MAX_HZ)
    }

    /// This voice's amplitude `ahead` seconds from now, **without advancing**.
    ///
    /// The field is a closed form, not a sampled signal: five sinusoids at
    /// known rates and phases, times a gesture whose shape is known from its
    /// start. So any future level can be evaluated analytically — no lookahead
    /// buffer, no per-sample bookkeeping, and cheap enough to call a handful of
    /// times per note-on at control rate.
    ///
    /// This is what makes predictive allocation possible at all: the allocator
    /// can ask which voice will be quietest at the moment a note actually needs
    /// the slot, rather than which is quietest now — and those are different
    /// about half the time, because a quiet voice may be climbing out of its
    /// trough while a louder one falls into its own.
    pub fn predict(&self, ahead: f32, freq_hz: f32, p: &FieldParams) -> f32 {
        let base = Self::rate_hz(freq_hz);

        let mut movement = 0.0;
        for i in 0..5 {
            let phase = self.phases[i] + base * self.rates[i] * ahead;
            movement += (phase.fract() * TAU).sin() * self.amps[i];
        }
        let movement01 = movement * 0.5 + 0.5;

        // The SAME envelope the audio path uses. Prediction that disagrees
        // with reality is worse than no prediction: the allocator would steal
        // at a trough that is not there.
        let shaped = self.boost_level * envelope(self.since + ahead, self.gesture_s(), p);
        let d = p.depth.clamp(0.0, 1.0);
        shaped * (1.0 - d + d * movement01)
    }

    /// When this voice is next at a local minimum **quieter than it is now**,
    /// searched over `window` seconds — the moment a steal is inaudible before
    /// any fade is applied.
    ///
    /// The "quieter than now" clause is load-bearing rather than fussy. A local
    /// minimum is not necessarily below the present level: if the field is
    /// rising at t=0 it can crest and come back down to a trough that is still
    /// louder than where it started, and scheduling a steal there is worse than
    /// stealing immediately. The first version of this searched for any local
    /// minimum and a test caught it doing exactly that.
    ///
    /// Returns `None` if the window contains no such moment, in which case the
    /// caller inherits the level and crossfades rather than waiting.
    pub fn next_trough(&self, window: f32, freq_hz: f32, p: &FieldParams) -> Option<f32> {
        const STEPS: usize = 96;
        if window <= 0.0 {
            return None;
        }
        let dt = window / STEPS as f32;
        let mut best: Option<(f32, f32)> = None;
        let now = self.predict(0.0, freq_hz, p);
        let mut prev = now;
        let mut cur = self.predict(dt, freq_hz, p);
        for k in 1..STEPS {
            let t = (k + 1) as f32 * dt;
            let next = self.predict(t, freq_hz, p);
            if cur < prev && cur <= next && cur < now {
                let at = k as f32 * dt;
                if best.map_or(true, |(_, v)| cur < v) {
                    best = Some((at, cur));
                }
            }
            prev = cur;
            cur = next;
        }
        best.map(|(t, _)| t)
    }

    /// Advance one sample and return this voice's amplitude.
    ///
    /// Never returns less than `floor` while the voice is held — by
    /// construction, not by clamping.
    #[inline]
    pub fn tick(&mut self, freq_hz: f32, p: &FieldParams) -> f32 {
        let dt = 1.0 / self.sample_rate;
        let base = Self::rate_hz(freq_hz);

        // rate x dt, accumulated. Never elapsed x rate.
        let mut movement = 0.0;
        for i in 0..5 {
            self.phases[i] += base * self.rates[i] * dt;
            if self.phases[i] >= 1.0 {
                self.phases[i] -= 1.0;
            }
            movement += (self.phases[i] * TAU).sin() * self.amps[i];
        }
        let movement01 = movement * 0.5 + 0.5;

        self.since += dt;
        let shaped = self.boost_level * envelope(self.since, self.gesture_s(), p);

        // The field MODULATES the event; it no longer sets a resting level.
        // `depth` at 0 leaves the envelope untouched, at 1 the five components
        // swing it all the way down and back.
        let d = p.depth.clamp(0.0, 1.0);
        shaped * (1.0 - d + d * movement01)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phyllotaxis_tuning::ROSTER;

    const SR: f32 = 48_000.0;

    fn field() -> Field {
        Field::new(SR, Algorithm::Fm2, Variant::Golden)
    }

    /// **Between events, amplitude tends to nothing.**
    ///
    /// This replaces `the_floor_is_a_floor`, which asserted the opposite: that
    /// a held voice never falls below a resting level. That level was the
    /// instrument's whole problem — it is why it droned instead of sounding
    /// events, and it was a constant (`1/φ²` of depth) rather than a decision.
    #[test]
    fn a_voice_left_alone_falls_towards_silence() {
        let (a, v) = ROSTER[0];
        let mut f = Field::new(SR, a, v);
        f.set_interval(2.0);
        f.strike();
        let p = FieldParams { depth: 0.0, ..Default::default() };
        let mut peak: f32 = 0.0;
        for _ in 0..(SR as usize / 2) {
            peak = peak.max(f.tick(220.0, &p));
        }
        // Thirty seconds later it must be a small fraction of what it was.
        let mut last = 0.0;
        for _ in 0..(SR as usize * 30) {
            last = f.tick(220.0, &p);
        }
        // The old model parked at a CONSTANT 1/φ² = 38% of depth and stayed
        // there forever. The bar is being nowhere near that, and still
        // falling — not an arbitrary smallness.
        assert!(
            last < peak * 0.12,
            "after 30 s it is still at {last} against a peak of {peak} — that is a drone"
        );
        let later = f.tick(220.0, &p);
        assert!(later <= last * 1.0001, "stopped decaying at {last}");
    }

    /// **...but it never actually arrives.**
    ///
    /// The pair is the point. Anything that decays to a hard zero has a moment
    /// where it switches off, and a floor never decays at all. A power law
    /// gives neither: strictly decreasing, strictly positive, forever, with no
    /// cutoff constant anywhere.
    #[test]
    fn the_tail_approaches_zero_without_reaching_it() {
        let p = FieldParams { tail: 0.9, knee: 0.4, decay: 2.5, ..Default::default() };
        let g = 1.0;
        let mut prev = envelope(0.5, g, &p);
        for step in 1..2000 {
            // Out to well over an hour.
            let t = 0.5 + step as f32 * 2.0;
            let a = envelope(t, g, &p);
            assert!(a > 0.0, "hit zero at {t}s");
            assert!(a < prev, "stopped decreasing at {t}s");
            prev = a;
        }
        assert!(prev < 1e-3, "an hour in it is still at {prev}");
    }

    /// A lower `tail` exponent must decay MORE SLOWLY. It is the control Billy
    /// is meant to find a point on, so its direction has to be true.
    #[test]
    fn a_lower_tail_exponent_descends_more_slowly() {
        let at = |tail: f32| {
            let p = FieldParams { tail, knee: 0.4, decay: 2.5, ..Default::default() };
            envelope(60.0, 1.0, &p)
        };
        let (slow, fast) = (at(0.3), at(3.0));
        assert!(slow > fast, "tail 0.3 gave {slow}, tail 3.0 gave {fast}");
    }


    /// The gesture is the golden section of the interval, at **every** rate.
    ///
    /// The first version of this test carried the guard
    /// `if interval < LONE_NOTE_S / GESTURE_FRACTION` around its exactness
    /// check — which is precisely the region where the ceiling broke the
    /// derivation. The test was shaped around the bug and passed for it. A
    /// guard that excludes the interesting case is not a guard.
    #[test]
    fn the_gesture_is_the_golden_section_at_every_interval() {
        let mut f = field();
        for &interval in &[0.05f32, 0.125, 0.5, 1.0, 1.597, 2.0, 3.2, 6.0, 8.0] {
            f.set_interval(interval);
            let g = f.gesture_s();
            assert!(g < interval, "gesture {g} does not fit inside {interval}");
            assert!(
                (g - interval * GESTURE_FRACTION).abs() < 1e-4,
                "at interval {interval}s the gesture is {g}s, not {}s",
                interval * GESTURE_FRACTION
            );
            // And the rest of the gap is 1/φ², the other half of the section.
            assert!(
                ((interval - g) - interval / (PHI * PHI)).abs() < 1e-4,
                "rest is not the complementary golden section"
            );
        }
    }

    /// The bound comes from the interval, not from a second ceiling.
    #[test]
    fn the_gesture_is_bounded_by_the_interval_clamp() {
        let mut f = field();
        f.set_interval(1000.0);
        assert!(f.gesture_s() <= MAX_INTERVAL_S * GESTURE_FRACTION + 1e-4);
        f.set_interval(0.0);
        assert!(f.gesture_s() >= MIN_INTERVAL_S * GESTURE_FRACTION - 1e-6);
    }

    /// Attack is a fraction, so it cannot outrun the gesture at any rate —
    /// the failure an ADSR has and this does not.
    #[test]
    fn attack_cannot_outrun_the_gesture() {
        for &interval in &[0.1f32, 0.5, 4.0] {
            for &attack in &[0.0f32, 0.5, 1.0] {
                let p = FieldParams { attack, ..Default::default() };
                let mut f = field();
                f.set_interval(interval);
                f.strike();
                let g = f.gesture_s();

                // Measured on the ENVELOPE, not on `tick`. `tick` is the
                // envelope times the five-component field, and with a tail
                // that runs for minutes a later movement crest can easily be
                // taller than the attack — which says nothing about whether
                // the attack outran anything. The claim is about the envelope,
                // so measure the envelope.
                let steps = 4000;
                let mut peak_at = 0.0f32;
                let mut peak = 0.0f32;
                for i in 0..steps {
                    let t = i as f32 / steps as f32 * g * 2.0;
                    let a = envelope(t, g, &p);
                    if a > peak { peak = a; peak_at = t; }
                }
                assert!(
                    peak_at <= g + 1e-3,
                    "attack {attack} at interval {interval} peaked at {peak_at}s, past the {g}s gesture"
                );
            }
        }
    }

    /// A high voice breathes faster than a low one — free internal movement in
    /// a chord, and the reason a held pad does not sound like one object.
    #[test]
    fn higher_voices_breathe_faster() {
        assert!(Field::rate_hz(880.0) > Field::rate_hz(110.0));
        assert!(Field::rate_hz(20_000.0) <= RATE_MAX_HZ + 1e-6);
        assert!(Field::rate_hz(1.0) >= RATE_MIN_HZ - 1e-6);
    }

    /// Five voices on golden offsets must not move together.
    #[test]
    fn voices_do_not_pump_as_one() {
        let p = FieldParams::default();
        let mut voices: Vec<Field> = (0..5)
            .map(|n| {
                let mut f = field();
                f.set_voice_index(n);
                f.set_interval(2.0);
                f.strike();
                f
            })
            .collect();
        let mut summed_swing = 0.0f32;
        let (mut lo, mut hi) = (f32::INFINITY, 0.0f32);
        for _ in 0..(SR as usize * 3) {
            let total: f32 = voices.iter_mut().map(|f| f.tick(220.0, &p)).sum::<f32>() / 5.0;
            lo = lo.min(total);
            hi = hi.max(total);
        }
        summed_swing = hi - lo;
        // One voice alone swings much harder than five decorrelated ones.
        let mut solo = field();
        solo.set_interval(2.0);
        solo.strike();
        let (mut slo, mut shi) = (f32::INFINITY, 0.0f32);
        for _ in 0..(SR as usize * 3) {
            let a = solo.tick(220.0, &p);
            slo = slo.min(a);
            shi = shi.max(a);
        }
        assert!(
            summed_swing < (shi - slo) * 0.8,
            "five voices swung {summed_swing} against one voice's {}",
            shi - slo
        );
    }

    /// Every roster entry produces finite, bounded movement.
    #[test]
    fn every_entry_is_well_behaved() {
        let p = FieldParams::default();
        for &(a, v) in ROSTER.iter() {
            let mut f = Field::new(SR, a, v);
            f.set_interval(1.5);
            f.strike();
            for _ in 0..(SR as usize) {
                let x = f.tick(330.0, &p);
                assert!(x.is_finite() && (0.0..=1.0).contains(&x), "{a:?} {v:?} produced {x}");
            }
        }
    }

    /// Prediction has to be the same function as reality, or every decision
    /// built on it is decided on a fiction.
    #[test]
    fn prediction_matches_what_actually_happens() {
        let p = FieldParams::default();
        for &ahead in &[0.001f32, 0.01, 0.1, 0.5, 1.2] {
            let mut f = field();
            f.set_interval(2.0);
            f.strike();
            for _ in 0..1000 { f.tick(220.0, &p); }

            let predicted = f.predict(ahead, 220.0, &p);
            let n = (ahead * SR) as usize;
            let mut actual = 0.0;
            for _ in 0..n { actual = f.tick(220.0, &p); }
            assert!(
                (predicted - actual).abs() < 2e-3,
                "at {ahead}s predicted {predicted}, got {actual}"
            );
        }
    }

    /// A trough found by search must actually be quieter than the level now.
    #[test]
    fn a_found_trough_is_quieter_than_the_present() {
        let p = FieldParams { depth: 1.0, ..Default::default() };
        let mut f = field();
        f.set_interval(3.0);
        f.strike();
        // Start somewhere on the way up, so "now" is not already a minimum.
        for _ in 0..(SR as usize / 5) { f.tick(220.0, &p); }
        if let Some(t) = f.next_trough(1.0, 220.0, &p) {
            assert!(t > 0.0);
            assert!(
                f.predict(t, 220.0, &p) <= f.predict(0.0, 220.0, &p) + 1e-6,
                "the trough was louder than now"
            );
        }
    }

    #[test]
    fn a_release_is_a_smaller_gesture_not_a_new_one() {
        let p = FieldParams::default();
        let mut f = field();
        f.set_interval(2.0);
        f.strike();
        for _ in 0..(SR as usize) { f.tick(220.0, &p); }
        let before = f.boost_level;
        f.release();
        assert!(f.boost_level <= before.max(RELEASE_BOOST) + 1e-6);
        assert!(f.boost_level <= 1.0);
    }
}
