//! Is variant II measurably smoother than variant I?
//!
//! Billy, on the whole-chain renders: *"2 over 1 — it's smoother and more
//! manageable dynamically, gives alive over unstable."* That is a claim about
//! dynamics, so it can be measured rather than agreed with.
//!
//! Two numbers per entry, both over the rendered chain:
//!   crest   — peak / RMS. How far the loudest moment is above the average.
//!   flux    — mean absolute change in short-window RMS. How much the level
//!             lurches from one 50 ms window to the next.
//! Low crest and low flux is "manageable". High flux with low crest is "alive".

use phyllotaxis_field::FieldParams;
use phyllotaxis_pool::Pool;
use phyllotaxis_tuning::{roster_name, spectrum_for, tuning_for, Kind, DEGREES_PER_SCALE, ROSTER};
use phyllotaxis_voice::VoiceParams;
use phyllotaxis_cadence::word;

const SR: f32 = 48_000.0;

fn render(a: phyllotaxis_tuning::Algorithm, v: phyllotaxis_tuning::Variant) -> Vec<f32> {
    let params = VoiceParams { algorithm: a, variant: v, index: 4.0, free_ratio: 1.0 };
    let tuning = tuning_for(a, v, 4.0, DEGREES_PER_SCALE);
    let cents: Vec<f64> = tuning.cents().into_iter().filter(|&c| c < 1200.0).collect();

    let mut pool = Pool::new(SR, params);
    pool.field_params = FieldParams { depth: 0.55, attack: 0.30, ..Default::default() };

    let mut out = Vec::new();
    for n in 1..=10u64 {
        let interval = word::interval_s(n, 1.618);
        pool.set_interval(interval);
        let k = cents.len().max(1);
        let r = (((n as f64) * word::INV_PHI).fract() * k as f64) as usize;
        let picks: Vec<f64> = if tuning.kind() == Kind::Chord {
            cents.clone()
        } else {
            [0usize, 2, 4, 6].iter().map(|&s| cents[(r + s) % k]).collect()
        };
        for &c in &picks {
            pool.note_on(110.0 * 2f32.powf(c as f32 / 1200.0), 0.0);
        }
        for _ in 0..(interval * SR) as usize {
            out.push(pool.tick());
        }
    }
    out
}

fn cents_of(t: &phyllotaxis_tuning::Tuning) -> Vec<f64> {
    t.cents().into_iter().filter(|&c| c < 1200.0).collect()
}

fn main() {
    println!(
        "{:<10} {:>8} {:>7} {:>8} {:>8} {:>9}",
        "entry", "partials", "exact", "beating", "crest", "flux"
    );
    println!("{:-<50}", "");

    let mut by_variant: [(f64, f64, usize); 2] = [(0.0, 0.0, 0); 2];

    for &(a, v) in ROSTER.iter() {
        let buf = render(a, v);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())) as f64;
        let rms = (buf.iter().map(|s| (s * s) as f64).sum::<f64>() / buf.len() as f64).sqrt();
        let crest = if rms > 0.0 { peak / rms } else { 0.0 };

        // Level flux: how much the short-window RMS lurches.
        let win = (0.05 * SR) as usize;
        let levels: Vec<f64> = buf
            .chunks(win)
            .map(|c| (c.iter().map(|s| (s * s) as f64).sum::<f64>() / c.len() as f64).sqrt())
            .collect();
        let flux = levels.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>()
            / (levels.len().max(2) - 1) as f64
            / rms.max(1e-9);

        let tuning = tuning_for(a, v, 4.0, DEGREES_PER_SCALE);
        let spectrum = spectrum_for(a, v, 4.0);
        let partials = spectrum.partials().len();

        // Beating pairs across a sounding CHORD, not within one voice.
        //
        // Measuring inside a single voice was the wrong scope and returned
        // zero for every entry: one voice's partials are widely spaced, so
        // nothing beats. The interaction is between partials of DIFFERENT
        // notes, which is where the 354 pairs came from.
        let mut beating = 0;
        let notes: Vec<f64> = if tuning.kind() == Kind::Chord {
            cents_of(&tuning).to_vec()
        } else {
            let c = cents_of(&tuning);
            [0usize, 2, 4, 6].iter().filter_map(|&i| c.get(i).copied()).collect()
        };
        let mut hz: Vec<(f64, usize)> = Vec::new();
        for (vi, &c) in notes.iter().enumerate() {
            let f0 = 110.0 * 2f64.powf(c / 1200.0);
            for p in spectrum.partials() {
                let f = f0 * p.ratio;
                if (20.0..18_000.0).contains(&f) {
                    hz.push((f, vi));
                }
            }
        }
        hz.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut exact = 0;
        for i in 0..hz.len() {
            for j in (i + 1)..hz.len() {
                let d = hz[j].0 - hz[i].0;
                if d > 8.0 { break; }
                if hz[i].1 == hz[j].1 { continue; }
                if d < 0.05 { exact += 1; } else { beating += 1; }
            }
        }

        let idx = if v == phyllotaxis_tuning::Variant::Harmonic { 0 } else { 1 };
        by_variant[idx].0 += crest;
        by_variant[idx].1 += flux;
        by_variant[idx].2 += 1;

        println!(
            "{:<10} {:>8} {:>8} {:>8.2} {:>9.4}",
            roster_name(a, v), partials, beating, crest, flux
        );
    }

    println!();
    for (i, name) in ["I  (harmonic)", "II (golden)  "].iter().enumerate() {
        let (c, f, n) = by_variant[i];
        if n > 0 {
            println!("{name}  mean crest {:.2}   mean flux {:.4}", c / n as f64, f / n as f64);
        }
    }
}
