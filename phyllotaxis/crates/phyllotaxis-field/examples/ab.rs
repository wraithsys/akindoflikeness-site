//! The field, A/B against the bare voice — per `DESIGN.md` §11's build rule.
//!
//! Each file: 4s of the voice with a plain cosine swell over it (what Billy
//! heard and called stellar), 0.6s of silence, then 4s with the field driving
//! amplitude instead. Same chord, same algorithm, same everything else.
//!
//! `cargo run --release -p phyllotaxis-field --example ab`

use std::f32::consts::TAU;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

use phyllotaxis_field::{Field, FieldParams};
use phyllotaxis_tuning::{roster_name, tuning_for, Algorithm, Variant, DEGREES_PER_SCALE, ROSTER};
use phyllotaxis_voice::{Voice, VoiceParams};

const SR: f32 = 48_000.0;
const ROOT_HZ: f32 = 110.0;
const SEG_S: f32 = 4.0;
const GAP_S: f32 = 0.6;
const UNISON: usize = 2;
const DETUNE_CENTS: f32 = 6.0;
/// The chord-change interval Cadence would be feeding in.
const INTERVAL_S: f32 = 3.2;

fn chord(cents: &[f64]) -> Vec<f64> {
    if cents.len() <= 4 { return cents.to_vec(); }
    let n = cents.len();
    vec![cents[0], cents[n / 3], cents[2 * n / 3], cents[n - 1]]
}

struct Stack {
    voices: Vec<Voice>,
    fields: Vec<Field>,
    freqs: Vec<f32>,
}

fn build(params: VoiceParams, degrees: &[f64]) -> Stack {
    let mut s = Stack { voices: vec![], fields: vec![], freqs: vec![] };
    for (vi, &deg) in degrees.iter().enumerate() {
        for u in 0..UNISON {
            let detune = if u == 0 { -DETUNE_CENTS } else { DETUNE_CENTS } * 0.5;
            let hz = ROOT_HZ * 2f32.powf((deg as f32 + detune) / 1200.0);
            let n = vi * UNISON + u;

            let mut v = Voice::new(SR);
            v.set_params(params);
            v.set_root_hz(hz);
            v.set_phase_offset((n as f32 * 0.618_034) % 1.0);

            let mut f = Field::new(SR, params.algorithm, params.variant);
            f.set_voice_index(n);
            f.set_interval(INTERVAL_S);
            f.strike();

            s.voices.push(v);
            s.fields.push(f);
            s.freqs.push(hz);
        }
    }
    s
}

/// `field: false` reproduces the bare-voice benchmark exactly.
fn render(params: VoiceParams, degrees: &[f64], out: &mut Vec<f32>, field: bool) {
    let mut s = build(params, degrees);
    let fp = FieldParams { floor: 0.12, depth: 0.85, curve: 0.42, attack: 0.30 };
    let n = (SEG_S * SR) as usize;
    let scale = 1.0 / (s.voices.len() as f32).sqrt();

    for i in 0..n {
        let t = i as f32 / SR;
        // The gesture restarts every interval, as a chord change would.
        if field && i > 0 && i % ((INTERVAL_S * SR) as usize) == 0 {
            for f in s.fields.iter_mut() { f.strike(); }
        }
        let swell = (1.0 - (TAU * 0.5 * t / SEG_S).cos()) * 0.5;
        let mut acc = 0.0;
        for k in 0..s.voices.len() {
            let sample = s.voices[k].tick();
            let amp = if field {
                s.fields[k].tick(s.freqs[k], &fp)
            } else {
                swell
            };
            acc += sample * amp;
        }
        // Both halves fade at the edges so nothing clicks; only the middle is
        // the comparison.
        let edge = (t / 0.25).min(1.0).min((SEG_S - t) / 0.25).max(0.0);
        out.push(acc * scale * edge);
    }
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
    fs::create_dir_all("renders/field")?;
    println!("{:<10} {:<14} {:>7}   chord", "entry", "signature", "period");
    println!("{:-<62}", "");

    for &(algorithm, variant) in ROSTER.iter() {
        let params = VoiceParams { algorithm, variant, index: 4.0, free_ratio: 1.0 };
        let tuning = tuning_for(algorithm, variant, 4.0, DEGREES_PER_SCALE);
        let degrees = chord(&tuning.cents());

        let mut buf = Vec::new();
        render(params, &degrees, &mut buf, false);
        buf.extend(std::iter::repeat(0.0).take((GAP_S * SR) as usize));
        render(params, &degrees, &mut buf, true);

        let sig = phyllotaxis_field::signature::signature_for(algorithm, variant);
        let name = roster_name(algorithm, variant).replace(' ', "-");
        write_wav(&format!("renders/field/{name}.wav"), &buf)?;
        println!(
            "{:<10} {:<14} {:>7}   {}",
            name, sig.family, sig.period(),
            degrees.iter().map(|c| format!("{c:.0}")).collect::<Vec<_>>().join(" ")
        );
    }

    println!("\nEach file: 4s bare voice (cosine swell), 0.6s silence, 4s with the field.");
    println!("The second half is the one on trial.");
    Ok(())
}
