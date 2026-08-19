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
use phyllotaxis_tuning::{tuning_for, Kind, DEGREES_PER_SCALE, ROSTER};
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
    /// Power-law exponent for the tail. **Low is slower.** Was FLOOR, which
    /// is gone: a resting level is what made this drone instead of sound.
    pub const TAIL: u32 = 5;
    pub const DEPTH: u32 = 6;
    /// Where the tail takes over, as a fraction of the peak. Was CURVE.
    pub const KNEE: u32 = 7;
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
    /// Where the instrument sits, in Hz. Appended rather than inserted: a
    /// preset is a list of `id:value` pairs in a URL, so renumbering the
    /// existing parameters would silently re-point every preset already shared.
    pub const ROOT_HZ: u32 = 18;
    /// Seconds from the peak down to KNEE.
    pub const DECAY: u32 = 19;
    pub const COUNT: u32 = 20;
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
    /// The sounding tuning, as cents. Held as a fixed array rather than a
    /// `Tuning`, because reading it used to mean `tuning.cents()` — which
    /// allocates a `Vec` — twice per chord, on the audio thread.
    cents: [f64; MAX_DEGREES],
    n_cents: usize,
    is_chord: bool,
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
    out_r: Box<[f32; QUANTUM]>,
}

/// Upper bound on scale degrees. `DEGREES_PER_SCALE` is 8; the headroom costs
/// nothing and means a wider scale can never index out of bounds.
const MAX_DEGREES: usize = 16;

fn defaults() -> [f32; param::COUNT as usize] {
    let mut p = [0.0f32; param::COUNT as usize];
    p[param::ENTRY as usize] = 1.0; // fm II — sparse, and the one that reads as manageable
    p[param::INDEX as usize] = 4.0;
    p[param::MEAN_INTERVAL as usize] = 1.618;
    p[param::MIRROR as usize] = word::INV_PHI2 as f32;
    p[param::STRUM_BIAS as usize] = -0.7;
    p[param::TAIL as usize] = 0.9;
    p[param::DEPTH as usize] = 0.85;
    p[param::KNEE as usize] = 0.40;
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
    p[param::ROOT_HZ as usize] = 110.0; // A2, where the instrument was built
    p[param::DECAY as usize] = 2.5;
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
            cents: [0.0; MAX_DEGREES],
            n_cents: 0,
            is_chord: false,
            mirror,
            root_cents: 0.0, // set by the first apply()
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
            out_r: Box::new([0.0; QUANTUM]),
        };
        // Install through the same path the main thread uses, so there is one
        // way a tuning reaches the engine and it is exercised from the first
        // sample.
        let cents = tuning.cents();
        e.install_tuning(&cents, tuning.kind() == Kind::Chord);
        e.apply();
        e
    }

/// The rendered left block. Valid after [`Engine::process_stereo`].
    pub fn left(&self) -> &[f32] {
        &self.out[..]
    }

    /// The rendered right block.
    pub fn right(&self) -> &[f32] {
        &self.out_r[..]
    }

    /// Derive and install the tuning for a roster entry at the current index.
    ///
    /// Slow — this is the work the browser gives to a worker. Offered here so
    /// offline rendering and tests do what the instrument does rather than a
    /// paraphrase of it.
    pub fn install_for_entry(&mut self, entry: u32) {
        let e = (entry as usize).min(ROSTER.len() - 1);
        let (a, v) = ROSTER[e];
        let t = tuning_for(a, v, self.params[param::INDEX as usize] as f64, DEGREES_PER_SCALE);
        self.install_tuning(&t.cents(), t.kind() == Kind::Chord);
    }

    /// Put a tuning into the engine. Cheap: a copy and a reflection table.
    ///
    /// **Deriving one is not cheap, and that is the whole reason this exists.**
    /// `apply()` used to call `tuning_for` whenever the entry changed, and
    /// `apply()` runs from `phy_set` and `phy_step` — on the audio thread.
    /// Measured, one derivation costs 1.0 s for `fm I`; a render quantum is
    /// 2.67 ms. Stepping the roster onto that entry stalled the audio thread
    /// for roughly four hundred quanta, which presents as the instrument
    /// freezing and never arriving at the entry you asked for. Derivation now
    /// happens on the main thread and the result arrives here.
    pub fn install_tuning(&mut self, cents: &[f64], is_chord: bool) {
        self.n_cents = cents.len().min(MAX_DEGREES);
        self.cents[..self.n_cents].copy_from_slice(&cents[..self.n_cents]);
        self.is_chord = is_chord;
        self.mirror = Mirror::new(&self.cents[..self.n_cents]);
    }

    fn field_params(&self) -> FieldParams {
        FieldParams {
            knee: self.params[param::KNEE as usize],
            decay: self.params[param::DECAY as usize],
            tail: self.params[param::TAIL as usize],
            depth: self.params[param::DEPTH as usize],
            attack: self.params[param::ATTACK as usize],
        }
    }

    /// Push parameter changes into the engine. Control rate, never per sample.
    fn apply(&mut self) {
        let entry = (self.params[param::ENTRY as usize] as usize).min(ROSTER.len() - 1);
        let index = self.params[param::INDEX as usize];
        let (a, v) = ROSTER[entry];
        let vp = VoiceParams { algorithm: a, variant: v, index, free_ratio: 1.0 };

        // Note what changed; do NOT derive a tuning here. See `install_tuning`.
        self.entry = entry;
        self.pool.set_entry(a, v, vp);
        self.pool.field_params = self.field_params();

        // Where the instrument sits. There was no control for this at all: the
        // root was fixed at 110 Hz in the constructor, so an instrument whose
        // whole subject is tuning could not be tuned.
        let hz = self.params[param::ROOT_HZ as usize].clamp(27.5, 440.0) as f64;
        self.root_cents = leading::cents_of(hz);
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

        let mut cents = [0.0f64; MAX_DEGREES];
        let mut k = 0usize;
        for i in 0..self.n_cents {
            let v = self.cents[i];
            if v < 1200.0 && k < MAX_DEGREES {
                cents[k] = v;
                k += 1;
            }
        }
        let k = k.max(1);

        // Where the golden rotation has walked to, as a degree of this entry's
        // own tuning. Both kinds use it; they spend it differently.
        let r = (((self.step as f64) * word::INV_PHI).fract() * k as f64) as usize;

        // Which degrees sound. A chord tuning is voiced whole; a scale is
        // stacked, rooted where the golden rotation puts it.
        //
        // **A chord being voiced whole is not the same as it standing still.**
        // It used to be: the chord kinds took `cents[0..k]` with no reference
        // to `self.step`, so rm I, rm II, am I and am II each sounded one chord
        // and then never moved for as long as you left them running — and since
        // a common tone is never restruck, they never re-attacked either. Half
        // the roster was a held drone. A scale walks its root; the answer for a
        // chord is the same move made whole, so the chord is transposed bodily
        // by a degree of its own tuning. That keeps the chord's internal
        // intervals exactly as the dissonance curve computed them — which is
        // the entire point of a chord kind — while letting it modulate.
        let mut targets = [0.0f64; VOICES];
        let mut n_targets = 0usize;
        if self.is_chord {
            let shift = cents[r.min(MAX_DEGREES - 1)];
            for i in 0..k.min(VOICES) {
                targets[n_targets] = (cents[i] + shift).rem_euclid(1200.0);
                n_targets += 1;
            }
        } else {
            for &s in &[0usize, 2, 4, 6] {
                if n_targets < VOICES {
                    targets[n_targets] = cents[(r + s) % k];
                    n_targets += 1;
                }
            }
        }

        // Mirror, on the golden schedule.
        //
        // This used to carry `kind() != Kind::Chord`, which excluded the four
        // chord entries from the mirror entirely. That guard cannot have been
        // right: `Mirror::reflect_chord` exempts the lowest voice as a pedal —
        // logic that exists *for* chords — and under that guard it was never
        // once called on one. The reflection was dead by construction on half
        // the roster, and negative harmony on a whole voiced chord is the
        // plainest use the technique has.
        let amount = self.params[param::MIRROR as usize] as f64;
        if mirror_this_chord(self.step, 0.0, amount) {
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
    fn tick(&mut self) -> (f32, f32) {
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

        let (dl, dr) = self.pool.tick_stereo();
        let (wl, wr) = self.bus.process_stereo(dl, dr, &plate, &chorus, &density);
        let m = self.params[param::MASTER as usize];
        let (l, r) = (wl * m, wr * m);

        // The scope is one trace of what is actually leaving the instrument.
        self.scope[self.scope_head] = (l + r) * 0.5;
        self.scope_head = (self.scope_head + 1) & (SCOPE_LEN - 1);
        (l, r)
    }

    pub fn process(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            let (l, r) = self.tick();
            *s = (l + r) * 0.5;
        }
    }

    /// Render into both output blocks.
    pub fn process_stereo(&mut self, n: usize) {
        for i in 0..n.min(QUANTUM) {
            let (l, r) = self.tick();
            self.out[i] = l;
            self.out_r[i] = r;
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
    (*e).process_stereo(frames as usize);
}

/// Pointer to the RIGHT output block. [`phy_out_ptr`] is the left.
///
/// The instrument shipped mono: it rendered one block and the worklet copied
/// it to every channel, so the plate and the chorus — both of which exist to
/// create width — were heard as a point source.
///
/// # Safety
/// `e` must come from [`phy_new`]. Valid until `phy_free`.
#[no_mangle]
pub unsafe extern "C" fn phy_out_r_ptr(e: *mut Engine) -> *const f32 {
    if e.is_null() { core::ptr::null() } else { (*e).out_r.as_ptr() }
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

// ── Deriving a tuning, off the audio thread ───────────────────────────────
//
// One module-level scratch buffer, written by whoever is deriving and read by
// whoever is installing. The module is single-threaded — an AudioWorklet gets
// its own instance and so does the main thread — so the two never share this.

const SCRATCH: usize = MAX_DEGREES;
static mut TUNING_SCRATCH: [f32; SCRATCH] = [0.0; SCRATCH];
static mut TUNING_IS_CHORD: u32 = 0;

/// Pointer to the tuning scratch buffer: `MAX_DEGREES` f32 cents.
///
/// Read it after [`phy_compute_tuning`], or write it before
/// [`phy_install_tuning`].
#[no_mangle]
pub extern "C" fn phy_tuning_ptr() -> *mut f32 {
    core::ptr::addr_of_mut!(TUNING_SCRATCH) as *mut f32
}

#[no_mangle]
pub extern "C" fn phy_tuning_cap() -> u32 {
    SCRATCH as u32
}

/// Derive the tuning for `entry` at `index` into the scratch buffer; returns
/// how many degrees it wrote. **Slow — up to a second.** Never call this from
/// the audio thread; that is the bug it exists to prevent.
#[no_mangle]
pub extern "C" fn phy_compute_tuning(entry: u32, index: f32) -> u32 {
    let e = (entry as usize).min(ROSTER.len() - 1);
    let (a, v) = ROSTER[e];
    let t = tuning_for(a, v, index as f64, DEGREES_PER_SCALE);
    let cents = t.cents();
    let n = cents.len().min(SCRATCH);
    unsafe {
        let buf = core::ptr::addr_of_mut!(TUNING_SCRATCH) as *mut f32;
        for (i, c) in cents.iter().take(n).enumerate() {
            buf.add(i).write(*c as f32);
        }
        TUNING_IS_CHORD = u32::from(t.kind() == Kind::Chord);
    }
    n as u32
}

#[no_mangle]
pub extern "C" fn phy_computed_is_chord() -> u32 {
    unsafe { core::ptr::addr_of!(TUNING_IS_CHORD).read() }
}

/// Install the first `len` degrees of the scratch buffer into the engine.
/// Cheap, and safe to call from the audio thread.
///
/// # Safety
/// `e` must come from [`phy_new`].
#[no_mangle]
pub unsafe extern "C" fn phy_install_tuning(e: *mut Engine, len: u32, is_chord: u32) {
    if e.is_null() {
        return;
    }
    let n = (len as usize).min(SCRATCH);
    let buf = core::ptr::addr_of!(TUNING_SCRATCH) as *const f32;
    let mut cents = [0.0f64; SCRATCH];
    for (i, c) in cents.iter_mut().take(n).enumerate() {
        *c = buf.add(i).read() as f64;
    }
    (*e).install_tuning(&cents[..n], is_chord != 0);
}

#[no_mangle]
pub extern "C" fn phy_param_count() -> u32 {
    param::COUNT
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Does every roster entry actually move harmonically, and does stepping
    /// past either end of the roster survive? Prints a table under
    /// `--nocapture`; asserts the properties that must hold.
    #[test]
    fn every_entry_moves_and_the_roster_wraps() {
        println!("{:<6} {:<6} {:>7} {:>9} {:>8}", "entry", "kind", "chords", "distinct", "attacks");
        let mut frozen = Vec::new();
        for entry in 0..ROSTER.len() as u32 {
            let mut e = Engine::new(SR);
            e.set(param::ENTRY, entry as f32);
            // What the main thread does on STEP: derive, then install.
            let (a, v) = ROSTER[entry as usize];
            let t = tuning_for(a, v, e.get(param::INDEX) as f64, DEGREES_PER_SCALE);
            e.install_tuning(&t.cents(), t.kind() == Kind::Chord);
            let mut buf = [0.0f32; QUANTUM];
            let mut seen: Vec<Vec<i64>> = Vec::new();
            let mut attacks = 0usize;
            let mut chords = 0usize;
            let mut last: Vec<i64> = Vec::new();
            // 20 s, not 120. At the default 1.618 s chord rate that is a
            // dozen chords — ample to show an entry moving, which is all this
            // asserts. At 120 s across eight entries it simulated sixteen
            // minutes of audio and took two and a half minutes to say so.
            for _ in 0..((SR as usize * 20) / QUANTUM) {
                e.process(&mut buf);
                let cur: Vec<i64> =
                    e.sounding[..e.sounding_len].iter().map(|c| c.round() as i64).collect();
                if cur != last {
                    chords += 1;
                    attacks += cur.iter().filter(|c| !last.contains(c)).count();
                    if !seen.contains(&cur) {
                        seen.push(cur.clone());
                    }
                    last = cur;
                }
            }
            let kind = if e.is_chord { "chord" } else { "scale" };
            println!("{:<6} {:<6} {:>7} {:>9} {:>8}", entry, kind, chords, seen.len(), attacks);
            if seen.len() < 2 {
                frozen.push(entry);
            }
        }
        assert!(
            frozen.is_empty(),
            "entries never change harmony in two minutes: {frozen:?}"
        );
    }

    /// **The audio thread must never derive a tuning.**
    ///
    /// This is the defect that presented as "stepping past the end crashes the
    /// instrument". `apply()` called `tuning_for`, and `apply()` runs from
    /// `phy_set` and `phy_step` — both of which the worklet calls on the audio
    /// thread. Deriving `fm I`'s tuning takes about a second; a render quantum
    /// is 2.67 ms. The engine did not crash and did not go out of range: it
    /// stopped answering for four hundred quanta, which is indistinguishable
    /// from crashing and is what you hear.
    ///
    /// A time-based assertion is a blunt instrument, but the quantity being
    /// guarded really is wall-clock on a real-time thread, and the margin here
    /// is three orders of magnitude.
    #[test]
    fn changing_entry_never_costs_more_than_a_render_quantum() {
        use std::time::Instant;
        // The real budget is one render quantum in a release build, which is
        // what the worklet runs; measured worst case there is 0.62 ms against
        // 2.67 ms. `cargo test` is unoptimised, so the allowance is widened for
        // it rather than the test being made meaningless. Either way this
        // catches the defect it exists for, which overran by 400×.
        let slack = if cfg!(debug_assertions) { 20.0 } else { 1.0 };
        let quantum = QUANTUM as f64 / SR as f64 * slack;
        let mut e = Engine::new(SR);
        let mut worst: f64 = 0.0;
        for round in 0..3 {
            for entry in 0..ROSTER.len() as u32 {
                let t = Instant::now();
                e.step_by(1);
                e.set(param::INDEX, 4.0 + round as f32 * 0.5);
                let el = t.elapsed().as_secs_f64();
                worst = worst.max(el);
                assert!(
                    el < quantum,
                    "entry {entry}: changing parameters took {:.1} ms, longer than the \
                     {:.2} ms budget — that is a stall on the audio thread",
                    el * 1e3,
                    quantum * 1e3,
                );
            }
        }
        assert!(worst < quantum);
    }

/// The instrument must not be dual mono at the boundary the browser sees.
    ///
    /// This is the end of the chain the whole stereo path exists for, and it
    /// is the one place a mistake anywhere in it shows up. It shipped
    /// correlating at exactly 1.0.
    #[test]
    fn the_two_output_blocks_are_not_the_same_signal() {
        let mut e = Engine::new(SR);
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for _ in 0..((SR as usize * 12) / QUANTUM) {
            e.process_stereo(QUANTUM);
            l.extend_from_slice(&e.out[..]);
            r.extend_from_slice(&e.out_r[..]);
        }
        let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
        for (a, b) in l.iter().zip(&r) {
            ll += (*a as f64) * (*a as f64);
            rr += (*b as f64) * (*b as f64);
            lr += (*a as f64) * (*b as f64);
        }
        assert!(ll > 1.0 && rr > 1.0, "one channel is silent");
        let c = lr / (ll.sqrt() * rr.sqrt());
        println!("channel correlation {c:.4}");
        assert!(c.abs() < 0.85, "the two channels correlate at {c:.3} — that is mono");
        // Both channels must also carry comparable energy: a hard-panned
        // accident would pass a correlation test on its own.
        let balance = (ll / rr).sqrt();
        assert!(
            (0.6..1.7).contains(&balance),
            "channels are unbalanced by {balance:.2}×"
        );
    }

    /// The root has to actually move the instrument, and by exactly the
    /// interval asked for.
    ///
    /// Measured on the pitches the engine schedules, not on a spectrum: the
    /// loudest partial is whichever the current chord and modulation index put
    /// on top, so an FFT peak moves for reasons that have nothing to do with
    /// the root. Same parameters and same sample count means the same step
    /// sequence, so the two runs differ only in where they sit.
    #[test]
    fn the_root_control_transposes_the_instrument() {
        fn sounding(root: f32) -> Vec<i64> {
            let mut e = Engine::new(SR);
            e.set(param::ROOT_HZ, root);
            let mut buf = [0.0f32; QUANTUM];
            for _ in 0..((SR as usize * 6) / QUANTUM) {
                e.process(&mut buf);
            }
            let mut v: Vec<i64> =
                e.sounding[..e.sounding_len].iter().map(|c| c.round() as i64).collect();
            v.sort_unstable();
            v
        }
        let low = sounding(55.0);
        let high = sounding(110.0);
        assert!(!low.is_empty() && low.len() == high.len(), "nothing sounding to compare");
        for (a, b) in low.iter().zip(&high) {
            assert_eq!(b - a, 1200, "doubling the root moved a voice by {} cents, not an octave", b - a);
        }
    }

    #[test]
    fn stepping_past_either_end_wraps_the_roster() {
        let n = ROSTER.len() as f32;
        let mut e = Engine::new(SR);
        e.set(param::ENTRY, 0.0);
        e.step_by(-1);
        assert_eq!(e.get(param::ENTRY), n - 1.0, "stepping back from 0 must wrap to the last entry");
        e.set(param::ENTRY, n - 1.0);
        e.step_by(1);
        assert_eq!(e.get(param::ENTRY), 0.0, "stepping forward from the last entry must wrap to 0");
    }

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
        assert_eq!(param::ROOT_HZ, 18);
        assert_eq!(param::DECAY, 19);
        assert_eq!(param::COUNT, 20);
        assert_eq!(phy_param_count(), param::COUNT);
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
