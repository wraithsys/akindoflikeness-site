//! Render the claim, so it can be judged by ear instead of by test.
//!
//! For each algorithm: the same chord, in the same timbre, played twice —
//! first in the tuning computed from that algorithm's own spectrum, then in
//! the nearest 12-TET. If §3 of DESIGN.md is right, the first one beats less.
//! If it doesn't, that is worth knowing before any more is built on it.
//!
//! `cargo run --release -p phyllotaxis-tuning --example audition`
//! Writes WAVs to `renders/`. Deterministic — no RNG anywhere.

use std::f64::consts::TAU;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

use phyllotaxis_tuning::{
    spectrum_for, tuning_for, Algorithm, Variant, DEGREES_PER_SCALE,
};

const SR: u32 = 44_100;
const ROOT_HZ: f64 = 110.0;
const SEG_S: f64 = 4.0;
const GAP_S: f64 = 0.6;

/// The chord: root, then three degrees spread up the tuning. Held, because
/// this instrument has no note that ends — the question is what a sustained
/// stack does, and beating only shows up if you let it.
fn voice_degrees(cents: &[f64]) -> Vec<f64> {
    if cents.len() <= 4 {
        return cents.to_vec();
    }
    let n = cents.len();
    vec![cents[0], cents[n / 3], cents[2 * n / 3], cents[n - 1]]
}

/// Nearest 12-TET degree to each, so the comparison changes tuning and
/// nothing else.
fn to_12tet(cents: &[f64]) -> Vec<f64> {
    cents.iter().map(|c| (c / 100.0).round() * 100.0).collect()
}

fn render(spectrum: &phyllotaxis_tuning::spectrum::Spectrum, degrees: &[f64], out: &mut Vec<f64>) {
    let n = (SEG_S * SR as f64) as usize;
    let voices = degrees.len().max(1) as f64;
    for i in 0..n {
        let t = i as f64 / SR as f64;
        // A slow swell in and out, so nothing clicks and the tail is audible.
        let env = (1.0 - (TAU * 0.5 * t / SEG_S).cos()) * 0.5;
        let mut s = 0.0;
        for &deg in degrees {
            let f0 = ROOT_HZ * 2f64.powf(deg / 1200.0);
            for p in spectrum.partials() {
                let f = f0 * p.ratio;
                if f > 18_000.0 || f < 20.0 {
                    continue;
                }
                s += (TAU * f * t).sin() * p.amp;
            }
        }
        out.push(s * env / (voices * 6.0));
    }
}

fn silence(secs: f64, out: &mut Vec<f64>) {
    out.extend(std::iter::repeat(0.0).take((secs * SR as f64) as usize));
}

fn write_wav(path: &str, samples: &[f64]) -> std::io::Result<()> {
    let peak = samples.iter().fold(0.0f64, |m, s| m.max(s.abs())).max(1e-9);
    let gain = 0.89 / peak;
    let mut w = BufWriter::new(File::create(path)?);
    let data_len = (samples.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;          // PCM
    w.write_all(&1u16.to_le_bytes())?;          // mono
    w.write_all(&SR.to_le_bytes())?;
    w.write_all(&(SR * 2).to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        let v = (s * gain).clamp(-1.0, 1.0);
        w.write_all(&((v * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()
}

fn main() -> std::io::Result<()> {
    fs::create_dir_all("renders")?;
    let index = 4.0;

    println!("{:<10} {:<6} {:>8}  {}", "algorithm", "kind", "degrees", "chord (cents)");
    println!("{:-<74}", "");

    for algorithm in Algorithm::ALL {
        for variant in Variant::ALL {
            let spectrum = spectrum_for(algorithm, variant, index);
            let tuning = tuning_for(algorithm, variant, index, DEGREES_PER_SCALE);
            let matched = voice_degrees(&tuning.cents());
            let tempered = to_12tet(&matched);

            let mut buf = Vec::new();
            render(&spectrum, &matched, &mut buf);
            silence(GAP_S, &mut buf);
            render(&spectrum, &tempered, &mut buf);

            let name = format!(
                "renders/{}-{}.wav",
                algorithm.name().replace(' ', "_"),
                variant.numeral().to_lowercase()
            );
            write_wav(&name, &buf)?;

            let drift: f64 = matched
                .iter()
                .zip(tempered.iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
            println!(
                "{:<10} {:<6} {:>8}  {}   worst drift {:.0}c",
                format!("{} {}", algorithm.name(), variant.numeral()),
                tuning.kind().name(),
                tuning.len(),
                matched.iter().map(|c| format!("{c:.0}")).collect::<Vec<_>>().join(" "),
                drift
            );
        }
    }

    println!("\nEach file: 4s in the computed tuning, 0.6s silence, 4s in 12-TET.");
    println!("Same timbre, same chord, same everything else.");
    Ok(())
}
