//! Render the instrument to stereo WAVs, for listening to rather than testing.
use phyllotaxis_wasm::{param, Engine, QUANTUM};
use std::io::Write;

const SR: f32 = 48_000.0;

fn wav(path: &str, l: &[f32], r: &[f32]) {
    let n = l.len().min(r.len());
    let data = n * 2 * 2;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    f.write_all(b"RIFF").unwrap();
    f.write_all(&((36 + data) as u32).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&(SR as u32).to_le_bytes()).unwrap();
    f.write_all(&((SR as u32) * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&(data as u32).to_le_bytes()).unwrap();
    for i in 0..n {
        for s in [l[i], r[i]] {
            f.write_all(&(((s.clamp(-1.0, 1.0)) * 32767.0) as i16).to_le_bytes()).unwrap();
        }
    }
}

fn render(entry: u32, secs: f32, tweak: impl Fn(&mut Engine)) -> (Vec<f32>, Vec<f32>) {
    let mut e = Engine::new(SR);
    e.set(param::ENTRY, entry as f32);
    tweak(&mut e);
    e.install_for_entry(entry);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for _ in 0..((SR * secs) as usize / QUANTUM) {
        e.process_stereo(QUANTUM);
        l.extend_from_slice(e.left());
        r.extend_from_slice(e.right());
    }
    (l, r)
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "renders".into());
    std::fs::create_dir_all(&out).unwrap();
    let names = ["fm-I", "fm-II", "rm-I", "rm-II", "am-I", "am-II", "rect-I", "rect-II"];
    for (i, name) in names.iter().enumerate() {
        let (l, r) = render(i as u32, 45.0, |_| {});
        let p = format!("{out}/{:02}-{name}.wav", i);
        wav(&p, &l, &r);
        println!("{p}");
    }
}
