// The audio thread.
//
// Everything here runs on a real-time thread, so it does three things and no
// others: build the module, call into it, copy the block out.
//
// AudioWorkletGlobalScope has no fetch, so the wasm has to arrive over
// port.postMessage. It arrives as **bytes**, not as a compiled
// WebAssembly.Module — and that distinction cost an afternoon.
//
// A WebAssembly.Module IS structured-cloneable and posting one to a Worker
// works. Posting one to an AudioWorklet's port does not: the message is
// **silently discarded**. No exception on the sender, no error on the
// receiver, no console warning — the port keeps working and every later
// message arrives normally, so it looks exactly like a handler that never
// ran. Measured directly: sending {plain}, {module}, {after-module} in order
// delivers the first and the third.
//
// So the main thread sends the ArrayBuffer and compilation happens here, with
// the synchronous constructor. The 4 KB limit on synchronous compilation
// applies to the main thread only, and this module is 146 KB.
//
// It imports nothing, so the import object is empty and there is no glue to
// construct or keep alive.

class PhyllotaxisProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.engine = 0;
    this.wasm = null;
    this.viewL = null;
    this.viewR = null;
    this.running = false;

    // A throw inside this handler is invisible: onprocessorerror only covers
    // process(), and an exception here just stops the instrument arriving with
    // no message anywhere. So it reports itself.
    this.port.onmessage = (e) => {
      try {
        this.handle(e.data);
      } catch (err) {
        this.port.postMessage({ type: "error", where: e.data && e.data.type, message: String(err && err.stack || err) });
      }
    };
  }

  handle(msg) {
    {
      if (msg.type === "init") {
        const instance = new WebAssembly.Instance(new WebAssembly.Module(msg.bytes), {});
        this.wasm = instance.exports;
        this.engine = this.wasm.phy_new(sampleRate);
        this.quantum = this.wasm.phy_quantum();
        this.outPtr = this.wasm.phy_out_ptr(this.engine);
        this.outRPtr = this.wasm.phy_out_r_ptr(this.engine);
        this.scopePtr = this.wasm.phy_scope_ptr(this.engine);
        this.scopeLen = this.wasm.phy_scope_len();
        this.running = true;
        this.port.postMessage({ type: "ready", scopeLen: this.scopeLen });
      } else if (msg.type === "set" && this.engine) {
        this.wasm.phy_set(this.engine, msg.id, msg.value);
      } else if (msg.type === "step" && this.engine) {
        this.wasm.phy_step(this.engine, msg.delta);
        this.port.postMessage({ type: "entry", value: this.wasm.phy_get(this.engine, 0) });
      } else if (msg.type === "tuning" && this.engine) {
        // Deriving a tuning costs up to a second. It is done on the main
        // thread and arrives here as a short list of cents, which is a copy
        // and a reflection table to install — safe on a real-time thread.
        //
        // This used to happen inside phy_step, on this thread, and the
        // instrument appeared to freeze and refuse to change entry.
        const cap = this.wasm.phy_tuning_cap();
        const n = Math.min(msg.cents.length, cap);
        const scratch = new Float32Array(this.wasm.memory.buffer, this.wasm.phy_tuning_ptr(), cap);
        for (let i = 0; i < n; i++) scratch[i] = msg.cents[i];
        this.wasm.phy_install_tuning(this.engine, n, msg.isChord ? 1 : 0);
      } else if (msg.type === "scope" && this.engine) {
        // The visualiser asks; it is not pushed at. A copy per animation frame
        // is cheap; a copy per 128-sample block would be 375 a second.
        const head = this.wasm.phy_scope_head(this.engine);
        const all = new Float32Array(this.wasm.memory.buffer, this.scopePtr, this.scopeLen);
        const out = new Float32Array(msg.count);
        for (let i = 0; i < msg.count; i++) {
          out[i] = all[(head - msg.count + i + this.scopeLen) % this.scopeLen];
        }
        this.port.postMessage({ type: "scope", samples: out }, [out.buffer]);
      } else if (msg.type === "stop" && this.engine) {
        this.running = false;
      }
    }
  }

  process(inputs, outputs) {
    const out = outputs[0];
    if (!this.running || !this.wasm) return true;

    const frames = out[0].length;
    this.wasm.phy_render(this.engine, frames);

    // Re-take the views if the buffer was detached. It should not be — nothing
    // allocates after phy_new — but a detached view fails silently and
    // silently is the worst way for audio to fail.
    if (!this.viewL || this.viewL.buffer !== this.wasm.memory.buffer) {
      this.viewL = new Float32Array(this.wasm.memory.buffer, this.outPtr, this.quantum);
      this.viewR = new Float32Array(this.wasm.memory.buffer, this.outRPtr, this.quantum);
    }

    // Two blocks, two channels. This used to render ONE block and copy it to
    // every channel — which is how a plate and a chorus, the two effects here
    // whose entire job is width, ended up as a point source.
    out[0].set(this.viewL.subarray(0, frames));
    if (out.length > 1) {
      out[1].set(this.viewR.subarray(0, frames));
      // A surface asking for more than two channels gets the pair repeated.
      for (let c = 2; c < out.length; c++) out[c].set(c % 2 ? this.viewR.subarray(0, frames) : this.viewL.subarray(0, frames));
    }
    return true;
  }
}

registerProcessor("phyllotaxis", PhyllotaxisProcessor);
