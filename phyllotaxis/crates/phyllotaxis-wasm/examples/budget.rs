use phyllotaxis_wasm::*;
use std::time::Instant;
fn main() {
    let sr = 48_000.0f32;
    let quantum = 128.0 / sr as f64;
    println!("quantum = {:.3} ms", quantum * 1e3);
    let mut e = Engine::new(sr);
    for round in 0..3 {
        for entry in 0..8u32 {
            let t = Instant::now(); e.step_by(1); let a = t.elapsed().as_secs_f64();
            let t = Instant::now(); e.set(1, 4.0 + round as f32 * 0.5); let b = t.elapsed().as_secs_f64();
            if a > quantum * 0.2 || b > quantum * 0.2 {
                println!("round {round} entry {entry}: step {:.2} ms, set {:.2} ms", a * 1e3, b * 1e3);
            }
        }
    }
    println!("done");
}
