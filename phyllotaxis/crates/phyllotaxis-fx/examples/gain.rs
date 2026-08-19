//! How loud is the plate's wet against the dry that feeds it? Sizes the
//! control's range so `mix` spans something usable end to end.
use phyllotaxis_fx::*;
fn main() {
    let sr = 48_000.0;
    let mut plate = Plate::new(sr);
    let p = PlateParams { decay: 0.74, damping: 0.34, noise_mod: 0.35, mix: 1.0 };
    let mut z = 22222u32;
    let (mut dry_e, mut wet_e) = (0.0f64, 0.0f64);
    let n = sr as usize * 6;
    for i in 0..n {
        z = z.wrapping_mul(1664525).wrapping_add(1013904223);
        let x = ((z >> 8) as f32 / 8388608.0 - 1.0) * 0.2;
        let (l, r) = plate.process_stereo(x, &p);
        if i > sr as usize {           // let the tank fill
            dry_e += (x * x) as f64;
            wet_e += ((l * l + r * r) * 0.5) as f64;
        }
    }
    let ratio = (wet_e / dry_e).sqrt();
    println!("wet/dry RMS = {ratio:.4}  ({:.1} dB)", 20.0 * ratio.log10());
    println!("makeup for unity at mix=1: {:.1}x", 1.0 / ratio);
}
