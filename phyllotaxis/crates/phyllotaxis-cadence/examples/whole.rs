//! The whole chain, playing itself.
//!
//! Cadence chooses the chords and how long each lasts; the mirror reflects some
//! of them; the strum offsets their arrivals; the pool allocates by prediction
//! and refuses to retrigger a common tone; the field drives amplitude with no
//! envelope anywhere; the bus adds the plate and the hyperchorus.
//!
//! Per `DESIGN.md` §11's build rule each file is an A/B — the same 24 seconds
//! twice, first dry and then through the bus — so the last stage still has to
//! win against what came before it.
//!
//! `cargo run --release -p phyllotaxis-cadence --example whole`

use std::fs::{self, File};
use std::io::{BufWriter, Write};

use phyllotaxis_cadence::{leading, mirror::Mirror, mirror_this_chord, strum, word};
use phyllotaxis_field::FieldParams;
use phyllotaxis_fx::{density::DensityParams, Bus, ChorusParams, PlateParams};
use phyllotaxis_pool::Pool;
use phyllotaxis_tuning::{roster_name, tuning_for, Kind, DEGREES_PER_SCALE, ROSTER};
use phyllotaxis_voice::VoiceParams;

const SR: f32 = 48_000.0;
const ROOT_HZ: f64 = 110.0;
const MEAN_INTERVAL_S: f32 = 1.618;
const CHORDS: u64 = 15;
const BIAS: f32 = -0.7; // ascending strum
const MIRROR_AMOUNT: f64 = word::INV_PHI2; // the Fibonacci word density

/// Build a chord from a scale by stacking degrees — root, third, fifth,
/// seventh in scale steps. A chord tuning is sounded whole, as §3 requires.
fn chord_degrees(cents: &[f64], kind: Kind, n: u64) -> Vec<f64> {
    if kind == Kind::Chord {
        return cents.iter().copied().filter(|&c| c < 1200.0).collect();
    }
    let inside: Vec<f64> = cents.iter().copied().filter(|&c| c < 1200.0).collect();
    let k = inside.len();
    if k == 0 {
        return vec![0.0];
    }
    // Golden rotation picks where in the scale the chord is rooted.
    let r = (((n as f64) * word::INV_PHI).fract() * k as f64) as usize;
    [0usize, 2, 4, 6]
        .iter()
        .map(|&step| inside[(r + step) % k] + 1200.0 * (((r + step) / k) as f64))
        .collect()
}

fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
    let gain = 0.89 / peak;
    let mut w = BufWriter::new(File::create(path)?);
    let n = (samples.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + n).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&(SR as u32).to_le_bytes())?;
    w.write_all(&((SR as u32) * 2).to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&n.to_le_bytes())?;
    for s in samples {
        w.write_all(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()
}

fn render(algorithm: phyllotaxis_tuning::Algorithm, variant: phyllotaxis_tuning::Variant, wet: bool) -> Vec<f32> {
    let index = 4.0;
    let tuning = tuning_for(algorithm, variant, index as f64, DEGREES_PER_SCALE);
    let cents = tuning.cents();
    let mirror = Mirror::new(&cents);

    let params = VoiceParams { algorithm, variant, index, free_ratio: 1.0 };
    let mut pool = Pool::new(SR, params);
    pool.field_params = FieldParams { floor: 0.10, depth: 0.85, curve: 0.42, attack: 0.30 };

    let mut bus = Bus::new(SR);
    let (pp, cp, dp) = (
        PlateParams { decay: 0.74, damping: 0.34, noise_mod: 0.35, mix: 0.30 },
        ChorusParams { depth: 0.4, rate: 0.35, mix: 0.30 },
        DensityParams::default(), // off: it has to earn its keep separately
    );

    let mut out: Vec<f32> = Vec::new();
    let mut sounding: Vec<f64> = Vec::new();

    for n in 1..=CHORDS {
        let interval = word::interval_s(n, MEAN_INTERVAL_S);
        pool.set_interval(interval);

        let mut degrees = chord_degrees(&cents, tuning.kind(), n);
        let flipped = tuning.kind() != Kind::Chord && mirror_this_chord(n, 0.0, MIRROR_AMOUNT);
        if flipped {
            degrees = mirror.reflect_chord(&degrees);
        }

        // Realise against the home ladder so the chord does not walk away.
        let root_c = leading::cents_of(ROOT_HZ);
        let mut targets: Vec<f64> = degrees
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                let h = leading::home(i.min(4), root_c);
                leading::realise(d % 1200.0, root_c, h, h)
            })
            .collect();
        targets.sort_by(|a, b| a.partial_cmp(b).unwrap());
        targets.dedup_by(|a, b| (*a - *b).abs() < 5.0);
        targets.truncate(5);

        // Notes that have gone: release them, strummed in reverse.
        let leaving: Vec<f64> = sounding
            .iter()
            .copied()
            .filter(|s| !targets.iter().any(|t| (t - s).abs() < 5.0))
            .collect();
        let rel = strum::release_offsets(leaving.len(), BIAS, strum::budget_s(interval));
        for (k, pitch) in leaving.iter().enumerate() {
            let _ = rel[k]; // scheduling granularity is the block below
            pool.note_off(leading::hz_of(*pitch) as f32);
        }

        // Notes arriving. A common tone returns AlreadySounding and is never
        // retriggered — voice leading's audible half, enforced by the pool.
        let arriving: Vec<f64> = targets
            .iter()
            .copied()
            .filter(|t| !sounding.iter().any(|s| (t - s).abs() < 5.0))
            .collect();
        let att = strum::attack_offsets(arriving.len(), BIAS, strum::budget_s(interval));

        let total = (interval * SR) as usize;
        let mut fired = vec![false; arriving.len()];
        for i in 0..total {
            let t = i as f32 / SR;
            for (k, pitch) in arriving.iter().enumerate() {
                if !fired[k] && t >= att[k] {
                    fired[k] = true;
                    pool.note_on(leading::hz_of(*pitch) as f32, att[k]);
                }
            }
            let dry = pool.tick();
            out.push(if wet { bus.process(dry, &pp, &cp, &dp) } else { dry });
        }
        sounding = targets;
    }

    // Fade the very edges so nothing clicks.
    let fade = (0.05 * SR) as usize;
    let len = out.len();
    for i in 0..fade.min(len) {
        let g = i as f32 / fade as f32;
        out[i] *= g;
        out[len - 1 - i] *= g;
    }
    out
}

fn main() -> std::io::Result<()> {
    fs::create_dir_all("renders/whole")?;
    println!("{:<10} {:>8} {:>8}  chords, and how long each lasts", "entry", "kind", "secs");
    println!("{:-<66}", "");

    for &(algorithm, variant) in ROSTER.iter() {
        let mut buf = render(algorithm, variant, false);
        buf.extend(std::iter::repeat(0.0).take((0.7 * SR) as usize));
        buf.extend(render(algorithm, variant, true));

        let name = roster_name(algorithm, variant).replace(' ', "-");
        write_wav(&format!("renders/whole/{name}.wav"), &buf)?;

        let tuning = tuning_for(algorithm, variant, 4.0, DEGREES_PER_SCALE);
        let secs: f32 = (1..=CHORDS).map(|n| word::interval_s(n, MEAN_INTERVAL_S)).sum();
        println!("{:<10} {:>8} {:>8.1}  {}", name, tuning.kind().name(), secs, word::prefix(CHORDS as usize));
    }

    println!("\nEach file: {CHORDS} chords dry, 0.7s silence, the same {CHORDS} through the bus.");
    println!("Intervals alternate long/short by the Fibonacci word. Mirror density 1/φ².");
    Ok(())
}
