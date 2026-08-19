//! Where the plate's tail sits against the field's rest.
//!
//! `DESIGN.md` §5 makes the gesture `interval/φ` and the rest `interval/φ²`.
//! Dry, that rest is a real fall toward the floor. With a plate, the tail
//! either fills it, falls short of it, or carries across it into the next
//! gesture — so the plate is a party to the amplitude contour without ever
//! touching amplitude.
//!
//! The two rests are φ apart, because the two step durations are, so one decay
//! cannot match both. It can sit on the short one, on the long one, or between
//! them at their geometric mean.
//!
//! `cargo run --release -p phyllotaxis-fx --example rt`

use phyllotaxis_fx::{Plate, PlateParams};

const SR: f32 = 48_000.0;
const PHI: f32 = 1.618_034;

/// Time for the tail to fall 60 dB below its peak.
pub fn rt60(decay: f32, damping: f32) -> f32 {
    let mut plate = Plate::new(SR);
    let p = PlateParams { decay, damping, noise_mod: 0.35, mix: 1.0 };
    let mut peak = 0.0f32;
    for i in 0..4000 {
        peak = peak.max(plate.process(if i < 64 { 1.0 } else { 0.0 }, &p).abs());
    }
    let target = peak * 0.001;
    let (mut n, mut quiet) = (0usize, 0usize);
    while n < (SR as usize * 30) {
        let y = plate.process(0.0, &p).abs();
        n += 1;
        if y < target {
            quiet += 1;
            if quiet > 2400 { break; }
        } else {
            quiet = 0;
        }
    }
    (n - quiet) as f32 / SR
}

fn main() {
    let mean = 1.618f32;
    let long = mean * 1.170_820;
    let short = mean * 0.723_607;
    let rest = |iv: f32| iv / (PHI * PHI);

    println!("field rests at mean interval {mean}s");
    println!("  long step  {long:.3}s -> rest {:.3}s", rest(long));
    println!("  short step {short:.3}s -> rest {:.3}s", rest(short));
    println!("  ratio {:.4}  (phi = {PHI:.4})", rest(long) / rest(short));
    println!("  geometric mean of the two rests: {:.3}s", (rest(long) * rest(short)).sqrt());

    println!("\nplate RT60 by decay");
    for &d in &[0.50f32, 0.60, 0.66, 0.74, 0.85, 0.95] {
        let t = rt60(d, 0.34);
        let note = if (t - rest(short)).abs() < 0.05 {
            "  ~ short rest"
        } else if (t - rest(long)).abs() < 0.05 {
            "  ~ long rest"
        } else if t > rest(long) {
            "  carries past the rest"
        } else {
            ""
        };
        println!("  decay {d:.2}  RT60 {t:.3}s{note}");
    }
}
