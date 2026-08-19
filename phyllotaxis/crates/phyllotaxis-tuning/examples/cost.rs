//! What does retuning cost? It runs on the audio thread, so this is a
//! real-time budget question, not a curiosity.
use phyllotaxis_tuning::{tuning_for, DEGREES_PER_SCALE, ROSTER};
use std::time::Instant;

fn main() {
    // One 128-frame quantum at 48 kHz is the entire budget for a render call.
    let quantum_us = 128.0 / 48_000.0 * 1e6;
    println!("one render quantum = {quantum_us:.0} µs\n");
    println!("{:<6} {:>12} {:>10}", "entry", "tuning_for", "× quantum");
    let mut worst = 0.0f64;
    for (i, &(a, v)) in ROSTER.iter().enumerate() {
        let t = Instant::now();
        let n = 20;
        for k in 0..n {
            let idx = 4.0 + k as f64 * 1e-6;
            std::hint::black_box(tuning_for(a, v, idx, DEGREES_PER_SCALE));
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        worst = worst.max(us);
        println!("{:<6} {:>10.0} µs {:>9.0}×", i, us, us / quantum_us);
    }
    println!("\nworst case {:.1} ms — {:.0} quanta of audio missed", worst / 1000.0, worst / quantum_us);
}
