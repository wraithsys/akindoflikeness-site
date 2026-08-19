//! Five voices, allocated by prediction rather than by age.
//!
//! The usual "steal the quietest" is wrong about half the time, because it asks
//! the wrong question: a voice that is quiet *now* may be climbing out of its
//! trough while a slightly louder one is about to fall into its own. And the
//! moment that matters is not now either — the strum says exactly when the
//! incoming note needs the slot.
//!
//! Since the field is a closed form (`Field::predict`), the right questions are
//! all answerable:
//!
//! 1. **Which voice will be quietest when the note actually arrives**, not which
//!    is quietest at the instant of asking.
//! 2. **When, inside that window, is the victim at a local minimum** — a steal
//!    at the bottom of a voice's own breath is inaudible before any fade is
//!    applied. The fade is insurance for when no trough falls inside the window,
//!    not the primary mechanism.
//! 3. **What level is it at then**, so the incoming voice can start its rise
//!    from there. The slot's level never steps: continuity by construction,
//!    the same argument as the floor.
//!
//! Level continuity is not waveform continuity — the new voice is at a
//! different pitch and phase — so a short crossfade still hides the signal
//! edge. But it no longer has to hide a level drop, so it is an order of
//! magnitude shorter than it would otherwise be. See `DESIGN.md` §6.

use phyllotaxis_field::{Field, FieldParams};

use phyllotaxis_voice::{Voice, VoiceParams};

/// A Fibonacci number, and the reason the whole design fits: five voices is
/// enough only because the plate carries stolen tails, voice leading holds
/// common tones, and this module declines work that would be masked.
pub const VOICES: usize = 5;

/// Two pitches this close are the same pitch.
///
/// Not a round number of cents because the scales are not 12-TET: a
/// spectrum-derived degree lands on values like 968.81¢, so the tolerance has
/// to be a musical threshold rather than a grid tolerance. Five cents is about
/// where a pitch difference stops being audible on a sustained tone.
pub const COMMON_TONE_CENTS: f32 = 5.0;

/// How long a steal crossfades when no trough was available. Short, because it
/// only has to hide a waveform edge — the level is already continuous.
pub const STEAL_FADE_S: f32 = 0.002;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotState {
    Free,
    Held,
    Released,
}

/// What the allocator decided, and why.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    /// A free slot was available; nothing was displaced.
    Free { slot: usize },
    /// This pitch is already sounding within `COMMON_TONE_CENTS`. Do not
    /// allocate and do not retrigger — the note never stopped, so restarting
    /// its gesture would be an audible fault caused by bookkeeping.
    AlreadySounding { slot: usize },
    /// A voice is displaced.
    Steal {
        slot: usize,
        /// Seconds from now, inside the window, at which to make the swap.
        at: f32,
        /// The victim's level then. The incoming voice starts here.
        inherit: f32,
        /// True if `at` is a genuine local minimum rather than a fallback.
        at_trough: bool,
    },
}

pub struct Slot {
    pub voice: Voice,
    pub field: Field,
    pub freq_hz: f32,
    pub state: SlotState,
    /// Increments on every note-on, so ties break toward the oldest.
    pub started: u64,
}

/// How far each voice's modulation index breathes around the panel value.
///
/// Billy, 2026-08-19: *"fm is almost always better as subtle and shifting on
/// pads — perhaps thats the main thing it lacks at the moment … natural
/// movement of things that aren't the inherited field model."* The field
/// moves amplitude and the chorus moves time; before this, nothing moved
/// timbre — the index sat wherever the slider left it.
pub const DRIFT_DEPTH: f32 = 0.15;

/// Seconds per breath, one per voice. Fibonacci, so adjacent rates relate by
/// ~φ and the set never re-aligns inside a session; assigned so the pedal
/// (slot 0) breathes slowest. Deterministic throughout — phase accumulates,
/// nothing reads wall time, and a preset remains a coordinate.
const DRIFT_PERIOD_S: [f32; VOICES] = [233.0, 89.0, 55.0, 144.0, 34.0];

pub struct Pool {
    pub slots: Vec<Slot>,
    pub field_params: FieldParams,
    counter: u64,
    /// The panel index — the anchor the drift breathes around.
    base_index: f32,
    /// One breath phase per voice, in cycles, started on the golden rotation
    /// so the five don't cross their centre together at t = 0.
    drift_phase: [f32; VOICES],
}

fn cents_between(a: f32, b: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 {
        return f32::INFINITY;
    }
    1200.0 * (a / b).log2().abs()
}

/// Equal-power pan gains per slot, placed on the golden rotation.
///
/// `frac(n/φ)` spread across the field, then narrowed to `SPREAD` of full
/// width so the pad stays a body rather than two edges. Computed once here
/// rather than per sample.
#[cfg_attr(not(test), allow(dead_code))] // read by the test that regenerates PAN
const SPREAD: f32 = 0.72;
static PAN: [(f32, f32); VOICES] = pan_table();

const fn pan_table() -> [(f32, f32); VOICES] {
    // `const fn` has no float trig, so the table is written out. Values are
    // `frac(n/φ)` mapped to 0.5 ± SPREAD·(x − 0.5) and taken through
    // `cos`/`sin` of `x·π/2`; `pan_positions_follow_the_golden_rotation`
    // recomputes them and checks.
    [
        (0.975917, 0.218143),
        (0.606702, 0.79493),
        (0.883788, 0.467887),
        (0.375483, 0.926829),
        (0.729035, 0.684476),
    ]
}

impl Pool {
    pub fn new(sample_rate: f32, entry: usize, params: VoiceParams) -> Self {
        let slots = (0..VOICES)
            .map(|n| {
                let mut voice = Voice::new(sample_rate);
                voice.set_params(params);
                voice.set_phase_offset((n as f32 * 0.618_034) % 1.0);
                let mut field = Field::new(sample_rate, entry);
                field.set_voice_index(n);
                Slot { voice, field, freq_hz: 0.0, state: SlotState::Free, started: 0 }
            })
            .collect();
        Self {
            slots,
            field_params: FieldParams::default(),
            counter: 0,
            base_index: params.index,
            drift_phase: [0.0, 0.618_034, 0.236_068, 0.854_102, 0.472_136],
        }
    }

    pub fn set_entry(&mut self, entry: usize, params: VoiceParams) {
        self.base_index = params.index;
        for s in self.slots.iter_mut() {
            s.voice.set_params(params);
            s.field.set_entry(entry);
        }
    }

    /// Breathe each voice's index around the panel value. Control rate: the
    /// caller hands in the block's duration and the phases integrate it, per
    /// the same rule the field lives by — phase accumulates, it is never
    /// derived from elapsed time.
    pub fn drift_indices(&mut self, dt: f32) {
        for i in 0..VOICES {
            let ph = (self.drift_phase[i] + dt / DRIFT_PERIOD_S[i]).fract();
            self.drift_phase[i] = ph;
            let idx = self.base_index
                * (1.0 + DRIFT_DEPTH * (core::f32::consts::TAU * ph).sin());
            self.slots[i].voice.set_index(idx);
        }
    }

    pub fn set_interval(&mut self, secs: f32) {
        for s in self.slots.iter_mut() {
            s.field.set_interval(secs);
        }
    }

    /// Decide where a note goes, given how long until it actually has to sound.
    ///
    /// `when_needed_s` comes from the strum: a note at the far end of an
    /// ascending strum has the whole span to wait, and that waiting is what
    /// buys the chance to steal at a trough.
    pub fn decide(&self, freq_hz: f32, when_needed_s: f32) -> Decision {
        // Already sounding? Then it is not a new note.
        for (i, s) in self.slots.iter().enumerate() {
            if s.state == SlotState::Held && cents_between(s.freq_hz, freq_hz) < COMMON_TONE_CENTS {
                return Decision::AlreadySounding { slot: i };
            }
        }

        if let Some(i) = self.slots.iter().position(|s| s.state == SlotState::Free) {
            return Decision::Free { slot: i };
        }

        // Released voices before held ones; within a class, the one that will
        // be quietest WHEN THE NOTE ARRIVES.
        let rank = |s: &Slot| match s.state {
            SlotState::Free => 0u8,
            SlotState::Released => 1,
            SlotState::Held => 2,
        };
        let best_rank = self.slots.iter().map(rank).min().unwrap_or(2);

        let mut victim = 0usize;
        let mut quietest = f32::INFINITY;
        for (i, s) in self.slots.iter().enumerate() {
            if rank(s) != best_rank {
                continue;
            }
            let level = s.field.predict(when_needed_s, s.freq_hz, &self.field_params);
            // Ties break toward the oldest note.
            if level < quietest - 1e-6
                || ((level - quietest).abs() <= 1e-6 && s.started < self.slots[victim].started)
            {
                quietest = level;
                victim = i;
            }
        }

        let s = &self.slots[victim];
        match s.field.next_trough(when_needed_s, s.freq_hz, &self.field_params) {
            Some(at) => Decision::Steal {
                slot: victim,
                at,
                inherit: s.field.predict(at, s.freq_hz, &self.field_params),
                at_trough: true,
            },
            None => Decision::Steal {
                slot: victim,
                at: when_needed_s,
                inherit: quietest,
                at_trough: false,
            },
        }
    }

    /// Commit a decision.
    pub fn note_on(&mut self, freq_hz: f32, when_needed_s: f32) -> Decision {
        let decision = self.decide(freq_hz, when_needed_s);
        self.counter += 1;
        match decision {
            Decision::AlreadySounding { .. } => {}
            Decision::Free { slot } | Decision::Steal { slot, .. } => {
                let n = self.counter;
                let s = &mut self.slots[slot];
                s.voice.set_root_hz(freq_hz);
                s.freq_hz = freq_hz;
                s.state = SlotState::Held;
                s.started = n;
                s.field.strike();
            }
        }
        decision
    }

    pub fn note_off(&mut self, freq_hz: f32) {
        for s in self.slots.iter_mut() {
            if s.state == SlotState::Held && cents_between(s.freq_hz, freq_hz) < COMMON_TONE_CENTS {
                s.state = SlotState::Released;
                s.field.release();
            }
        }
    }

    /// Advance one sample, summing every sounding voice.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let (l, r) = self.tick_stereo();
        (l + r) * 0.5
    }

    /// Advance one sample, with the voices placed across the field.
    ///
    /// Five voices summed to one point is five voices you cannot hear apart:
    /// the whole argument for polyphony is that the parts stay distinguishable,
    /// and in a pad — where the notes sustain and overlap rather than
    /// articulating — position is most of what separates them.
    ///
    /// The placement is the golden rotation again, `frac(n/φ)`, the same
    /// sequence that offsets each voice's field phase and picks the chord root.
    /// Consecutive slots land far apart and the set never clumps, which is the
    /// property phyllotaxis is named for. Equal-power, so moving a voice across
    /// the field does not change how loud it is.
    #[inline]
    pub fn tick_stereo(&mut self) -> (f32, f32) {
        let p = self.field_params;
        let (mut l, mut r) = (0.0, 0.0);
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.state == SlotState::Free {
                continue;
            }
            let v = s.voice.tick() * s.field.tick(s.freq_hz, &p);
            let (gl, gr) = PAN[i];
            l += v * gl;
            r += v * gr;
        }
        let n = 1.0 / (VOICES as f32).sqrt();
        (l * n, r * n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

/// The hand-written `PAN` table, recomputed. A table of magic constants is
    /// only as good as the thing that checks it.
    #[test]
    fn pan_positions_follow_the_golden_rotation() {
        const PHI: f32 = 1.618_034;
        for i in 0..VOICES {
            let x = ((i as f32) / PHI).fract();
            let placed = 0.5 + SPREAD * (x - 0.5);
            let theta = placed * std::f32::consts::FRAC_PI_2;
            let (want_l, want_r) = (theta.cos(), theta.sin());
            let (got_l, got_r) = PAN[i];
            assert!(
                (got_l - want_l).abs() < 5e-4 && (got_r - want_r).abs() < 5e-4,
                "slot {i}: table says ({got_l}, {got_r}), golden rotation says ({want_l}, {want_r})"
            );
            // Equal power: moving a voice across the field must not change how
            // loud it is.
            assert!((got_l * got_l + got_r * got_r - 1.0).abs() < 1e-3);
        }
    }

    /// Voices must actually land in different places. If they do not, this is
    /// dual mono wearing a pan control.
    #[test]
    fn the_voices_are_not_all_in_the_same_place() {
        let mut seen: Vec<f32> = PAN.iter().map(|(l, r)| r - l).collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let closest = seen.windows(2).map(|w| w[1] - w[0]).fold(f32::MAX, f32::min);
        assert!(closest > 0.1, "two voices sit within {closest} of each other");
    }

        fn pool() -> Pool {
        let mut p = Pool::new(SR, 0, VoiceParams::default());
        p.field_params = FieldParams { depth: 1.0, ..Default::default() };
        p.set_interval(2.0);
        p
    }

    fn fill(p: &mut Pool) {
        for k in 0..VOICES {
            p.note_on(110.0 * 2f32.powf(k as f32 * 0.19), 0.0);
        }
    }

    #[test]
    fn free_slots_are_used_before_anything_is_stolen() {
        let mut p = pool();
        for k in 0..VOICES {
            assert!(matches!(p.note_on(200.0 + k as f32 * 40.0, 0.05), Decision::Free { .. }));
        }
        assert!(matches!(p.note_on(999.0, 0.05), Decision::Steal { .. }));
    }

    /// A pitch already sounding is not a new note. Retriggering it would
    /// restart a gesture on a note that never stopped — a fault caused purely
    /// by bookkeeping, and exactly what voice leading exists to avoid.
    #[test]
    fn a_common_tone_is_never_retriggered() {
        let mut p = pool();
        p.note_on(220.0, 0.0);
        assert!(matches!(p.note_on(220.0, 0.05), Decision::AlreadySounding { .. }));
        // Three cents away is the same note; twenty is not.
        assert!(matches!(p.note_on(220.0 * 2f32.powf(3.0 / 1200.0), 0.05), Decision::AlreadySounding { .. }));
        assert!(matches!(p.note_on(220.0 * 2f32.powf(20.0 / 1200.0), 0.05), Decision::Free { .. }));
    }

    #[test]
    fn released_voices_are_taken_before_held_ones() {
        let mut p = pool();
        fill(&mut p);
        let released_freq = p.slots[2].freq_hz;
        p.note_off(released_freq);
        match p.decide(1000.0, 0.05) {
            Decision::Steal { slot, .. } => assert_eq!(slot, 2),
            other => panic!("expected a steal, got {other:?}"),
        }
    }

    /// The victim is the voice quietest **when the note arrives**, which is not
    /// the same voice as the one quietest now.
    #[test]
    fn the_victim_minimises_level_at_the_moment_of_arrival() {
        let mut p = pool();
        fill(&mut p);
        for _ in 0..(SR as usize / 3) { p.tick(); }

        let window = 0.12;
        let chosen = match p.decide(1500.0, window) {
            Decision::Steal { slot, .. } => slot,
            other => panic!("expected a steal, got {other:?}"),
        };
        let level_at = |i: usize| {
            p.slots[i].field.predict(window, p.slots[i].freq_hz, &p.field_params)
        };
        for i in 0..VOICES {
            assert!(
                level_at(chosen) <= level_at(i) + 1e-6,
                "chose {chosen} at {} but {i} is at {}",
                level_at(chosen), level_at(i)
            );
        }
    }

    /// The distinction is real rather than academic: there exists a moment when
    /// the quietest-now voice is NOT the quietest-on-arrival. If this ever
    /// stopped being true, prediction would be an elaborate way to do nothing.
    #[test]
    fn quietest_now_and_quietest_on_arrival_genuinely_differ() {
        let mut p = pool();
        fill(&mut p);
        let window = 0.15;
        let mut disagreements = 0;
        for _ in 0..400 {
            for _ in 0..(SR as usize / 200) { p.tick(); }
            let argmin = |ahead: f32| {
                (0..VOICES)
                    .min_by(|&a, &b| {
                        let la = p.slots[a].field.predict(ahead, p.slots[a].freq_hz, &p.field_params);
                        let lb = p.slots[b].field.predict(ahead, p.slots[b].freq_hz, &p.field_params);
                        la.partial_cmp(&lb).unwrap()
                    })
                    .unwrap()
            };
            if argmin(0.0) != argmin(window) {
                disagreements += 1;
            }
        }
        assert!(
            disagreements > 20,
            "only {disagreements}/400 disagreements — prediction is not buying anything"
        );
    }

    /// A steal has to be able to wait for a quieter moment. What "quieter"
    /// means changed with the envelope, and this test changed with it.
    ///
    /// It used to assert that a low voice has no reachable trough in a
    /// strum-length window and a high one does — true when amplitude parked at
    /// a constant resting level and only oscillated, so the only way to get
    /// quieter was to catch a dip. Now every voice is decaying, so past the
    /// attack "quieter" is usually just *later*, and the oscillation may not
    /// produce a local minimum at all because the decay outruns it.
    ///
    /// `next_trough` still answers its own question honestly — it finds a
    /// local minimum below the present level, or says there isn't one, in
    /// which case the caller crossfades. So what is worth asserting is the
    /// property the allocator actually depends on: waiting is never worse.
    #[test]
    fn waiting_finds_a_quieter_moment_than_stealing_now() {
        let fp = FieldParams { depth: 1.0, ..Default::default() };
        let settled = |freq: f32| {
            let mut f = phyllotaxis_field::Field::new(SR, 1);
            f.set_interval(3.2);
            f.strike();
            for _ in 0..(SR as usize) { f.tick(freq, &fp); }
            f
        };
        for &freq in &[110.0f32, 220.0, 2000.0] {
            let f = settled(freq);
            let now = f.predict(0.0, freq, &fp);
            let best = (1..=64)
                .map(|k| f.predict(k as f32 / 64.0 * 0.2, freq, &fp))
                .fold(f32::MAX, f32::min);
            assert!(
                best <= now,
                "at {freq} Hz nothing in the next 0.2 s is quieter than now ({best} vs {now})"
            );
            // And when it does report a trough, that trough is below now.
            if let Some(t) = f.next_trough(0.2, freq, &fp) {
                assert!(f.predict(t, freq, &fp) < now, "reported a trough louder than now");
            }
        }
    }

    /// When a trough *is* found, it must genuinely be quieter than taking the
    /// voice immediately — otherwise scheduling is worse than not.
    #[test]
    fn a_scheduled_steal_is_quieter_than_an_immediate_one() {
        let fp = FieldParams { depth: 1.0, ..Default::default() };
        let mut found = 0;
        for k in 0..40 {
            let freq = 1200.0 + k as f32 * 55.0;
            let mut f = phyllotaxis_field::Field::new(SR, 1);
            f.set_interval(3.2);
            f.strike();
            for _ in 0..(SR as usize / 3 + k * 311) { f.tick(freq, &fp); }
            if let Some(at) = f.next_trough(0.35, freq, &fp) {
                found += 1;
                assert!(
                    f.predict(at, freq, &fp) <= f.predict(0.0, freq, &fp) + 1e-6,
                    "scheduled steal at {at}s was louder than stealing now"
                );
            }
        }
        assert!(found > 20, "only {found}/40 windows contained a trough");
    }

    /// The level the incoming voice inherits is the level the outgoing voice is
    /// actually at — so the slot never steps.
    #[test]
    fn the_inherited_level_is_the_victims_real_level() {
        let mut p = pool();
        fill(&mut p);
        for _ in 0..(SR as usize / 5) { p.tick(); }
        if let Decision::Steal { slot, at, inherit, .. } = p.decide(1200.0, 0.1) {
            let s = &p.slots[slot];
            let truth = s.field.predict(at, s.freq_hz, &p.field_params);
            assert!((inherit - truth).abs() < 1e-6, "inherit {inherit} vs {truth}");
        }
    }

    /// The drift breathes each voice around the panel index, stays inside its
    /// stated depth, actually excursions (it is movement, not a constant), and
    /// the five voices are not breathing in unison.
    #[test]
    fn the_index_breathes_around_the_panel_value_and_not_in_unison() {
        let mut p = pool();
        let panel = p.slots[0].voice.params().index;
        let block = 128.0 / SR;
        let mut lo = [f32::MAX; VOICES];
        let mut hi = [0.0f32; VOICES];
        // Six minutes of control blocks — longer than every period in the set.
        let blocks = (360.0 / block) as usize;
        for _ in 0..blocks {
            p.drift_indices(block);
            for v in 0..VOICES {
                let idx = p.slots[v].voice.params().index;
                lo[v] = lo[v].min(idx);
                hi[v] = hi[v].max(idx);
            }
        }
        for v in 0..VOICES {
            assert!(
                lo[v] >= panel * (1.0 - DRIFT_DEPTH - 0.01)
                    && hi[v] <= panel * (1.0 + DRIFT_DEPTH + 0.01),
                "voice {v} escaped the stated depth: [{}, {}] around {panel}",
                lo[v], hi[v]
            );
            assert!(
                hi[v] - lo[v] > panel * DRIFT_DEPTH,
                "voice {v} barely moved: [{}, {}]",
                lo[v], hi[v]
            );
        }
        // Not in unison: at this instant the voices hold distinct indices.
        let now: Vec<f32> = (0..VOICES).map(|v| p.slots[v].voice.params().index).collect();
        let mut sorted = now.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let closest = sorted.windows(2).map(|w| w[1] - w[0]).fold(f32::MAX, f32::min);
        assert!(closest > 1e-4, "two voices breathe as one: {now:?}");
    }

    #[test]
    fn the_pool_never_produces_a_non_finite_sample() {
        let mut p = pool();
        for k in 0..12 {
            p.note_on(90.0 + k as f32 * 57.0, 0.02);
            for _ in 0..(SR as usize / 10) {
                assert!(p.tick().is_finite());
            }
        }
    }
}
