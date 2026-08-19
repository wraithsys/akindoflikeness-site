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
    /// The level this voice never falls below while it is held. 0…1.
    pub floor: f32,
    /// How far the field reaches up from the floor. 0…1.
    pub depth: f32,
    /// Logarithmic → linear → exponential. 0…1, 0.5 linear.
    pub curve: f32,
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
        Self { floor: 0.0, depth: 0.7, curve: 0.5, attack: 0.35 }
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
    /// so it always fits inside the gap.
    pub fn gesture_s(&self) -> f32 {
        (self.interval_s * GESTURE_FRACTION)
            .clamp(MIN_INTERVAL_S * GESTURE_FRACTION, LONE_NOTE_S)
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
        let g = self.gesture_s();
        let exp = Self::curve_exponent(p.curve);
        let attack_s = g * p.attack.clamp(0.0, 1.0);

        let shaped = if self.since >= g {
            0.0
        } else if self.since < attack_s && attack_s > 0.0 {
            // Rising: the leading edge, as a fraction of the gesture.
            self.boost_level * (self.since / attack_s).powf(1.0 / exp)
        } else {
            let fall = (self.since - attack_s) / (g - attack_s).max(1e-6);
            self.boost_level * (1.0 - fall).max(0.0).powf(exp)
        };

        // Depth rests at 1/phi^2 and interpolates up to full, so a note is
        // phi^2 deeper than rest and nothing saturates.
        let rest = REST_FRACTION;
        let depth_now = p.depth * (rest + (1.0 - rest) * shaped);

        // Reaches UP from the floor by a ratio, never down through it.
        let floor = p.floor.clamp(0.0, 1.0);
        floor + (1.0 - floor) * depth_now * movement01
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

    /// The invariant the module exists for: with a voice held, nothing takes
    /// the level below the floor.
    #[test]
    fn the_floor_is_a_floor() {
        for &floor in &[0.0f32, 0.25, 0.5, 0.9, 1.0] {
            let p = FieldParams { floor, depth: 1.0, ..Default::default() };
            let mut f = field();
            f.set_interval(1.0);
            f.strike();
            for i in 0..(SR as usize * 4) {
                let a = f.tick(220.0, &p);
                assert!(a >= floor - 1e-5, "fell through the floor at {i}: {a} < {floor}");
                assert!(a <= 1.0 + 1e-5, "exceeded unity: {a}");
            }
        }
    }

    /// Turning the floor down gives the movement more room, rather than
    /// squashing it — the property a ratio has and an offset does not.
    #[test]
    fn a_lower_floor_leaves_more_room_to_move() {
        let swing = |floor: f32| {
            let p = FieldParams { floor, depth: 1.0, ..Default::default() };
            let mut f = field();
            f.set_interval(1.0);
            f.strike();
            let (mut lo, mut hi) = (f32::INFINITY, 0.0f32);
            for _ in 0..(SR as usize * 3) {
                let a = f.tick(220.0, &p);
                lo = lo.min(a);
                hi = hi.max(a);
            }
            hi - lo
        };
        assert!(swing(0.0) > swing(0.5), "{} vs {}", swing(0.0), swing(0.5));
        assert!(swing(1.0) < 1e-4, "at floor 1.0 there is nowhere left to go");
    }

    /// The gesture is the golden section of the interval, at every rate.
    #[test]
    fn the_gesture_fits_the_interval() {
        let mut f = field();
        for &interval in &[0.125f32, 0.5, 2.0, 6.0] {
            f.set_interval(interval);
            let g = f.gesture_s();
            assert!(g <= interval, "gesture {g} does not fit in {interval}");
            if interval > MIN_INTERVAL_S && interval < LONE_NOTE_S / GESTURE_FRACTION {
                assert!((g - interval * GESTURE_FRACTION).abs() < 1e-5);
            }
        }
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
                let n = (interval * SR) as usize;
                let mut peak_at = 0usize;
                let mut peak = 0.0f32;
                for i in 0..n {
                    let a = f.tick(220.0, &p);
                    if a > peak { peak = a; peak_at = i; }
                }
                assert!(
                    peak_at as f32 / SR <= f.gesture_s() + 0.05,
                    "attack {attack} at interval {interval} peaked after the gesture"
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
