//! Density control — Billy's design, behind a switch, to see if it earns its keep.
//!
//! > *"envelope follower on the verb send and reconcile with a sidechain to
//! > voice output and ride down decay on scheduled voice steal"*
//!
//! This is deliberately **not** the thing that was proposed and withdrawn.
//! That one derived FX depth from the algorithm's partial count — a lookup
//! keyed to the roster, which decides for the player and closes off the misuse
//! they would otherwise have found. This responds to the *signal*: it cannot
//! know which algorithm is playing and does not care. That difference is why
//! this one is allowed to exist at all.
//!
//! Three parts, all of which vanish exactly when `enabled` is false:
//!
//! 1. **Follower on the send.** The louder and denser the voices, the less goes
//!    to the plate — so a dense algorithm does not stack its own wash on top of
//!    a wash it already produced, and a sparse one still gets everything.
//! 2. **Sidechain to voice output.** The duck is driven by the dry signal, so
//!    the tail opens up in the gaps rather than being permanently smaller.
//! 3. **Decay riding down on a scheduled steal.** The allocator knows a voice
//!    is about to be displaced before it happens (`Pool::decide` returns the
//!    moment), so the tail can start making room *before* the steal rather than
//!    being surprised by it.

/// Ducking is judged by whether it can be switched off cleanly, so `enabled`
/// is not a mix amount — at `false` every path here is bypassed exactly.
#[derive(Clone, Copy, Debug)]
pub struct DensityParams {
    pub enabled: bool,
    /// 0…1 — how hard the send ducks under load.
    pub amount: f32,
    /// Follower attack, seconds. Fast enough to catch a chord arriving.
    pub attack_s: f32,
    /// Follower release, seconds. Slow, or the tail pumps.
    pub release_s: f32,
    /// 0…1 — how far decay drops while a steal is pending.
    pub steal_duck: f32,
}

impl Default for DensityParams {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 0.55,
            attack_s: 0.012,
            release_s: 0.45,
            steal_duck: 0.35,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Density {
    sample_rate: f32,
    follower: f32,
    /// Seconds until a scheduled steal lands, or `None`.
    pending_steal: Option<f32>,
}

impl Density {
    pub fn new(sample_rate: f32) -> Self {
        Self { sample_rate, follower: 0.0, pending_steal: None }
    }

    pub fn reset(&mut self) {
        self.follower = 0.0;
        self.pending_steal = None;
    }

    /// The allocator has scheduled a steal `in_secs` from now.
    ///
    /// This is the part that could only exist alongside predictive allocation:
    /// an ordinary voice pool does not know a steal is coming, so its reverb
    /// cannot get out of the way in advance.
    pub fn steal_scheduled(&mut self, in_secs: f32) {
        self.pending_steal = Some(in_secs.max(0.0));
    }

    pub fn follower(&self) -> f32 {
        self.follower
    }

    /// Advance the follower and return the send gain for this sample.
    #[inline]
    pub fn send_gain(&mut self, dry: f32, p: &DensityParams) -> f32 {
        if !p.enabled {
            return 1.0;
        }
        let dt = 1.0 / self.sample_rate;
        let target = dry.abs();
        let tau = if target > self.follower { p.attack_s } else { p.release_s };
        let k = 1.0 - (-dt / tau.max(1e-4)).exp();
        self.follower += (target - self.follower) * k;

        if let Some(t) = self.pending_steal.as_mut() {
            *t -= dt;
            if *t <= 0.0 {
                self.pending_steal = None;
            }
        }

        (1.0 - p.amount.clamp(0.0, 1.0) * self.follower.min(1.0)).clamp(0.0, 1.0)
    }

    /// Multiplier on the plate's decay while a steal is pending.
    #[inline]
    pub fn decay_scale(&self, p: &DensityParams) -> f32 {
        if !p.enabled || self.pending_steal.is_none() {
            return 1.0;
        }
        1.0 - p.steal_duck.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// The whole basis on which this is allowed to ship: switched off, it does
    /// nothing at all — not "almost nothing", nothing.
    #[test]
    fn disabled_is_an_exact_bypass() {
        let mut d = Density::new(SR);
        let p = DensityParams { enabled: false, ..Default::default() };
        d.steal_scheduled(0.1);
        for i in 0..10_000 {
            let x = (i as f32 * 0.003).sin();
            assert_eq!(d.send_gain(x, &p), 1.0);
            assert_eq!(d.decay_scale(&p), 1.0);
        }
    }

    #[test]
    fn the_send_ducks_under_load_and_opens_in_the_gaps() {
        let mut d = Density::new(SR);
        let p = DensityParams { enabled: true, ..Default::default() };
        // Loud passage.
        let mut loud = 1.0;
        for _ in 0..(SR as usize / 4) {
            loud = d.send_gain(0.9, &p);
        }
        // Then silence.
        let mut quiet = 0.0;
        for _ in 0..(SR as usize * 2) {
            quiet = d.send_gain(0.0, &p);
        }
        assert!(loud < 0.7, "send did not duck: {loud}");
        assert!(quiet > 0.95, "send did not reopen: {quiet}");
    }

    /// The follower must be slow enough on release that the tail does not pump.
    #[test]
    fn release_is_slow_enough_not_to_pump() {
        let mut d = Density::new(SR);
        let p = DensityParams { enabled: true, ..Default::default() };
        for _ in 0..(SR as usize / 4) { d.send_gain(0.9, &p); }
        let after_10ms = {
            for _ in 0..(SR as usize / 100) { d.send_gain(0.0, &p); }
            d.follower()
        };
        assert!(after_10ms > 0.5, "follower collapsed in 10 ms: {after_10ms}");
    }

    #[test]
    fn a_scheduled_steal_rides_the_decay_down_then_recovers() {
        let mut d = Density::new(SR);
        let p = DensityParams { enabled: true, ..Default::default() };
        assert_eq!(d.decay_scale(&p), 1.0);
        d.steal_scheduled(0.05);
        assert!(d.decay_scale(&p) < 1.0);
        for _ in 0..(SR as usize / 10) { d.send_gain(0.1, &p); }
        assert_eq!(d.decay_scale(&p), 1.0, "decay never recovered after the steal");
    }
}
