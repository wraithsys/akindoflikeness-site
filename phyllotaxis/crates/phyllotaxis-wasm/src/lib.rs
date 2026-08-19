//! The boundary the AudioWorklet calls.
//!
//! Plain `extern "C"`, no `wasm-bindgen`. Not austerity — bindgen's glue
//! allocates and touches JS objects, and the worklet's `process()` runs on a
//! real-time thread that must not do either. What crosses this boundary is
//! four pointers and a handful of `f32`s.
//!
//! **Nothing here allocates after `phy_new`.** Every buffer is sized at
//! construction: the voice pool is five slots, the pending-event queue is
//! bounded by the voice count, and the scope ring is a fixed array. `phy_process`
//! writes into memory JS already owns.
//!
//! The worklet cannot `fetch`, so the module is compiled on the main thread and
//! the `WebAssembly.Module` is passed over `port.postMessage` — it is
//! structured-cloneable — then instantiated synchronously inside the worklet.
//! See `DESIGN.md` §1.

use phyllotaxis_cadence::{leading, mirror::Mirror, mirror_this_chord, strum, word};
use phyllotaxis_field::FieldParams;
use phyllotaxis_fx::{density::DensityParams, Bus, ChorusParams, PlateParams};
use phyllotaxis_pool::{Pool, VOICES};
use phyllotaxis_tuning::{tuning_for, Kind, Tuning, DEGREES_PER_SCALE, ROSTER};
use phyllotaxis_voice::VoiceParams;

/// The worklet's render quantum. Fixed by the platform, not by us.
pub const QUANTUM: usize = 128;

/// How many samples of output the visualiser can read back.
///
/// A power of two so the ring wraps by mask rather than by modulo, and long
/// enough to hold a couple of chord changes at 48 kHz.
pub const SCOPE_LEN: usize = 8192;

/// Parameter addresses. These are the contract with the JS side; the numbers
/// are part of the ABI and must not be reordered.
pub mod param {
    pub const ENTRY: u32 = 0; // roster index, 0..8
    pub const INDEX: u32 = 1;
    pub const MEAN_INTERVAL: u32 = 2;
    pub const MIRROR: u32 = 3;
    pub const STRUM_BIAS: u32 = 4;
    pub const FLOOR: u32 = 5;
    pub const DEPTH: u32 = 6;
    pub const CURVE: u32 = 7;
    pub const ATTACK: u32 = 8;
    pub const PLATE_DECAY: u32 = 9;
    pub const PLATE_DAMPING: u32 = 10;
    pub const PLATE_MIX: u32 = 11;
    pub const CHORUS_DEPTH: u32 = 12;
    pub const CHORUS_RATE: u32 = 13;
    pub const CHORUS_MIX: u32 = 14;
    pub const DENSITY_ON: u32 = 15;
    pub const DENSITY_AMOUNT: u32 = 16;
    pub const MASTER: u32 = 17;
    pub const COUNT: u32 = 18;
}

/// A note waiting for its strum offset to elapse.
#[derive(Clone, Copy)]
struct Pending {
    hz: f32,
    at: f32,
    fired: bool,
}

pub struct Engine {
    sample_rate: f32,
    pool: Pool,
    bus: Bus,

    entry: usize,
    tuning: Tuning,
    mirror: Mirror,
    root_cents: f64,

    /// Cadence's entire time state: which chord we are on.
    step: u64,
    /// Samples remaining in the current chord.
    countdown: usize,
    time_in_chord: f32,

    // Fixed-capacity, so the audio path never allocates.
    pending: [Pending; VOICES],
    pending_len: usize,
    sounding: [f64; VOICES],
    sounding_len: usize,

    params: [f32; param::COUNT as usize],
    scope: Box<[f32; SCOPE_LEN]>,
    scope_head: usize,
    /// The block the worklet copies out of. Owned here so JS never has to
    /// allocate inside wasm memory, and so there is no pointer to keep in sync.
    out: Box<[f32; QUANTUM]>,
}

fn defaults() -> [f32; param::COUNT as usize] {
    let mut p = [0.0f32; param::COUNT as usize];
    p[param::ENTRY as usize] = 1.0; // fm II — sparse, and the one that reads as manageable
    p[param::INDEX as usize] = 4.0;
    p[param::MEAN_INTERVAL as usize] = 1.618;
    p[param::MIRROR as usize] = word::INV_PHI2 as f32;
    p[param::STRUM_BIAS as usize] = -0.7;
    p[param::FLOOR as usize] = 0.10;
    p[param::DEPTH as usize] = 0.85;
    p[param::CURVE as usize] = 0.42;
    p[param::ATTACK as usize] = 0.30;
    // RT60 0.94s: longer than either rest window, so the tail carries through
    // rather than filling it. Settled by ear against 0.60. See DESIGN.md §8.
    p[param::PLATE_DECAY as usize] = 0.74;
    p[param::PLATE_DAMPING as usize] = 0.34;
    p[param::PLATE_MIX as usize] = 0.30;
    p[param::CHORUS_DEPTH as usize] = 0.40;
    p[param::CHORUS_RATE as usize] = 0.35;
    p[param::CHORUS_MIX as usize] = 0.30;
    p[param::DENSITY_ON as usize] = 0.0; // off until it earns its keep
    p[param::DENSITY_AMOUNT as usize] = 0.55;
    p[param::MASTER as usize] = 0.8;
    p
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self {
        let params = defaults();
        let entry = params[param::ENTRY as usize] as usize;
        let (a, v) = ROSTER[entry.min(ROSTER.len() - 1)];
        let vp = VoiceParams { algorithm: a, variant: v, index: params[param::INDEX as usize], free_ratio: 1.0 };
        let tuning = tuning_for(a, v, params[param::INDEX as usize] as f64, DEGREES_PER_SCALE);
        let mirror = Mirror::new(&tuning.cents());

        let mut e = Self {
            sample_rate,
            pool: Pool::new(sample_rate, vp),
            bus: Bus::new(sample_rate),
            entry,
            tuning,
            mirror,
            root_cents: leading::cents_of(110.0),
            step: 0,
            countdown: 0,
            time_in_chord: 0.0,
            pending: [Pending { hz: 0.0, at: 0.0, fired: true }; VOICES],
            pending_len: 0,
            sounding: [0.0; VOICES],
            sounding_len: 0,
            params,
            scope: Box::new([0.0; SCOPE_LEN]),
            scope_head: 0,
            out: Box::new([0.0; QUANTUM]),
        };
        e.apply();
        e
    }

    fn field_params(&self) -> FieldParams {
        FieldParams {
            floor: self.params[param::FLOOR as usize],
            depth: self.params[param::DEPTH as usize],
            curve: self.params[param::CURVE as usize],
            attack: self.params[param::ATTACK as usize],
        }
    }

    /// Push parameter changes into the engine. Control rate, never per sample.
    fn apply(&mut self) {
        let entry = (self.params[param::ENTRY as usize] as usize).min(ROSTER.len() - 1);
        let index = self.params[param::INDEX as usize];
        let (a, v) = ROSTER[entry];
        let vp = VoiceParams { algorithm: a, variant: v, index, free_ratio: 1.0 };

        if entry != self.entry {
            self.entry = entry;
            self.tuning = tuning_for(a, v, index as f64, DEGREES_PER_SCALE);
            self.mirror = Mirror::new(&self.tuning.cents());
        }
        self.pool.set_entry(a, v, vp);
        self.pool.field_params = self.field_params();
    }

    /// Begin the next chord. Runs at chord rate, so a little work is fine here
    /// and none of it is on the per-sample path.
    fn advance(&mut self) {
        self.step += 1;
        let mean = self.params[param::MEAN_INTERVAL as usize].max(0.05);
        let interval = word::interval_s(self.step, mean);
        self.pool.set_interval(interval);
        self.countdown = (interval * self.sample_rate) as usize;
        self.time_in_chord = 0.0;

        let cents: [f64; 8] = {
            let mut c = [0.0f64; 8];
            for (i, v) in self.tuning.cents().into_iter().filter(|&x| x < 1200.0).take(8).enumerate() {
                c[i] = v;
            }
            c
        };
        let k = self.tuning.cents().iter().filter(|&&x| x < 1200.0).count().max(1);

        // Which degrees sound. A chord tuning is voiced whole; a scale is
        // stacked, rooted where the golden rotation puts it.
        let mut targets = [0.0f64; VOICES];
        let mut n_targets = 0usize;
        if self.tuning.kind() == Kind::Chord {
            for i in 0..k.min(VOICES) {
                targets[n_targets] = cents[i];
                n_targets += 1;
            }
        } else {
            let r = (((self.step as f64) * word::INV_PHI).fract() * k as f64) as usize;
            for &s in &[0usize, 2, 4, 6] {
                if n_targets < VOICES {
                    targets[n_targets] = cents[(r + s) % k];
                    n_targets += 1;
                }
            }
        }

        // Mirror, on the golden schedule.
        let amount = self.params[param::MIRROR as usize] as f64;
        if self.tuning.kind() != Kind::Chord && mirror_this_chord(self.step, 0.0, amount) {
            let flipped = self.mirror.reflect_chord(&targets[..n_targets]);
            targets[..n_targets].copy_from_slice(&flipped);
        }

        // Realise against the home ladder so the pad cannot walk away.
        for i in 0..n_targets {
            let h = leading::home(i, self.root_cents);
            targets[i] = leading::realise(targets[i].rem_euclid(1200.0), self.root_cents, h, h);
        }
        targets[..n_targets].sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Release what has gone.
        for i in 0..self.sounding_len {
            let s = self.sounding[i];
            if !targets[..n_targets].iter().any(|t| (t - s).abs() < 5.0) {
                self.pool.note_off(leading::hz_of(s) as f32);
            }
        }

        // Schedule what arrives. A common tone is not in this list, so it is
        // never restruck.
        let bias = self.params[param::STRUM_BIAS as usize];
        let budget = strum::budget_s(interval);
        self.pending_len = 0;
        let mut arriving = [0.0f64; VOICES];
        let mut n_arriving = 0;
        for i in 0..n_targets {
            let t = targets[i];
            if !self.sounding[..self.sounding_len].iter().any(|s| (t - s).abs() < 5.0) {
                arriving[n_arriving] = t;
                n_arriving += 1;
            }
        }
        let offsets = strum::attack_offsets(n_arriving, bias, budget);
        for i in 0..n_arriving {
            self.pending[self.pending_len] = Pending {
                hz: leading::hz_of(arriving[i]) as f32,
                at: offsets[i],
                fired: false,
            };
            self.pending_len += 1;
        }

        self.sounding[..n_targets].copy_from_slice(&targets[..n_targets]);
        self.sounding_len = n_targets;
    }

    /// One sample. This is the hot path and it allocates nothing.
    #[inline]
    fn tick(&mut self) -> f32 {
        if self.countdown == 0 {
            self.advance();
        }
        self.countdown -= 1;

        let dt = 1.0 / self.sample_rate;
        self.time_in_chord += dt;
        for i in 0..self.pending_len {
            if !self.pending[i].fired && self.time_in_chord >= self.pending[i].at {
                self.pending[i].fired = true;
                let (hz, at) = (self.pending[i].hz, self.pending[i].at);
                self.pool.note_on(hz, at);
            }
        }

        let plate = PlateParams {
            decay: self.params[param::PLATE_DECAY as usize],
            damping: self.params[param::PLATE_DAMPING as usize],
            noise_mod: 0.35,
            mix: self.params[param::PLATE_MIX as usize],
        };
        let chorus = ChorusParams {
            depth: self.params[param::CHORUS_DEPTH as usize],
            rate: self.params[param::CHORUS_RATE as usize],
            mix: self.params[param::CHORUS_MIX as usize],
        };
        let density = DensityParams {
            enabled: self.params[param::DENSITY_ON as usize] > 0.5,
            amount: self.params[param::DENSITY_AMOUNT as usize],
            ..Default::default()
        };

        let dry = self.pool.tick();
        let wet = self.bus.process(dry, &plate, &chorus, &density);
        let out = wet * self.params[param::MASTER as usize];

        self.scope[self.scope_head] = out;
        self.scope_head = (self.scope_head + 1) & (SCOPE_LEN - 1);
        out
    }

    pub fn process(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = self.tick();
        }
    }

    pub fn set(&mut self, id: u32, value: f32) {
        if (id as usize) < self.params.len() {
            self.params[id as usize] = value;
            self.apply();
        }
    }

    pub fn get(&self, id: u32) -> f32 {
        self.params.get(id as usize).copied().unwrap_or(0.0)
    }

    /// Move along the golden rotation. §10's STEP, walking the roster.
    pub fn step_by(&mut self, delta: i32) {
        let n = ROSTER.len() as i32;
        let cur = self.params[param::ENTRY as usize] as i32;
        self.params[param::ENTRY as usize] = (cur + delta).rem_euclid(n) as f32;
        self.apply();
    }
}

// ── The C ABI ─────────────────────────────────────────────────────────────

/// # Safety
/// The returned pointer must be freed exactly once with [`phy_free`].
#[no_mangle]
pub extern "C" fn phy_new(sample_rate: f32) -> *mut Engine {
    Box::into_raw(Box::new(Engine::new(sample_rate)))
}

/// # Safety
/// `e` must come from [`phy_new`] and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn phy_free(e: *mut Engine) {
    if !e.is_null() {
        drop(Box::from_raw(e));
    }
}

/// Render `frames` samples into `out`.
///
/// # Safety
/// `out` must point to at least `frames` writable `f32`s.
#[no_mangle]
pub unsafe extern "C" fn phy_process(e: *mut Engine, out: *mut f32, frames: u32) {
    if e.is_null() || out.is_null() {
        return;
    }
    let engine = &mut *e;
    engine.process(core::slice::from_raw_parts_mut(out, frames as usize));
}

/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_set(e: *mut Engine, id: u32, value: f32) {
    if !e.is_null() {
        (*e).set(id, value);
    }
}

/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_get(e: *mut Engine, id: u32) -> f32 {
    if e.is_null() { 0.0 } else { (*e).get(id) }
}

/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_step(e: *mut Engine, delta: i32) {
    if !e.is_null() {
        (*e).step_by(delta);
    }
}

/// Render into the engine's own output block, then read it with
/// [`phy_out_ptr`].
///
/// `frames` is clamped to [`QUANTUM`]: the worklet's render quantum is fixed by
/// the platform, and a larger request would be a bug on the JS side rather than
/// something to grow a buffer for.
///
/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_render(e: *mut Engine, frames: u32) {
    if e.is_null() {
        return;
    }
    let engine = &mut *e;
    let n = (frames as usize).min(QUANTUM);
    for i in 0..n {
        engine.out[i] = engine.tick();
    }
}

/// # Safety
/// `e` must come from [`phy_new`]. Valid until `phy_free`.
#[no_mangle]
pub unsafe extern "C" fn phy_out_ptr(e: *mut Engine) -> *const f32 {
    if e.is_null() { core::ptr::null() } else { (*e).out.as_ptr() }
}

#[no_mangle]
pub extern "C" fn phy_quantum() -> u32 {
    QUANTUM as u32
}

/// Pointer to the scope ring, for the visualiser to read directly out of wasm
/// memory. Read-only from JS.
///
/// # Safety
/// `e` must come from [`phy_new`]. The pointer is valid until `phy_free`.
#[no_mangle]
pub unsafe extern "C" fn phy_scope_ptr(e: *mut Engine) -> *const f32 {
    if e.is_null() { core::ptr::null() } else { (*e).scope.as_ptr() }
}

#[no_mangle]
pub extern "C" fn phy_scope_len() -> u32 {
    SCOPE_LEN as u32
}

/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_scope_head(e: *mut Engine) -> u32 {
    if e.is_null() { 0 } else { (*e).scope_head as u32 }
}

#[no_mangle]
pub extern "C" fn phy_param_count() -> u32 {
    param::COUNT
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn it_makes_sound_and_never_goes_non_finite() {
        let mut e = Engine::new(SR);
        let mut buf = [0.0f32; QUANTUM];
        let mut energy = 0.0f64;
        for _ in 0..(SR as usize * 8 / QUANTUM) {
            e.process(&mut buf);
            assert!(buf.iter().all(|s| s.is_finite()));
            assert!(buf.iter().all(|s| s.abs() < 4.0));
            energy += buf.iter().map(|s| (s * s) as f64).sum::<f64>();
        }
        assert!(energy > 1.0, "eight seconds produced no sound");
    }

    /// Every roster entry has to run, and the chord tunings differ from the
    /// scale ones in behaviour rather than only in name.
    #[test]
    fn every_entry_runs() {
        for i in 0..ROSTER.len() {
            let mut e = Engine::new(SR);
            e.set(param::ENTRY, i as f32);
            let mut buf = [0.0f32; QUANTUM];
            for _ in 0..2000 {
                e.process(&mut buf);
                assert!(buf.iter().all(|s| s.is_finite()), "entry {i} produced a non-finite sample");
            }
        }
    }

    /// STEP walks the roster and comes back around — no dead ends, and
    /// negative steps work.
    #[test]
    fn step_walks_the_roster_both_ways() {
        let mut e = Engine::new(SR);
        let start = e.get(param::ENTRY);
        for _ in 0..ROSTER.len() {
            e.step_by(1);
        }
        assert_eq!(e.get(param::ENTRY), start, "a full lap did not return");

        // Stepping back off the start of the roster wraps to its end, which is
        // the property that makes STEP a walk you can always return along
        // rather than a control with edges.
        e.set(param::ENTRY, 0.0);
        e.step_by(-1);
        assert_eq!(e.get(param::ENTRY) as usize, ROSTER.len() - 1);
        e.step_by(1);
        assert_eq!(e.get(param::ENTRY) as usize, 0);
    }

    /// Same settings, same samples. §10's whole thesis depends on it.
    #[test]
    fn rendering_is_deterministic() {
        let run = || {
            let mut e = Engine::new(SR);
            let mut buf = [0.0f32; QUANTUM];
            let mut all = Vec::new();
            for _ in 0..500 {
                e.process(&mut buf);
                all.extend_from_slice(&buf);
            }
            all
        };
        assert_eq!(run(), run());
    }

    /// A parameter change must not click, since every control is live.
    #[test]
    fn parameter_changes_do_not_click() {
        let mut e = Engine::new(SR);
        let mut buf = [0.0f32; QUANTUM];
        for _ in 0..400 {
            e.process(&mut buf);
        }
        let mut worst: f32 = 0.0;
        for k in 0..200 {
            e.set(param::INDEX, 1.0 + (k as f32 % 11.0));
            let before = buf[QUANTUM - 1];
            e.process(&mut buf);
            worst = worst.max((buf[0] - before).abs());
        }
        assert!(worst < 0.5, "a parameter change stepped the output by {worst}");
    }

    #[test]
    fn the_scope_ring_fills() {
        let mut e = Engine::new(SR);
        let mut buf = [0.0f32; QUANTUM];
        for _ in 0..200 {
            e.process(&mut buf);
        }
        assert!(e.scope.iter().any(|&s| s != 0.0), "scope never filled");
        assert!(e.scope_head < SCOPE_LEN);
    }

    /// The ABI is a contract: these must not be reordered.
    /// The worklet path: render into the engine's own block, read it back.
    #[test]
    fn the_render_block_round_trips() {
        let e = phy_new(SR);
        unsafe {
            phy_render(e, QUANTUM as u32);
            let p = phy_out_ptr(e);
            assert!(!p.is_null());
            let out = core::slice::from_raw_parts(p, QUANTUM);
            assert!(out.iter().all(|s| s.is_finite()));
            // A larger request is clamped rather than overrunning.
            phy_render(e, 4096);
            assert!(core::slice::from_raw_parts(phy_out_ptr(e), QUANTUM).iter().all(|s| s.is_finite()));
            phy_free(e);
        }
        assert_eq!(phy_quantum(), QUANTUM as u32);
    }

    #[test]
    fn the_abi_is_stable() {
        assert_eq!(param::ENTRY, 0);
        assert_eq!(param::MASTER, 17);
        assert_eq!(phy_param_count(), 18);
        assert_eq!(phy_scope_len(), SCOPE_LEN as u32);
    }

    #[test]
    fn the_c_abi_round_trips() {
        let e = phy_new(SR);
        assert!(!e.is_null());
        unsafe {
            phy_set(e, param::MASTER, 0.5);
            assert_eq!(phy_get(e, param::MASTER), 0.5);
            let mut buf = [0.0f32; QUANTUM];
            phy_process(e, buf.as_mut_ptr(), QUANTUM as u32);
            assert!(buf.iter().all(|s| s.is_finite()));
            assert!(!phy_scope_ptr(e).is_null());
            phy_free(e);
        }
        // Null pointers must be survivable, since JS can pass one.
        unsafe {
            phy_process(core::ptr::null_mut(), core::ptr::null_mut(), 128);
            phy_set(core::ptr::null_mut(), 0, 1.0);
            phy_free(core::ptr::null_mut());
        }
    }
}
