//! One operator: a sine, its phase, and nothing else.
//!
//! Phase is kept as a fraction of a cycle rather than in radians, so it wraps
//! by subtraction instead of by `%` and never loses precision at long
//! runtimes. A drone that has been sounding for an hour must be as clean as
//! one that started a second ago, and `f32` radians is not.

use core::f32::consts::TAU;

/// Below this, an `f32` is subnormal and every operation on it costs an order
/// of magnitude more than it should.
///
/// On x86 this is free — the FPU's flush-to-zero mode handles it. **WebAssembly
/// has no such mode**: IEEE 754 subnormals are required by the specification,
/// so every feedback path in this instrument has to flush by hand or its tail
/// costs more CPU than its head. See `DESIGN.md` §1.
const DENORMAL_FLOOR: f32 = 1e-20;

#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < DENORMAL_FLOOR { 0.0 } else { x }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Operator {
    /// Fraction of a cycle, always in [0, 1).
    phase: f32,
    inc: f32,
}

impl Operator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the frequency. `sample_rate` in Hz.
    #[inline]
    pub fn set_hz(&mut self, hz: f32, sample_rate: f32) {
        self.inc = hz / sample_rate;
    }

    /// Advance one sample and return the sine, with `pm` added to the phase as
    /// a fraction of a cycle — phase modulation, which is what every hardware
    /// "FM" synth has always actually done.
    #[inline]
    pub fn tick(&mut self, pm: f32) -> f32 {
        let out = ((self.phase + pm) * TAU).sin();
        self.phase += self.inc;
        // Wrap by subtraction: exact, and no modulo on the audio path.
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }

    /// The current sine without advancing — used where one operator's output
    /// must be read twice in a sample.
    #[inline]
    pub fn peek(&self) -> f32 {
        (self.phase * TAU).sin()
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Start somewhere other than zero. Voices in a unison pair want different
    /// starting phases or they cancel on the first sample and swell in.
    pub fn set_phase(&mut self, phase01: f32) {
        self.phase = phase01.rem_euclid(1.0);
    }
}

/// A one-pole DC blocker.
///
/// Rectification emits a large DC term — `2/π` for full-wave — and that offset
/// walks straight into the plate, eats headroom and leaves the tail sitting off
/// centre. `DESIGN.md` §2 calls this not optional, so it is not a parameter.
#[derive(Clone, Copy, Debug, Default)]
pub struct DcBlocker {
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    /// ≈ 20 Hz corner at 48 kHz. High enough to remove the offset within a
    /// note, low enough to leave the lowest sub alone.
    const R: f32 = 0.9975;

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + Self::R * self.y1;
        self.x1 = x;
        self.y1 = flush(y);
        self.y1
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_stays_bounded_over_a_long_run() {
        let mut op = Operator::new();
        op.set_hz(440.0, 48_000.0);
        for _ in 0..48_000 * 60 {
            let s = op.tick(0.0);
            assert!(s.is_finite() && s.abs() <= 1.0001);
        }
        assert!((0.0..1.0).contains(&op.phase), "phase escaped: {}", op.phase);
    }

    #[test]
    fn a_sine_is_a_sine() {
        let mut op = Operator::new();
        op.set_hz(1.0, 4.0); // quarter cycle per sample
        let got: Vec<f32> = (0..4).map(|_| op.tick(0.0)).collect();
        let want = [0.0, 1.0, 0.0, -1.0];
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "{got:?}");
        }
    }

    #[test]
    fn dc_blocker_removes_an_offset() {
        let mut dc = DcBlocker::default();
        let mut last = 0.0;
        for _ in 0..48_000 {
            last = dc.process(0.5);
        }
        assert!(last.abs() < 1e-3, "offset survived: {last}");
    }

    #[test]
    fn dc_blocker_keeps_the_signal() {
        let mut dc = DcBlocker::default();
        let mut op = Operator::new();
        op.set_hz(220.0, 48_000.0);
        let mut peak = 0.0f32;
        for _ in 0..48_000 {
            peak = peak.max(dc.process(op.tick(0.0)).abs());
        }
        assert!(peak > 0.9, "signal was eaten: {peak}");
    }

    #[test]
    fn denormals_are_flushed() {
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-1e-30), 0.0);
        assert_eq!(flush(0.5), 0.5);
    }
}
