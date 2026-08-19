//! Why the dry voice already sounds chorused, reverberant and unisoned.
//!
//! Billy, hearing the bare additive audition: *"i can hear chorus reverb and
//! the fact it was mono … there was also unison going on"* — against a render
//! containing none of those. He is not mishearing. Sound a chord of inharmonic
//! spectra and the partials of different notes land near each other; near
//! coincidences beat, and beating at a few Hz is what chorus IS. This counts
//! them, so the effect is a measured quantity rather than an impression.
//!
//! `cargo run --release -p phyllotaxis-tuning --example coincidences`

use phyllotaxis_tuning::{spectrum_for, tuning_for, Algorithm, Variant, DEGREES_PER_SCALE};

const ROOT_HZ: f64 = 110.0;

/// Beats slower than this are heard as one tone drifting, not two tones.
const SLOW_HZ: f64 = 0.05;
/// Above roughly this, a pair stops beating and starts sounding rough.
const CHORUS_HZ: f64 = 8.0;

fn chord(cents: &[f64]) -> Vec<f64> {
    if cents.len() <= 4 { return cents.to_vec(); }
    let n = cents.len();
    vec![cents[0], cents[n / 3], cents[2 * n / 3], cents[n - 1]]
}

fn main() {
    println!(
        "{:<10} {:>7} {:>9} {:>9} {:>9}   {}",
        "algorithm", "voices", "partials", "unison", "chorus", "slowest beat"
    );
    println!("{:-<80}", "");

    for algorithm in Algorithm::ALL {
        for variant in Variant::ALL {
            let spectrum = spectrum_for(algorithm, variant, 4.0);
            let tuning = tuning_for(algorithm, variant, 4.0, DEGREES_PER_SCALE);
            let degrees = chord(&tuning.cents());

            // Every partial of every note, in Hz.
            let mut all: Vec<(f64, usize)> = Vec::new();
            for (vi, &d) in degrees.iter().enumerate() {
                let f0 = ROOT_HZ * 2f64.powf(d / 1200.0);
                for p in spectrum.partials() {
                    let f = f0 * p.ratio;
                    if (20.0..18_000.0).contains(&f) {
                        all.push((f, vi));
                    }
                }
            }
            all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            let mut unison = 0;      // exact coincidence: reinforcement
            let mut chorusing = 0;   // slow beat: heard as chorus
            let mut slowest = f64::INFINITY;

            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    let beat = all[j].0 - all[i].0;
                    if beat > CHORUS_HZ { break; }
                    // Only pairs from DIFFERENT notes — within one note the
                    // partials are the timbre, not an interaction.
                    if all[i].1 == all[j].1 { continue; }
                    if beat < SLOW_HZ {
                        unison += 1;
                    } else {
                        chorusing += 1;
                        slowest = slowest.min(beat);
                    }
                }
            }

            println!(
                "{:<10} {:>7} {:>9} {:>9} {:>9}   {}",
                format!("{} {}", algorithm.name(), variant.numeral()),
                degrees.len(),
                all.len(),
                unison,
                chorusing,
                if slowest.is_finite() { format!("{slowest:.2} Hz") } else { "—".into() }
            );
        }
    }

    println!("\nunison  = partial pairs from different notes within {SLOW_HZ} Hz — exact");
    println!("          reinforcement, which is what a unison voice does");
    println!("chorus  = pairs beating between {SLOW_HZ} and {CHORUS_HZ} Hz — audible as chorus");
    println!("\nNo chorus, reverb, unison or filter is applied anywhere in the render.");
}
