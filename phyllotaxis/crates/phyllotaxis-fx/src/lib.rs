//! The FX bus — where the *sequence* gets spent.
//!
//! The instrument spends Fibonacci **ratios** on tuning and Fibonacci
//! **integers** on its counts. The sequence itself pays off here, in two places
//! that both turn on the same theorem.
//!
//! **The plate.** Diffusion and delay lengths are consecutive Fibonacci sample
//! counts. Consecutive Fibonacci numbers are always coprime — `gcd(F(n),
//! F(n+1)) = 1`, and not by coincidence: the Fibonacci recursion is precisely
//! the worst case for the Euclidean algorithm. Coprime lengths are what a
//! diffusion network wants, because echoes that never coincide keep the tail
//! dense instead of letting it collapse into flutter.
//!
//! The general identity is `gcd(F(m), F(n)) = F(gcd(m, n))`, so non-adjacent
//! terms can share a factor — `F(12)=144` and `F(15)=610` share 2, since
//! `gcd(12,15)=3` and `F(3)=2`. Perfect pairwise coprimality across eight terms
//! is therefore not available from this sequence at all. What is available, and
//! what is asserted below, is that every *adjacent* pair is coprime and the
//! worst shared factor anywhere in the set is small.
//!
//! **The hyperchorus.** LFO rates are φ-spaced, therefore incommensurate,
//! therefore the modulation never re-aligns and the chorus never develops an
//! audible pulse. For sustained material that is the difference between a
//! chorus you can leave on and one you cannot.
//!
//! **The plate is a bus, not per-voice**, which is what makes predictive
//! allocation work: a stolen voice's send has already happened, so its energy
//! persists in the tail after the voice itself is gone.

pub mod density;

use core::f32::consts::TAU;

const PHI: f32 = 1.618_033_9;

/// Below this an `f32` is subnormal. WebAssembly has no flush-to-zero mode, so
/// every feedback path here flushes by hand or its tail costs an order of
/// magnitude more CPU than its head. This is the module where that matters
/// most: a reverb is nothing but feedback paths decaying toward zero.
#[inline(always)]
fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

// ── Why these lengths are Lucas AND Fibonacci, never four consecutive terms ──
//
// The original sets were four consecutive Fibonacci numbers each, chosen for
// coprimality. That was the wrong invariant, and it was wrong in the worst
// possible way: it selected for the exact property that breaks a feedback
// delay network.
//
// In an FDN every line feeds every other, so an echo's arrival time is a SUM
// of line lengths. Two hops through F(18) and F(19) land on the same sample as
// one hop through F(20) — because F(n) + F(n+1) = F(n+2) is the sequence's
// defining identity. Consecutive Fibonacci lengths guarantee that path sums
// collide, endlessly, at every order. The tail duly rang: measured
// autocorrelation peaked at 12× the median in the flutter band.
//
// Coprimality answers a different question — when two PERIODIC echo trains
// realign — and that question belongs to a pair of delays in series. It says
// nothing about path sums, and path sums are what a network is made of. The
// test that passed was measuring the constants, not the network.
//
// The fix keeps the instrument's premise and strengthens it. Lucas and
// Fibonacci are the two canonical integer sequences converging on φ, and
// interleaving them — L(n), L(n+1), F(n+4), F(n+5) — gives adjacent lengths a
// ratio of φ itself while breaking the additive identity that F alone cannot
// escape. Searched exhaustively over both sequences, the coincidence-free sets
// with the most even spacing are exactly the ones spaced by φ. The delays are
// more golden than they were before, not less.
//
// The invariant is checked by `no_small_path_sum_coincides`, which is the test
// coprimality should always have been.

/// Input diffuser lengths: L(8), L(9), F(12), F(13) at 48 kHz — 1.0 to 4.9 ms.
pub const DIFFUSER_LEN: [usize; 4] = [47, 76, 144, 233];

/// Second in-loop allpass bank — the pure Fibonacci half of the eight
/// available terms, F(14)…F(17), 7.9 to 33 ms.
///
/// One allpass per line is not enough diffusion for four lines. An allpass
/// passes its input straight through at `-g` alongside the delayed part, so a
/// single one leaves an undiffused fast path around the loop and you hear the
/// bare tank period — measured as a 0.35 autocorrelation spike at exactly
/// `TANK_LEN[0]`. Cascading a second one per line is the Dattorro arrangement
/// and is what actually raises echo density.
pub const LOOP_AP2_LEN: [usize; 4] = [377, 610, 987, 1597];

/// Allpass lengths **inside** the tank loop: L(12), L(13), F(16), F(17) —
/// 6.7 to 33 ms.
///
/// These are not decoration and their absence is a defect a test caught. With
/// diffusion only at the input, the tank is four comb filters in a ring: an
/// impulse recirculates unchanged and the tail rings at the loop period. The
/// measured short-lag autocorrelation was **0.98** — flutter, exactly the
/// failure coprime lengths are supposed to prevent, and coprimality cannot
/// prevent it because the problem is topological rather than arithmetic.
pub const LOOP_AP_LEN: [usize; 4] = [322, 521, 843, 1364];

/// Tank lengths: L(16), L(17), F(20), F(21) — 46 to 228 ms.
pub const TANK_LEN: [usize; 4] = [2207, 3571, 6765, 10946];

pub fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A fixed-length delay line. Allocation happens once, at construction.
#[derive(Clone, Debug)]
struct Delay {
    buf: Vec<f32>,
    idx: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0 }
    }
    #[inline]
    fn len(&self) -> usize {
        self.buf.len()
    }
    #[inline]
    fn read(&self) -> f32 {
        self.buf[self.idx]
    }
    /// Fractional read, for the modulated taps.
    #[inline]
    fn read_at(&self, back: f32) -> f32 {
        let n = self.buf.len() as f32;
        let pos = (self.idx as f32 - back).rem_euclid(n);
        let i = pos.floor() as usize % self.buf.len();
        let j = (i + 1) % self.buf.len();
        let f = pos - pos.floor();
        self.buf[i] * (1.0 - f) + self.buf[j] * f
    }
    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.idx] = flush(x);
        self.idx = (self.idx + 1) % self.buf.len();
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.idx = 0;
    }
}

/// A Schroeder allpass: diffuses without colouring.
#[derive(Clone, Debug)]
struct Allpass {
    d: Delay,
    g: f32,
}

impl Allpass {
    fn new(len: usize, g: f32) -> Self {
        Self { d: Delay::new(len), g }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.d.read();
        let v = x + delayed * self.g;
        self.d.write(v);
        flush(delayed - v * self.g)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlateParams {
    /// 0…1 — tail length.
    pub decay: f32,
    /// 0…1 — high-frequency loss per pass.
    pub damping: f32,
    /// 0…1 — how far the tank taps wander. Keeps a plate from sounding static.
    pub noise_mod: f32,
    /// 0…1 — wet level at the output.
    pub mix: f32,
}

impl Default for PlateParams {
    fn default() -> Self {
        Self { decay: 0.72, damping: 0.38, noise_mod: 0.3, mix: 0.28 }
    }
}

/// A plate on Fibonacci delays.
#[derive(Clone, Debug)]
pub struct Plate {
    sample_rate: f32,
    diffusers: Vec<Allpass>,
    loop_ap: Vec<Allpass>,
    loop_ap2: Vec<Allpass>,
    tank: Vec<Delay>,
    lowpass: [f32; 4],
    /// Modulation phases, accumulated as rate × dt.
    mod_phase: [f32; 4],
    mod_rate: [f32; 4],
}

impl Plate {
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 48_000.0;
        let len = |n: usize| ((n as f32 * scale).round() as usize).max(2);
        Self {
            sample_rate,
            diffusers: DIFFUSER_LEN.iter().map(|&n| Allpass::new(len(n), 0.62)).collect(),
            loop_ap: LOOP_AP_LEN.iter().map(|&n| Allpass::new(len(n), 0.5)).collect(),
            loop_ap2: LOOP_AP2_LEN.iter().map(|&n| Allpass::new(len(n), 0.55)).collect(),
            tank: TANK_LEN.iter().map(|&n| Delay::new(len(n) + 32)).collect(),
            lowpass: [0.0; 4],
            mod_phase: [0.0, 0.25, 0.5, 0.75],
            // φ-spaced, so no two taps wander together.
            mod_rate: core::array::from_fn(|i| 0.11 * PHI.powi(i as i32)),
        }
    }

    pub fn clear(&mut self) {
        self.diffusers.iter_mut().for_each(|a| a.d.clear());
        self.loop_ap.iter_mut().for_each(|a| a.d.clear());
        self.loop_ap2.iter_mut().for_each(|a| a.d.clear());
        self.tank.iter_mut().for_each(|d| d.clear());
        self.lowpass = [0.0; 4];
    }

    /// One sample in, one wet sample out.
    #[inline]
    /// Mono output takes ONE orthogonal row, and averaging the stereo pair
    /// would be a mistake rather than a shortcut: the two rows differ in the
    /// sign of `taps[1]` and `taps[3]`, so their mean cancels both and leaves
    /// a two-tap plate. Measured, that alone doubled the flutter.
    pub fn process(&mut self, x: f32, p: &PlateParams) -> f32 {
        self.process_stereo(x, p).0
    }

    /// One sample in, a stereo pair out.
    ///
    /// This is where a plate's width comes from, and it costs nothing extra:
    /// the four tank lines are already decorrelated, so reading them on two
    /// different orthogonal rows gives a genuine stereo pair rather than the
    /// same signal twice. `the_plate_is_not_dual_mono` measures the
    /// correlation between the two outputs.
    #[inline]
    pub fn process_stereo(&mut self, x: f32, p: &PlateParams) -> (f32, f32) {
        let dt = 1.0 / self.sample_rate;

        let mut s = x;
        for a in self.diffusers.iter_mut() {
            s = a.process(s);
        }

        let feedback = 0.35 + 0.62 * p.decay.clamp(0.0, 1.0);
        let damp = p.damping.clamp(0.0, 0.95);
        let wander = p.noise_mod.clamp(0.0, 1.0) * 18.0;

        let mut taps = [0.0f32; 4];
        for i in 0..4 {
            self.mod_phase[i] += self.mod_rate[i] * dt;
            if self.mod_phase[i] >= 1.0 {
                self.mod_phase[i] -= 1.0;
            }
            // Read near the FAR end of the line, not near the write head.
            //
            // `read_at(back)` counts back from the write index, so `read_at(16)`
            // is a sixteen-sample delay — 0.3 ms — whatever the buffer length
            // is. That is what this line used to say, which made the tank a
            // ~3 kHz resonator and the plate inaudible as a reverb, while every
            // coprimality test still passed: they check `TANK_LEN`, and nothing
            // checked that `TANK_LEN` reached the signal. The buffers are
            // allocated at `len + 32` for exactly this tap to sit inside, which
            // is the surviving evidence of what was meant.
            let offset = 16.0 + (self.mod_phase[i] * TAU).sin() * wander;
            taps[i] = self.tank[i].read_at(self.tank[i].len() as f32 - offset);
        }
        // Output on a DIFFERENT orthogonal row than the one fed back into line
        // 0. Summing all four with the same sign makes `wet` proportional to
        // `mix[0]` — the output is then literally the signal recirculating in
        // one line, and you hear that line's period.
        let wet_l = (taps[0] - taps[1] - taps[2] + taps[3]) * 0.25;
        let wet_r = (taps[0] + taps[1] - taps[2] - taps[3]) * 0.25;

        // A **Hadamard** mix, not a rotation.
        //
        // Feeding each line from its neighbour sends energy round a ring, and a
        // ring of combs is still combs: an impulse comes back recognisable and
        // the tail rings at the loop period. Measured autocorrelation with a
        // rotation was 0.53 even with allpasses inside the loop. An orthogonal
        // matrix scatters every line into every other on every pass, which is
        // what makes a feedback delay network dense rather than merely long —
        // and being orthogonal, it does it without changing the energy, so
        // stability still depends only on the decay coefficient.
        let mix = [
            (taps[0] + taps[1] + taps[2] + taps[3]) * 0.5,
            (taps[0] - taps[1] + taps[2] - taps[3]) * 0.5,
            (taps[0] + taps[1] - taps[2] - taps[3]) * 0.5,
            (taps[0] - taps[1] - taps[2] + taps[3]) * 0.5,
        ];

        for i in 0..4 {
            let from = mix[i];
            let damped = self.lowpass[i] + (1.0 - damp) * (from - self.lowpass[i]);
            self.lowpass[i] = flush(damped);
            // Diffuse INSIDE the loop. Without this the ring is a comb bank and
            // the tail rings at the loop period however coprime the delays are.
            let diffused = self.loop_ap2[i].process(self.loop_ap[i].process(damped));
            // Alternating injection signs. Writing the same `s` into all four
            // lines starts them perfectly correlated, and correlated content
            // through an orthogonal mix stays structured instead of scattering.
            let inject = if i % 2 == 0 { s * 0.5 } else { -s * 0.5 };
            self.tank[i].write(inject + diffused * feedback);
        }

        (flush(wet_l), flush(wet_r))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChorusParams {
    /// 0…1 — modulation excursion.
    pub depth: f32,
    /// 0…1 — overall rate scaling.
    pub rate: f32,
    /// 0…1 — wet level.
    pub mix: f32,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self { depth: 0.45, rate: 0.4, mix: 0.35 }
    }
}

/// Six voices on φ-spaced rates.
///
/// Six is not a Fibonacci number and does not need to be — it is a count of
/// modulators, not a structural count of the instrument. What matters is that
/// no two rates share a period, which φ-spacing guarantees.
#[derive(Clone, Debug)]
pub struct Hyperchorus {
    sample_rate: f32,
    line: Delay,
    phase: [f32; 6],
    rate: [f32; 6],
    base_delay: [f32; 6],
}

impl Hyperchorus {
    pub const VOICES: usize = 6;

    pub fn new(sample_rate: f32) -> Self {
        let max = (sample_rate * 0.05) as usize + 8;
        Self {
            sample_rate,
            line: Delay::new(max),
            phase: core::array::from_fn(|i| ((i as f32) * (PHI - 1.0)).fract()),
            rate: core::array::from_fn(|i| 0.083 * PHI.powi(i as i32)),
            base_delay: core::array::from_fn(|i| 0.006 + 0.004 * i as f32),
        }
    }

    pub fn clear(&mut self) {
        self.line.clear();
    }

    /// The φ-spaced rates in Hz, at a given rate setting.
    pub fn rates(&self, rate: f32) -> [f32; 6] {
        core::array::from_fn(|i| self.rate[i] * (0.25 + 1.75 * rate.clamp(0.0, 1.0)))
    }

    #[inline]
    pub fn process(&mut self, x: f32, p: &ChorusParams) -> f32 {
        let (l, r) = self.process_stereo(x, p);
        (l + r) * 0.5
    }

    /// One sample in, a stereo pair out.
    ///
    /// Each channel reads the same delay line through the same LFOs a quarter
    /// cycle apart. In quadrature the two sides are at their widest excursion
    /// when the other is at rest, so the pitch wobble never agrees across the
    /// image — which is the whole reason a chorus widens anything. Summed to
    /// one point it is a detune and nothing more.
    #[inline]
    pub fn process_stereo(&mut self, x: f32, p: &ChorusParams) -> (f32, f32) {
        let dt = 1.0 / self.sample_rate;
        self.line.write(x);
        let rates = self.rates(p.rate);
        let depth = p.depth.clamp(0.0, 1.0);

        let (mut wl, mut wr) = (0.0, 0.0);
        for i in 0..Self::VOICES {
            self.phase[i] += rates[i] * dt;
            if self.phase[i] >= 1.0 {
                self.phase[i] -= 1.0;
            }
            let sl = self.base_delay[i] * (1.0 + depth * 0.6 * (self.phase[i] * TAU).sin());
            let sr = self.base_delay[i] * (1.0 + depth * 0.6 * (self.phase[i] * TAU).cos());
            wl += self.line.read_at(sl * self.sample_rate);
            wr += self.line.read_at(sr * self.sample_rate);
        }
        let n = 1.0 / Self::VOICES as f32;
        (flush(wl * n), flush(wr * n))
    }
}

/// The whole bus.
pub struct Bus {
    pub plate: Plate,
    pub chorus: Hyperchorus,
    pub density: density::Density,
}

impl Bus {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            plate: Plate::new(sample_rate),
            chorus: Hyperchorus::new(sample_rate),
            density: density::Density::new(sample_rate),
        }
    }

    #[inline]
    pub fn process(
        &mut self,
        dry: f32,
        plate: &PlateParams,
        chorus: &ChorusParams,
        density: &density::DensityParams,
    ) -> f32 {
        // One row, not the mean of two — see `Plate::process`.
        self.process_stereo(dry, dry, plate, chorus, density).0
    }

    /// The bus, in stereo.
    ///
    /// The dry pair arrives already placed — the pool spreads its voices across
    /// the field — and the chorus and plate each widen it further from their
    /// own decorrelated outputs. The plate is fed the MONO sum of the chorused
    /// pair: a reverb send is a send, and feeding the tank two slightly
    /// different signals buys nothing a second orthogonal output row does not
    /// already give, while doubling the most expensive thing on the path.
    #[inline]
    pub fn process_stereo(
        &mut self,
        dry_l: f32,
        dry_r: f32,
        plate: &PlateParams,
        chorus: &ChorusParams,
        density: &density::DensityParams,
    ) -> (f32, f32) {
        let dry_mono = (dry_l + dry_r) * 0.5;
        let (cl, cr) = self.chorus.process_stereo(dry_mono, chorus);

        // **Equal-power, and the wet does not contain the dry.**
        //
        // This used to be `dry + (wet - dry) * mix`, a linear blend of two
        // signals that are near-copies of each other a few milliseconds apart.
        // That is a comb filter: at mix = 0.5 the delayed copy cancels the
        // direct one across a whole set of frequencies, and the level falls
        // out from under you as you turn the control up. "Chorus destroys
        // volume" is not a side effect of chorus, it is that formula.
        //
        // An equal-power crossfade holds the total constant instead, because
        // sin² + cos² = 1 — the level stays put and the control does only what
        // it says.
        let m = chorus.mix.clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
        let (gw, gd) = (m.sin(), m.cos());
        let chorused_l = dry_l * gd + cl * gw;
        let chorused_r = dry_r * gd + cr * gw;

        // The follower has to advance before the decay is read, so the two
        // agree about the same sample.
        let send = self.density.send_gain(dry_mono, density);
        let mut p = *plate;
        p.decay *= self.density.decay_scale(density);
        let (wl, wr) = self
            .plate
            .process_stereo((chorused_l + chorused_r) * 0.5 * send, &p);
        // Equal-power here too, and WET_MAKEUP because the tank is quiet.
        //
        // The plate "having barely any impact" was gain, not topology. The
        // input is scattered across four lines at 0.5, the output is a quarter
        // of an orthogonal row, and the whole thing is then added at `mix` on
        // top of a dry signal at unity — so at mix = 0.3 the reverb arrived
        // 11.6 dB under the source — measured, not estimated, by
        // `examples/gain.rs`. Turning it up did almost nothing because there
        // was almost nothing to turn up.
        //
        // 3.8x is unity at mix = 1. This is 6, so the control can go PAST
        // all-wet rather than stopping exactly at it: a control you are meant
        // to find a point on should overshoot the point.
        //
        // With makeup and an equal-power blend, mix = 1 is genuinely all
        // reverb and the control spans something worth having an opinion
        // about. Where it should sit is Billy's, not a constant of mine.
        const WET_MAKEUP: f32 = 6.0;
        let pm = plate.mix.clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
        let (pw, pd) = (pm.sin() * WET_MAKEUP, pm.cos());
        (chorused_l * pd + wl * pw, chorused_r * pd + wr * pw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Diagnostic: where exactly does the tail ring? Not an assertion.
    #[test]
    #[ignore]
    fn show_flutter_lags() {
        let mut plate = Plate::new(SR);
        let p = PlateParams { decay: 0.85, damping: 0.2, noise_mod: 0.0, mix: 1.0 };
        for i in 0..2000 { plate.process(if i < 64 { 1.0 } else { 0.0 }, &p); }
        let tail: Vec<f32> = (0..(SR as usize / 2)).map(|_| plate.process(0.0, &p)).collect();
        let energy: f32 = tail.iter().map(|s| s * s).sum();
        let lo = (SR as usize) * 5 / 1000;
        let hi = (SR as usize) * 250 / 1000;
        let mut v: Vec<(f32, usize)> = (lo..hi).step_by(7).map(|lag| {
            let c: f32 = tail[..tail.len()-lag].iter().zip(&tail[lag..]).map(|(a,b)| a*b).sum();
            ((c/energy).abs(), lag)
        }).collect();
        v.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap());
        println!("TANK_LEN    {TANK_LEN:?}");
        println!("LOOP_AP_LEN {LOOP_AP_LEN:?}");
        println!("top correlating lags:");
        for (c, lag) in v.iter().take(14) {
            println!("  lag {:6} ({:6.1} ms)  corr {:.3}", lag, *lag as f32 / SR * 1000.0, c);
        }
    }

    /// **The tank length has to reach the signal.**
    ///
    /// Every other test in this file checks `TANK_LEN` — that the numbers are
    /// coprime, that their coincidence period is long. None of them checked
    /// that those numbers were the delay anyone hears, and for a while they
    /// were not: the taps read sixteen samples behind the write head, so the
    /// tank was 0.3 ms regardless of how carefully the lengths were chosen.
    /// The reverb was inaudible and the whole test suite was green.
    ///
    /// So: send one impulse in and find the first echo. It must arrive no
    /// earlier than the shortest tank line, not in the first millisecond.
    #[test]
    fn the_first_echo_arrives_at_the_tank_length() {
        let mut plate = Plate::new(SR);
        let p = PlateParams { decay: 0.8, damping: 0.2, noise_mod: 0.0, mix: 1.0 };

        let mut tail = vec![0.0f32; SR as usize / 2];
        for (i, out) in tail.iter_mut().enumerate() {
            *out = plate.process(if i == 0 { 1.0 } else { 0.0 }, &p);
        }

        // The diffusers alone smear the input over their own lengths, so look
        // past them: the first tank echo is the first peak after that.
        let diffuse_span: usize = DIFFUSER_LEN.iter().sum::<usize>() + LOOP_AP_LEN[0];
        let shortest_tank = TANK_LEN[0];
        assert!(
            diffuse_span < shortest_tank,
            "the diffusers reach past the tank; this test cannot separate them"
        );

        let peak = tail[diffuse_span..]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(i, _)| i + diffuse_span)
            .unwrap();

        assert!(
            peak >= shortest_tank,
            "the loudest echo lands at {peak} samples ({:.2} ms) — before the \
             shortest tank line at {shortest_tank} ({:.1} ms). The tank length \
             is not in the signal path.",
            peak as f32 / SR * 1000.0,
            shortest_tank as f32 / SR * 1000.0,
        );
    }

    /// A plate is a plate because the tail outlasts the input by a long way.
    /// With the tap inverted this measured about 3 ms.
    #[test]
    fn the_tail_lasts_a_plausible_reverb_time() {
        let mut plate = Plate::new(SR);
        let p = PlateParams { decay: 0.74, damping: 0.34, noise_mod: 0.35, mix: 1.0 };

        let n = SR as usize * 4;
        let mut tail = vec![0.0f32; n];
        for (i, out) in tail.iter_mut().enumerate() {
            *out = plate.process(if i == 0 { 1.0 } else { 0.0 }, &p);
        }

        // Energy in 10 ms windows; find where it falls 60 dB below the peak.
        let win = SR as usize / 100;
        let energy: Vec<f32> = tail
            .chunks(win)
            .map(|c| c.iter().map(|s| s * s).sum::<f32>() / win as f32)
            .collect();
        let peak = energy.iter().cloned().fold(0.0f32, f32::max);
        let floor = peak * 1e-6; // -60 dB in power
        let rt60 = energy.iter().rposition(|&e| e > floor).unwrap_or(0);
        let secs = (rt60 * win) as f32 / SR;

        assert!(
            (0.3..8.0).contains(&secs),
            "RT60 of {secs:.3}s is not a reverb tail"
        );
    }

/// Correlation between two channels: 1.0 is the same signal twice.
    fn correlation(l: &[f32], r: &[f32]) -> f32 {
        let (mut ll, mut rr, mut lr) = (0.0f64, 0.0f64, 0.0f64);
        for (a, b) in l.iter().zip(r) {
            ll += (*a as f64) * (*a as f64);
            rr += (*b as f64) * (*b as f64);
            lr += (*a as f64) * (*b as f64);
        }
        if ll <= 0.0 || rr <= 0.0 {
            return 1.0;
        }
        (lr / (ll.sqrt() * rr.sqrt())) as f32
    }

    /// **The instrument shipped in mono.** Not by decision — the engine
    /// returned one sample and the worklet copied it to both channels, so a
    /// plate and a chorus, the two effects whose entire purpose is width, were
    /// summed to a point.
    ///
    /// Nothing caught it because nothing looked. Every FX test measured one
    /// channel, which is exactly as true of a mono plate as a stereo one. So
    /// this measures the thing that was wrong: how alike the two outputs are.
    #[test]
    fn the_plate_is_not_dual_mono() {
        let mut plate = Plate::new(SR);
        let p = PlateParams { decay: 0.8, damping: 0.3, noise_mod: 0.35, mix: 1.0 };
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for i in 0..(SR as usize * 2) {
            let x = if i < 128 { 1.0 } else { 0.0 };
            let (a, b) = plate.process_stereo(x, &p);
            l.push(a);
            r.push(b);
        }
        let c = correlation(&l, &r);
        assert!(
            c.abs() < 0.6,
            "the plate's two outputs correlate at {c:.3} — that is barely stereo"
        );
    }

    #[test]
    fn the_chorus_is_not_dual_mono() {
        let mut ch = Hyperchorus::new(SR);
        let p = ChorusParams { depth: 0.5, rate: 0.4, mix: 1.0 };
        let (mut l, mut r) = (Vec::new(), Vec::new());
        // Noise in, so the measurement is of the effect and not of the source.
        let mut z = 12345u32;
        for _ in 0..(SR as usize * 4) {
            z = z.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (z >> 8) as f32 / 8388608.0 - 1.0;
            let (a, b) = ch.process_stereo(x, &p);
            l.push(a);
            r.push(b);
        }
        let c = correlation(&l, &r);
        assert!(c.abs() < 0.9, "the chorus's two outputs correlate at {c:.3}");
    }

    /// **The invariant coprimality should always have been.**
    ///
    /// This test replaces `adjacent_delay_lengths_are_coprime`, which asserted
    /// `gcd(len[i], len[i+1]) == 1` on the grounds that consecutive Fibonacci
    /// numbers are coprime "so no two echoes ever coincide". That reasoning is
    /// sound for two delays in series and worthless for a feedback network,
    /// and following it selected the one family of lengths guaranteed to fail.
    ///
    /// In an FDN every line feeds every other, so an echo's arrival time is a
    /// SUM of line lengths. `F(n) + F(n+1) = F(n+2)` is Fibonacci's defining
    /// identity, so with four consecutive terms a two-hop path lands exactly on
    /// a one-hop path — at every order, forever. The old test passed on those
    /// lengths. The tail rang at 12× the median in the flutter band.
    ///
    /// So: check the thing that actually has to hold. No small combination of
    /// line lengths may come out near zero, because each such combination is
    /// two different paths through the network arriving on the same sample.
    #[test]
    fn no_small_path_sum_coincides() {
        // Two echoes closer together than this are heard as one, so a
        // coincidence inside it is a real concentration of energy.
        const TOL: i64 = 24;

        // Scoped to the TANK, and the scope is the point.
        //
        // A tank line's length IS an echo's arrival time, so a vanishing
        // combination of them is two paths landing on one sample. The
        // allpasses are a different object: they have unity magnitude
        // response and exist to smear phase, so "when do two echoes coincide"
        // is not a question they answer — the same argument
        // `no_two_echoes_in_a_loop_coincide_within_a_minute` already makes.
        // Applying this test to the 47-sample diffusers would demand a 24-sample
        // separation between combinations of 47 and 76, which is arithmetic
        // nobody can hear and would drive the diffusers out of their range.
        //
        // What guards the allpasses is `the_tail_does_not_flutter`, which
        // measures the finished tail instead of arguing about its parts.
        for (name, set) in [("TANK_LEN", &TANK_LEN)] {
            let l: Vec<i64> = set.iter().map(|&n| n as i64).collect();
            for a in -2i64..=2 {
                for b in -2i64..=2 {
                    for c in -2i64..=2 {
                        for d in -2i64..=2 {
                            let coeffs = [a, b, c, d];
                            let weight: i64 = coeffs.iter().map(|c| c.abs()).sum();
                            if weight == 0 || weight > 3 {
                                continue;
                            }
                            let v: i64 = coeffs.iter().zip(&l).map(|(c, n)| c * n).sum();
                            assert!(
                                v.abs() >= TOL,
                                "{name}: {a}·{} + {b}·{} + {c}·{} + {d}·{} = {v} — two \
                                 paths through the network land on the same sample",
                                l[0], l[1], l[2], l[3],
                            );
                        }
                    }
                }
            }
        }
    }

    /// What actually has to hold: **no two echoes in the same loop coincide
    /// within any span anyone hears.**
    ///
    /// Coprimality was a means to that and is not achievable as an end. Four
    /// consecutive Fibonacci terms `F(n..n+3)` are all-pairs coprime only when
    /// `3 ∤ n`, because `gcd(F(n), F(n+3)) = F(gcd(n, 3))` — the tank at
    /// `F(18)…F(21)` shares a factor of 2, and a test asserting `gcd == 1`
    /// duly failed on it.
    ///
    /// It applies to the **tank** and nowhere else. The input diffusers are
    /// allpasses in series: they smear phase rather than emitting discrete
    /// echoes, so "coincidence" is not a property they have — and being short,
    /// 55 and 89 have an lcm of a tenth of a second while being perfectly
    /// coprime. Scoping this to the feedback loop is not a convenience; the
    /// quantity is meaningless outside it.
    ///
    /// The quantity that matters is the *coincidence period*, `lcm(a, b)`,
    /// and for that pair it is 14.1 million samples — **294 seconds**. Two
    /// delays sharing a factor of 2 is not a defect; a test demanding they not
    /// is stricter than the physics and would have driven the tank up into
    /// 369 ms delays to satisfy arithmetic nobody can hear.
    #[test]
    fn no_two_echoes_in_a_loop_coincide_within_a_minute() {
        const SR_USIZE: usize = 48_000;
        {
            let set = &TANK_LEN;
            for i in 0..set.len() {
                for j in (i + 1)..set.len() {
                    let (a, b) = (set[i], set[j]);
                    let lcm = a / gcd(a, b) * b;
                    let secs = lcm as f32 / SR_USIZE as f32;
                    assert!(
                        secs > 60.0,
                        "{a} and {b} coincide every {secs:.1}s (gcd {})",
                        gcd(a, b)
                    );
                }
            }
        }
    }

    /// Across networks it is not achievable and does not need to be.
    ///
    /// `gcd(F(m), F(n)) = F(gcd(m, n))`, so any eleven consecutive terms
    /// contain a pair sharing a large factor — here F(10)=55 and F(20)=6765
    /// share 55. Those two never meet in a feedback path: one is an input
    /// diffuser, the other a tank line. Recorded rather than asserted away, so
    /// nobody later "fixes" the lengths to chase a property that was never
    /// required.
    #[test]
    fn cross_network_sharing_is_reported_not_required() {
        let all: Vec<usize> = DIFFUSER_LEN.iter().chain(LOOP_AP_LEN.iter()).chain(TANK_LEN.iter()).copied().collect();
        let mut worst = 1;
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                worst = worst.max(gcd(all[i], all[j]));
            }
        }
        eprintln!("worst gcd across the whole network: {worst}");
        // No length may be REUSED. A shared factor across networks is expected;
        // the same number appearing twice is not, and this test found exactly
        // that — F(17) was in both the loop allpasses and the tank, reporting a
        // cross-network gcd of 1597, which is the length itself.
        let mut sorted = all.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "a delay length is used in two places");
        assert!(worst > 1, "if this ever becomes 1, the note above is obsolete");
    }

    /// φ-spaced rates are incommensurate, so the chorus does not develop an
    /// audible pulse.
    ///
    /// Two false starts are worth recording, because both looked like tests.
    /// The first asked whether each rate ratio was near a rational `p/q` with
    /// `q ≤ 16` — which measures nothing, since with `p` unbounded every real is
    /// close to some such fraction (`φ⁵ ≈ 122/11` to four places). The second
    /// simulated the phases but counted `t = 0`, where they are aligned by
    /// definition, and duly reported an alignment distance of 0.0046.
    ///
    /// The property that matters is neither "irrational" nor "never aligns" —
    /// six irrational rotations come arbitrarily close eventually, by Weyl
    /// equidistribution. It is that the **first** re-alignment falls outside any
    /// span a pad is held for. It does: 78 seconds.
    #[test]
    fn the_chorus_does_not_pulse() {
        let c = Hyperchorus::new(SR);
        let rates = c.rates(1.0);
        let start: [f32; 6] = core::array::from_fn(|i| ((i as f32) * (PHI - 1.0)).fract());

        let aligned_at = |t: f32| {
            let mut dist = 0.0;
            for i in 0..6 {
                let ph = (start[i] + rates[i] * t).fract();
                let d = (ph - start[i]).abs();
                dist += d.min(1.0 - d);
            }
            dist
        };

        // Skip the opening second: at t = 0 they are aligned by construction.
        let mut first = None;
        let mut t = 1.0f32;
        while t < 240.0 {
            if aligned_at(t) < 0.05 {
                first = Some(t);
                break;
            }
            t += 0.001;
        }
        match first {
            None => eprintln!("no re-alignment within four minutes"),
            Some(t) => {
                assert!(t > 30.0, "the chorus re-aligns after only {t:.1}s");
                eprintln!("first re-alignment at {t:.1}s");
            }
        }
    }

    /// The tail must actually reach zero, not sit at a subnormal floor burning
    /// CPU — the WebAssembly failure this module is written around.
    #[test]
    fn the_tail_flushes_to_exact_zero() {
        let mut plate = Plate::new(SR);
        let p = PlateParams { decay: 0.5, ..Default::default() };
        for _ in 0..(SR as usize / 10) {
            plate.process(1.0, &p);
        }
        let mut last = 1.0;
        for _ in 0..(SR as usize * 90) {
            last = plate.process(0.0, &p);
        }
        assert_eq!(last, 0.0, "tail settled at {last:e} rather than zero");
    }

    /// A reverb that is not unconditionally stable is a bug waiting for a
    /// setting. Every decay value must decay.
    #[test]
    fn the_plate_is_stable_at_every_setting() {
        for &decay in &[0.0f32, 0.5, 0.9, 1.0] {
            for &damping in &[0.0f32, 0.5, 0.95] {
                let mut plate = Plate::new(SR);
                let p = PlateParams { decay, damping, ..Default::default() };
                let mut peak_early = 0.0f32;
                for i in 0..(SR as usize * 2) {
                    let x = if i < 1000 { 1.0 } else { 0.0 };
                    let y = plate.process(x, &p);
                    assert!(y.is_finite(), "decay {decay} damping {damping} blew up");
                    if i < SR as usize { peak_early = peak_early.max(y.abs()); }
                }
                let mut peak_late = 0.0f32;
                for _ in 0..(SR as usize * 3) {
                    peak_late = peak_late.max(plate.process(0.0, &p).abs());
                }
                assert!(
                    peak_late < peak_early.max(1e-6),
                    "decay {decay} damping {damping}: tail grew ({peak_late} vs {peak_early})"
                );
            }
        }
    }

    /// Flutter is the failure coprime lengths exist to prevent: a tail that
    /// repeats at a short period, heard as a ringing rather than a wash.
    ///
    /// The measurement has to be a **peak against its own neighbours**, not an
    /// absolute correlation. The first version swept from 1 ms and reported
    /// 0.58 — which was not flutter at all but bandwidth: the tail is
    /// lowpassed, so samples half a millisecond apart are naturally similar. In
    /// the band flutter actually occupies, 20 to 200 ms, correlation sits
    /// between 0.05 and 0.29 with nothing standing out.
    #[test]
    fn the_tail_does_not_flutter() {
        let mut plate = Plate::new(SR);
        // Modulation off deliberately: density must come from the topology, not
        // from smearing the taps until the problem is hidden.
        let p = PlateParams { decay: 0.85, damping: 0.2, noise_mod: 0.0, mix: 1.0 };
        for i in 0..2000 {
            plate.process(if i < 64 { 1.0 } else { 0.0 }, &p);
        }
        let tail: Vec<f32> = (0..(SR as usize / 2)).map(|_| plate.process(0.0, &p)).collect();
        let energy: f32 = tail.iter().map(|s| s * s).sum();
        assert!(energy > 1e-9, "no tail to analyse");

        let lo = (SR as usize) * 20 / 1000;   // 20 ms
        let hi = (SR as usize) * 200 / 1000;  // 200 ms
        let mut corrs: Vec<f32> = Vec::new();
        for lag in (lo..hi).step_by(53) {
            let c: f32 = tail[..tail.len() - lag]
                .iter()
                .zip(&tail[lag..])
                .map(|(a, b)| a * b)
                .sum();
            corrs.push((c / energy).abs());
        }
        let peak = corrs.iter().cloned().fold(0.0f32, f32::max);
        let mut sorted = corrs.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];

        eprintln!("flutter band: peak {peak:.3}, median {median:.3}");
        assert!(peak < 0.4, "a lag stands out at {peak:.2} — the tail rings");
        assert!(
            peak < median.max(0.02) * 6.0,
            "peak {peak:.3} is {:.1}x the median {median:.3} — a standing period",
            peak / median.max(1e-6)
        );
    }

    #[test]
    fn the_bus_never_produces_a_non_finite_sample() {
        let mut bus = Bus::new(SR);
        let (pp, cp, dp) = (PlateParams::default(), ChorusParams::default(), density::DensityParams::default());
        for i in 0..(SR as usize * 2) {
            let x = ((i as f32 * 0.01).sin() * 0.7).clamp(-1.0, 1.0);
            assert!(bus.process(x, &pp, &cp, &dp).is_finite());
        }
    }
}
