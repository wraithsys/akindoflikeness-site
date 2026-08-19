// Where a tuning is actually worked out.
//
// Reading a scale off a spectrum means sweeping a dissonance curve — 6300
// samples, each summing every pair of partials between two spectra. For `fm I`
// that measured **one second**. It is not a real-time operation and it must
// not be on a real-time thread: it used to run inside phy_step, on the audio
// thread, and the instrument appeared to freeze and refuse to change entry.
//
// It is not on the UI thread either. A second of unresponsive page on every
// STEP is a different bug with the same cause, so the work sits here, and the
// instrument keeps sounding in its old tuning until the new one arrives.
//
// This worker holds its own instance of the same module. The module is
// single-threaded and its tuning scratch buffer belongs to whichever instance
// owns it, so the worklet's copy and this one never touch.

let wasm = null;

self.onmessage = (e) => {
  const m = e.data;
  try {
    if (m.type === "init") {
      wasm = new WebAssembly.Instance(new WebAssembly.Module(m.bytes), {}).exports;
      self.postMessage({ type: "ready" });
    } else if (m.type === "derive" && wasm) {
      const n = wasm.phy_compute_tuning(m.entry, m.index);
      const cap = wasm.phy_tuning_cap();
      const view = new Float32Array(wasm.memory.buffer, wasm.phy_tuning_ptr(), cap);
      self.postMessage({
        type: "tuning",
        entry: m.entry,
        index: m.index,
        cents: Array.from(view.subarray(0, n)),
        isChord: wasm.phy_computed_is_chord() !== 0,
      });
    }
  } catch (err) {
    self.postMessage({ type: "error", message: String((err && err.stack) || err) });
  }
};
