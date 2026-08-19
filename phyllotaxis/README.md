# PHYLLOTAXIS

A polyphonic web instrument: Rust compiled to WASM for the DSP, JS and WebGL for
the surface, and scales computed from each algorithm's own spectrum rather than
assumed to be 12-TET.

`DESIGN.md` is the spec. It lives here rather than in its own repository because
this is where the instrument will eventually be served from, beside `bypo.html`.

## Run

```sh
cargo test                                    # the tuning method
cargo run --release -p phyllotaxis-tuning --example tables [index]
```

`tables` prints what each of the eight algorithms wants to be played in:

```
fm I      0  498  702  814  884  969 1049 1200   scale
rm II     0  386                                 chord
```

`fm I` returning 498, 702, 884 and 969 — the just fourth, fifth, sixth and
septimal seventh — is the method working. Nobody told it about those intervals;
they are where a harmonic-ratio spectrum stops grating against itself.

## Crates

| crate | what it is |
|---|---|
| `phyllotaxis-tuning` | Bessel sidebands, partial generators, Plomp–Levelt dissonance, and tuning extraction by curve minimisation. Offline: the tables bake, and the audio thread never sees any of it. |
