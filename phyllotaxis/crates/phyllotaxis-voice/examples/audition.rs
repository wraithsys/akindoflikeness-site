//! The real signal path, rendered — not a sine bank standing in for it.
//!
//! Same format as the tuning crate's audition so the two can be compared
//! directly: the computed tuning, silence, then 12-TET. Plus an index sweep,
//! because INDEX is the control the scale itself tracks and it should be
//! audible that it is doing two things at once.
//!
//! `cargo run --release -p phyllotaxis-voice --example audition`

use std::f32::consts::TAU;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

use phyllotaxis_tuning::{tuning_for, Algorithm, Variant, DEGREES_PER_SCALE};
use phyllotaxis_voice::{Voice, VoiceParams};

const SR: f32 = 48_000.0;
const ROOT_HZ: f32 = 110.0;
const SEG_S: f32 = 4.0;
const GAP_S: f32 = 0.6;
/// Per DESIGN.md §2 — two per voice, and they need different phases or the
/// pair starts as one voice at double amplitude.
const UNISON: usize = 2;
const DETUNE_CENTS: f32 = 6.0;

fn chord(cents: &[f64]) -> Vec<f64> {
    if cents.len() <= 4 { return cents.to_vec(); }
    let n = cents.len();
    vec![cents[0], cents[n / 3], cents[2 * n / 3], cents[n - 1]]
}

fn to_12tet(cents: &[f64]) -> Vec<f64> {
    cents.iter().map(|c| (c / 100.0).round() * 100.0).collect()
}

/// Render a held chord. No envelope — amplitude moves, it does not shape, so
/// this is a swell in and out around a floor rather than an ADSR.
fn render(params: VoiceParams, degrees: &[f64], out: &mut Vec<f32>, sweep: bool) {
    let n = (SEG_S * SR) as usize;
    let mut voices: Vec<Voice> = Vec::new();
    for (vi, &deg) in degrees.iter().enumerate() {
        for u in 0..UNISON {
            let detune = if u == 0 { -DETUNE_CENTS } else { DETUNE_CENTS } * 0.5;
            let hz = ROOT_HZ * 2f32.powf((deg as f32 + detune) / 1200.0);
            let mut v = Voice::new(SR);
            v.set_params(params);
            v.set_root_hz(hz);
            // Golden-rotation phase offsets, so the stack never starts aligned.
            v.set_phase_offset(((vi * UNISON + u) as f32 * 0.618_034) % 1.0);
            voices.push(v);
        }
    }

    let scale = 1.0 / (voices.len() as f32).sqrt();
    for i in 0..n {
        let t = i as f32 / SR;
        let env = (1.0 - (TAU * 0.5 * t / SEG_S).cos()) * 0.5;
        if sweep {
            // INDEX rising across the segment: sidebands appear, and the scale
            // the voice wants moves under it at the same time.
            let idx = 0.5 + 11.5 * (t / SEG_S);
            if i % 64 == 0 {
                for v in voices.iter_mut() {
                    let mut p = v.params();
                    p.index = idx;
                    v.set_params(p);
                }
            }
        }
        let mut s = 0.0;
        for v in voices.iter_mut() {
            s += v.tick();
        }
        out.push(s * scale * env);
    }
}

fn silence(secs: f32, out: &mut Vec<f32>) {
    out.extend(std::iter::repeat(0.0).take((secs * SR) as usize));
}

fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
    let gain = 0.89 / peak;
    let mut w = BufWriter::new(File::create(path)?);
    let data_len = (samples.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&(SR as u32).to_le_bytes())?;
    w.write_all(&((SR as u32) * 2).to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        w.write_all(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()
}

fn main() -> std::io::Result<()> {
    fs::create_dir_all("renders/voice")?;
    let index = 4.0;

    for algorithm in Algorithm::ALL {
        for variant in Variant::ALL {
            let params = VoiceParams { algorithm, variant, index, free_ratio: 1.0 };
            let tuning = tuning_for(algorithm, variant, index as f64, DEGREES_PER_SCALE);
            let matched = chord(&tuning.cents());
            let tempered = to_12tet(&matched);

            let mut buf = Vec::new();
            render(params, &matched, &mut buf, false);
            silence(GAP_S, &mut buf);
            render(params, &tempered, &mut buf, false);

            let name = format!(
                "renders/voice/{}-{}.wav",
                algorithm.name().replace(' ', "_"),
                variant.numeral().to_lowercase()
            );
            write_wav(&name, &buf)?;
            println!(
                "{:<10} {:<6} {:>2} voices x{UNISON} unison   {}",
                format!("{} {}", algorithm.name(), variant.numeral()),
                tuning.kind().name(),
                matched.len(),
                matched.iter().map(|c| format!("{c:.0}")).collect::<Vec<_>>().join(" ")
            );
        }
    }

    // INDEX sweeping, on the algorithm with the most to reveal.
    for (algo, var, tag) in [
        (Algorithm::Fm1, Variant::Golden, "fm-ii"),
        (Algorithm::Rect, Variant::Golden, "rect-ii"),
    ] {
        let tuning = tuning_for(algo, var, 4.0, DEGREES_PER_SCALE);
        let mut buf = Vec::new();
        render(VoiceParams { algorithm: algo, variant: var, index: 0.5, free_ratio: 1.0 },
               &chord(&tuning.cents()), &mut buf, true);
        write_wav(&format!("renders/voice/sweep-{tag}.wav"), &buf)?;
        println!("sweep-{tag}: index 0.5 -> 12 across 4s");
    }

    println!("\nEach: 4s in the computed tuning, 0.6s silence, 4s in 12-TET.");
    println!("Real voice - 3 operators, 2-voice unison, 6c detune, golden phase offsets.");
    Ok(())
}
