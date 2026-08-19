//! Print the computed scale for every algorithm, against the 12-TET grid.
//!
//! `cargo run --release -p phyllotaxis-tuning --example tables -- [index]`

use phyllotaxis_tuning::{tables, DEGREES_PER_SCALE};

fn main() {
    let index: f64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(4.0);

    println!("index {index:.1}   —   {DEGREES_PER_SCALE} degrees, cents above the root");
    println!("{:-<78}", "");

    for (algorithm, variant, tuning) in tables(index) {
        let label = format!("{} {}", algorithm.name(), variant.numeral());
        let degrees: Vec<String> = tuning
            .degrees()
            .iter()
            .map(|d| format!("{:>6.0}", d.cents))
            .collect();
        let drift = tuning
            .degrees()
            .iter()
            .map(|d| d.detune_from_12tet())
            .fold(0.0f64, f64::max);
        let width = 6 * DEGREES_PER_SCALE;
        println!(
            "{label:<9} {:<width$}  {:<5}  drift {drift:>3.0}c",
            degrees.join(""),
            tuning.kind().name(),
        );
    }
}
