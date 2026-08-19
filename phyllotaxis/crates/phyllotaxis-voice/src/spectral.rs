//! Measuring what the voice actually emits.
//!
//! This exists for one test, and that test is the contract between this crate
//! and `phyllotaxis-tuning`: **every partial the model predicts must come out
//! of the DSP.** The scales were computed from those predictions, so if the two
//! disagree, the instrument is tuned to a spectrum nothing produces.
//!
//! Test-and-tooling only. Nothing here runs on the audio path.

use core::f32::consts::TAU;

/// In-place iterative radix-2 FFT. `re`/`im` must be a power of two long.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two(), "radix-2 needs a power of two");

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (vr, vi) = (
                    re[i + k + len / 2] * cr - im[i + k + len / 2] * ci,
                    re[i + k + len / 2] * ci + im[i + k + len / 2] * cr,
                );
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
                let _ = k;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Magnitude spectrum of a real signal, Hann-windowed.
///
/// Hann rather than rectangular because an FM voice's partials are not
/// bin-centred and rectangular leakage would smear a weak partial into
/// invisibility — which would fail the contract for the wrong reason.
pub fn magnitude(samples: &[f32]) -> Vec<f64> {
    let n = samples.len();
    let mut re: Vec<f64> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 - 0.5 * ((TAU * i as f32) / n as f32).cos();
            (s * w) as f64
        })
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    (0..n / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect()
}

/// Is there a local maximum within `tolerance` bins of `bin`, at least
/// `floor` of the spectrum's peak?
pub fn peak_near(mag: &[f64], bin: usize, tolerance: usize, floor: f64) -> bool {
    let peak = mag.iter().cloned().fold(0.0f64, f64::max);
    if peak <= 0.0 {
        return false;
    }
    let lo = bin.saturating_sub(tolerance);
    let hi = (bin + tolerance).min(mag.len() - 1);
    (lo..=hi).any(|k| mag[k] / peak >= floor)
}
