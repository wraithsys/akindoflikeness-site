# PHYLLOTAXIS — design

A polyphonic web instrument. Rust compiled to WASM for the DSP, JS and WebGL
for the surface. Every structural number in it is Fibonacci-derived, and the
tuning is computed from the timbre rather than assumed.

Settled over a whiteboard session, 2026-08-18. Billy is design lead; this file
is the working spec the implementation follows. Where a decision has a reason,
the reason is written down — if a number here feels arbitrary, that is a
documentation bug.

---

## 0. The two sentences

**The hook:** a synth that retunes its scale to fit its own spectrum, so the
more inharmonic it gets, the more consonant it sounds.

**The thesis:** a web of decisions that lets someone discover and play, rather
than generate.

Everything below serves one or the other.

---

## 1. Stack

| layer | choice |
|---|---|
| DSP | Rust → WASM, C-ABI exports, no `wasm-bindgen` glue on the audio path |
| Audio host | `AudioWorklet`, 128-frame render quantum |
| Surface | JS + WebGL |
| Node | build tooling only — it does not ship |

### WASM constraints that shape the code

These are not deployment details; they change how the DSP is written.

- **No flush-to-zero.** WebAssembly implements IEEE 754 strictly, so subnormals
  are *required* rather than flushed. The FTZ/DAZ mode that makes this free on
  x86 does not exist here. Every IIR feedback path — the plate's diffusers, the
  SVF's state — needs explicit denormal prevention or its tail costs an order
  of magnitude more CPU than its head.
- **No `fetch` in `AudioWorkletGlobalScope`.** Compile the `WebAssembly.Module`
  on the main thread, pass it over `port.postMessage` (it is structured-
  cloneable), instantiate synchronously inside the worklet.
- **No allocation in `process()`.** Preallocated linear memory, fixed scratch
  buffers, C-ABI entry points. 128 frames a call.
- **Visualiser data** wants a SharedArrayBuffer ring, which requires COOP/COEP
  headers — available on Cloudflare Pages via `_headers`, the same deployment
  `akindoflikeness-site` already uses. Without them, fall back to transferables
  every 4–8 blocks rather than one message per quantum.
- `AudioContext` needs a user gesture to start. Web MIDI is absent in Safari.

---

## 2. The Fibonacci Net — voice architecture

The Net is not a component. It is the name for the whole voice architecture:
Fibonacci-derived sequences, ratios and integers used to make complex voices.
Tuned for pads — context, not a limitation.

### The counts are consecutive Fibonacci

| count | value |
|---|---|
| unison per voice | **2** |
| operators | **3** |
| voices | **5** |
| algorithms | **8** |

### The 8 algorithms — 4 modulation types × 2

**FM I** — 2 modulators → 1 carrier. One modulator's ratio is free; the other
sits at a Fibonacci relationship to the free one, so the Fibonacci *index* is a
single control sweeping from consonant to maximally inharmonic.

**FM II** — 1 modulator → 1 feedback operator → 1 carrier.

**RM I/II, AM I/II, Rect I/II** — the complex modulation types. Each is 1
carrier, 1 modulator, 1 sub. The two modes per type differ by ratio.

#### Ratio pairs come from opposite ends of the convergents

The Fibonacci convergents 2/1, 3/2, 5/3, 8/5, 13/8 … converge on φ. **Adjacent
terms are too close to be two algorithms**: 5/3 (1.667) against 8/5 (1.600) is
a 4 % difference, which at audio rates reads as a tuning error, not a timbre.

So each pair takes a low term and a high term:

- **I — harmonic.** A low convergent (2/1, 3/2). Sum and difference tones land
  on musical intervals.
- **II — golden.** A high convergent (13/8, 21/13), effectively φ. φ is the
  most irrational number by continued fraction, so the partials are maximally
  inharmonic.

Same axis, two ends, and the numbering explains itself.

#### Rect's pair cannot differ by ratio alone — found in implementation

Full-wave rectification of a sine emits a harmonic series *scaled by the source
ratio*, so its internal structure is ratio-independent. Two full-wave modes at
different ratios are one timbre transposed, and a dissonance curve — a function
of interval — cannot tell them apart: **measured at 0.02 ¢**, which is not an
algorithm.

So **Rect I is half-wave and Rect II full-wave**, each still at its own end of
the convergents. Half-wave keeps the fundamental (`1/π + ½·sin + even
harmonics`) where full-wave discards it, which is a difference in content rather
than a transposition. The I/II reading survives; only the mechanism separating
them changes.

The other three pairs are unaffected — RM, AM and FM all place partials at
`c ± m`, so their internal structure moves with the ratio as intended.

### Consequences to build around

- **Rect emits DC.** Full-wave rectification doubles the fundamental and leaves
  an offset; half-wave leaves a larger one. A DC blocker goes before the bus, or
  the plate loses headroom and carries the offset through the whole tail.
- **The FM modes have no sub.** Six algorithms have a low anchor and two do not.
  Gain-match per algorithm or the algorithm selector doubles as a volume control.
- **Complex modulation chorus for free.** RM and AM produce sum and difference
  tones; with 2-voice unison detune, each voice's difference tones land in a
  different place. Set the unison detune knowing this — small values do more
  than expected, before the hyperchorus does anything.

---

## 3. Tuning — scales computed from the spectrum

The headline feature, and the one that needs building first because everything
downstream reads its tables.

**Principle.** Consonance is a property of the relationship between a timbre and
a tuning, not of the tuning alone. 12-TET sounds consonant because it
approximately aligns with the partials of *harmonic* spectra. Our operators are
φ-tuned, so their partials are maximally inharmonic — which makes 12-TET the
mismatched choice, and a scale derived from the actual spectrum more consonant
while looking wrong written down.

**Method.** Plomp–Levelt sensory-dissonance curves, as developed in William
Sethares, *Tuning, Timbre, Spectrum, Scale*. Given a spectrum, the scale is the
set of local minima of the dissonance curve.

**Per algorithm the partials are known in closed form:**

| algorithm | partials |
|---|---|
| FM | `|c ± n·m|`, amplitudes `J_n(I)` |
| RM | `c ± m` |
| AM | `c`, `c ± m` |
| Rect (full-wave) | even harmonics of the modulator, plus DC |

Sweep the interval ratio, sum pairwise dissonance across those partials, take
the minima. **8 algorithms × 2 ratio settings = 16 scale tables, computed
offline and baked.** Zero runtime cost.

**The scale tracks INDEX.** As `I` rises, `J_n(I)` brings more sidebands up, the
dissonance curve reshapes, and the minima move. Precompute degree positions at
a handful of index points and interpolate — a control-rate lookup, affordable
even if recomputed at chord-change rate.

### Degrees are taken greedily, with a floor on their spacing

Minima are ranked by curve value — a shallow dip at genuinely low dissonance is
a usable interval, a deep dip out of a rough region is not — and then accepted
only if no degree already sits within **50 ¢**, half a semitone.

The floor is not tidiness. An inharmonic spectrum's curve often falls away
toward the octave rather than dipping cleanly, and depth-ranking alone then
returns a cluster: the golden FM spectrum yielded six degrees inside 57 ¢, which
is one degree and five mistunings of it. It is the same argument as the variant
pair in §2 — a difference below this is heard as a tuning error, not a step.

### Sparse spectra are chords, and say so

RM and AM produce three partials (`c ± m`, plus the sub). Three partials collide
in few places, so the curve is shallow everywhere and the method returns **two
or three degrees**, against seven or eight for the FM algorithms.

That is not a degenerate scale to be padded out. A sparse spectrum genuinely has
little to collide, which is the same as saying almost every interval is
permitted — and the handful of minima it does have are the intervals it wants
**sounded together**. So a tuning below five degrees is a **chord**, Cadence
voices it as one, and it is never stepped through.

Five is the threshold because it is the voice count: a tuning with fewer degrees
than the instrument has voices can be sounded all at once, and one with more
cannot.

**The distinction is carried in the type, not inferred at each call site.**
`Tuning::kind()` returns `Scale` or `Chord` and the caller has to ask — a caller
that forgot which it held would silently arpeggiate a chord, and that failure
would be inaudible as a bug and merely disappointing as music.

As computed at index 4: FM I, FM II, FM-fb I, FM-fb II and both Rect modes are
scales; RM I/II and AM I/II are chords. `rm II` is a bare major third, `am II` a
third, minor sixth and octave. Those are the intervals ring modulation at the
golden ratio actually wants, and the instrument now plays them rather than
pretending it has a scale.

### Auditioned, 2026-08-19 — confirmed by ear

Rendered with `examples/audition.rs`: the same chord, the same timbre, played
first in the computed tuning and then in the nearest 12-TET, so nothing changes
but the tuning. Billy's verdict on the roster, `fm II` carrying the largest
drift at 32 ¢:

> *"ghostly — atmospheric — but also capable of harmonic precision … those wavs
> made me way more confident in the design than I was when I first thought of
> it."*

So §3 stops being the headline claim and becomes the confirmed core: an
inharmonic spectrum played in a scale computed from itself is **more** consonant
than the same spectrum on the grid, and it is audible without training or
prompting. Everything downstream can be built on it.

**One honest caveat about what was heard.** The audition is a bare additive sine
bank — no filter, no plate, no hyperchorus, no unison, no field movement. The
ghostliness may be partly the *purity* of that path rather than the tuning, and
the full signal chain could bury it. Keep a route back to it: filter wide open,
plate low, unison off should reproduce the audition's thinness, and that should
be a preset rather than a coincidence.

**Applied at chord change, not to held notes.** The tuning drifts while INDEX
moves; retuning a held pad mid-note is worse than letting the next chord land in
the new scale. Glide covers the seam.

---

### Why this sounds nothing like Blow Your Phase Off

Billy, on first hearing the renders: *"it sounds really different to the other
fibonaccis too — I knew it would, the design takes heavy detours, but I didn't
think it would be that different."*

The reason is one decision, and stating it makes the family coherent rather than
merely adjacent. **Both instruments use the same mathematics and take opposite
positions on it.**

BYPO tunes its operators to φ — maximally inharmonic partials — and then plays
notes on a grid those partials do not fit. The mismatch is not a shortcoming, it
*is* the instrument: the partials grind against the pitches, and that grinding is
the phase violence the whole thing is named for.

Phyllotaxis computes the scale from those same partials, so they land where the
notes are. Identical inharmonicity, deliberately met instead of deliberately
refused — and it comes out consonant, spacious and ghostly rather than brutal.

So the pair is not two synths that share a number. It is one idea and its two
answers: **what golden-ratio inharmonicity sounds like when you refuse to tune to
it, and what it sounds like when you do.** Nothing in either instrument needs to
change for that to be true; it already is. It just needed hearing.

## 4. Cadence — the harmony engine

Outputs chords. The pitch source is not new work: `fibonacci-dsp/src/melody.rs`
in `wraithsys/fibonacci-synth` already implements the golden Weyl sequence
`x ← (x + 1/φ) mod 1`, eight scales and five tunings, deterministic and
allocation-free. Cadence is that source plus a harmonisation layer.

### Hold source — one axis, not two switches

```
x = frac(n/φ + ε · u)        u ∈ [−0.5, 0.5], fresh per fire
```

ε = 0 is the pure golden rotation: equidistributed, aperiodic, cannot clump.
ε ≈ 0.5 is indistinguishable from random. Between them, golden coverage with a
random feel. This replaces the two-way `HoldSource` switch with one control.

Equidistribution matters more for chord roots than it did for melody notes: a
repeated root does not read as variety, it reads as the progression stalling.

**Pitch and time get separate ε.** And neither should default to zero — on a pad
a repeated root is a pedal tone, not a fault.

### Mirror — both meanings, they are orthogonal

- **Negative harmony**, acting on pitch: reflect each pitch about the axis
  midway between tonic and dominant. Swaps major and minor, inverts the pull of
  a cadence. One bipolar control: 0 as generated, 1 fully reflected, between
  them a per-chord probability. Against the existing dark-scale roster —
  Phrygian, Byzantine, Neapolitan — the reflections land somewhere genuinely
  strange.
- **The Fibonacci word**, acting on time: `S(n) = S(n−1) S(n−2)` →
  `0100101001001…`, a Sturmian sequence, self-similar and non-periodic. It
  decides *when* the chord changes, not what it is.

Neither knows about the other, so they compose without conflict.

### Strum

Bipolar offsetting of note events, ordered by pitch:

| bias | behaviour |
|---|---|
| **−1** | ascending — lowest note first, highest offset most |
| **0** | block chord, all notes simultaneous |
| **+1** | descending — highest note first, lowest offset most |

- **Applies to note-offs as well, mirrored.** Five staggered releases is how a
  real instrument lets go of a chord; five simultaneous ones is a gate closing.
- **Gaps are φ-spaced**, each successive gap × 1/φ, so the strum accelerates
  rather than arriving metronomically. That is most of what makes a strum sound
  played rather than sequenced.
- **The span must be in the same order as the gesture.** Guitar strums are
  10–30 ms; against pad-length arrivals, a spread that short is inaudible — the
  notes blur into one swell. Scale the span against the gesture, not against a
  fixed millisecond range.

### Voicing

**Nearest-note voice leading.** Hold common tones, move each remaining voice the
shortest distance. Root-position triads jumping around lurch badly across five
sustained voices. It also composes with the strum: once voices are led rather
than reset, strum offsets apply only to the notes that actually changed — and
with prediction (§6) a chord change holding two common tones only allocates two
new voices.

---

## 5. The field — amplitude, and no envelope

**There is no ADSR and no Envelope panel.** The argument is already written in
`fibonacci-synth/crates/fibonacci-dsp/src/breath.rs`: *"a fixed decay cannot
work across the sequencer's whole range: at 8 Hz the notes are 125 ms apart, and
a boost falling over ~1 s is re-triggered long before it has moved."* An
envelope with fixed times is wrong whenever the rate is generated rather than
played — which here it always is. Four knobs that need re-dialling every time
Cadence changes rate is a panel spent on a liability.

Amplitude *moves* instead. The field is always on, and a note is not a shape —
it is the field moving more for a while.

### Carried over from `breath.rs`

- **Floor as a ratio.** `floor + (1 − floor)·amount` never falls below the knob
  and still fills the whole range at floor 0. Target-independent reasoning.
- **Gesture length is the golden section of the interval.** `gesture = interval/φ`,
  rest = `interval/φ²`. The pad breathes in time with the harmony and no tempo
  control exists anywhere in the instrument.
- **`curve`** — one control, logarithmic through linear to exponential over a
  φ-power exponent. The measured behaviour table at `breath.rs:105` stands.
- **Rate from frequency.** `freq/φ¹³` per voice, so the top of a chord breathes
  faster than the bottom. In a drone that was a physical detail; in a five-voice
  chord it is free internal movement, and it is what stops a held pad sounding
  like a single object.

### Reworked for polyphony

1. **Floor is no longer the master.** In BYPO they are one knob because the
   instrument only ever sounds. Here master is a real master, and floor becomes
   a per-voice rest depth. The invariant holds *per voice while the note is
   held*; ending a voice is the allocator's job, not the field's.
2. **The interval is fed, not measured.** Cadence knows the chord-change
   interval exactly, so the derivation stops being an estimate. The measurement
   path in `begin()` stays for hand-played input.
3. **ATTACK is a fraction of the gesture, never a time in milliseconds.**
   `breath.rs` has no rise at all — `begin()` sets `boost_level = 1.0` and
   `curve` shapes only the fall. Adding a rise in absolute time would reintroduce
   exactly the failure the file documents for decay. As a fraction it inherits
   the interval-derived length: 0 is the current instantaneous strike, 1 is a
   gesture that is all swell and arrives as the next chord does, and it cannot
   outrun the gesture, so "attack longer than the note" is not a reachable state.
   The name is for the audience; the behaviour is honest — it does control how
   fast the sound arrives, it simply is not reaching a sustain level, because
   there is not one.
4. **Per-voice phase offset `frac(n/φ)`.** Five voices running one signature in
   phase pump as a single object — the inverse of the failure `breath.rs` opens
   with. Offset each voice's field phase and the chord breathes as five bodies.
5. **Eight signatures, one per algorithm**, extending the Fibonacci / Lucas /
   Padovan families already in `signature()`. Preserve the property deliberately:
   exactly one signature is commensurate, and it is the only movement that
   repeats. The mode you would expect to be periodic should stay the only
   periodic one.
6. **`release()` becomes load-bearing.** `breath.rs:277` notes the sequencer
   never calls it, because S&H has no note-offs. Cadence does — every new chord
   ends the last one — so `RELEASE_BOOST` (1/φ) now fires constantly and needs
   tuning against the strum rather than being an edge case.

---

## 6. Voice allocation — predictive

The field is a closed form, not a sampled signal: the boost decays from a known
start over a known `gesture_s()` with a known curve, and the always-on component
is five sinusoids at known rates and phases. **Any voice's amplitude at any
future moment can be evaluated analytically** — no lookahead buffer, no
per-sample bookkeeping.

1. **Steal by predicted level, not current level.** "Quietest now" is wrong
   about half the time: a quiet voice may be climbing out of its trough while a
   louder one falls into its own. The strum says exactly when the incoming note
   needs the slot, so the question is which voice is quietest at
   `t + strum_offset`.
2. **Schedule the steal.** Inside the strum's time budget, steal at the victim's
   next amplitude minimum. A steal at the bottom of a voice's own breath is
   inaudible before any fade applies; the fade is insurance for when no trough
   falls inside the window.
3. **Inherit the level.** The incoming voice starts its rise from the outgoing
   voice's current amplitude, which is known exactly. The slot's level never
   steps — continuity by construction, the same shape of argument as the floor.
   This is envelope continuity, not waveform continuity: the new voice is at a
   different pitch and phase, so a 1–2 ms crossfade (or a phase-continuous
   restart) still hides the signal edge — but it no longer has to hide a level
   drop, so it is an order of magnitude shorter than it would otherwise be.
4. **Decline to allocate** when an incoming note's contribution would sit under
   the masking threshold of what is already sounding plus the tail.

Priority: released voices before held ones, then maximum masking margin. All of
it is a handful of closed-form evaluations over five voices, once per note-on,
at control rate.

**Five voices is not tight.** The plate carries a stolen voice's energy after
the voice is gone, voice leading holds common tones, and prediction declines
work that would be masked.

---

## 7. Filter

**One topology: a state-variable filter.** The SEM *is* an SVF — a 12 dB
multimode with LP and HP blended to a notch at the midpoint and a separate BP
tap — so "SVF or SEM" is one filter with a mode blend, not two characters.

If a genuinely second character is wanted later, pair the SVF with a ladder or a
diode/Sallen-Key, not with itself.

Denormal prevention on the state variables is mandatory (§1).

---

## 8. FX — where the sequence gets spent

The board spends Fibonacci *ratios* on tuning and Fibonacci *integers* on the
counts. The sequence itself pays off here.

**Plate reverb.** Diffusion and comb delay lengths are **consecutive Fibonacci
sample counts**. Consecutive Fibonacci numbers are always coprime —
`gcd(F(n), F(n+1)) = 1`, and not by coincidence: the Fibonacci recursion is
precisely the worst case for the Euclidean algorithm. Coprime lengths are what a
diffusion network wants — echoes that never coincide, so the tail stays dense
instead of developing flutter.

The caveat matters: in general `gcd(F(m), F(n)) = F(gcd(m, n))`, so F(6)=8
against F(9)=34 shares a factor of 2. **Use adjacent terms, or terms at coprime
indices.**

**Noise modulation** on the plate, which is what keeps a plate from sounding
static.

**Hyperchorus.** LFO rates are φ-spaced, therefore incommensurate, therefore the
modulation never re-aligns and the chorus never develops an audible pulse. For
sustained material that is the difference between a chorus you can leave on and
one you cannot.

**The plate is a bus, not per-voice.** A stolen voice's send has already
happened, so its energy persists in the tail after the voice is gone. This is
why §6 works.

---

## 9. Surface

Three columns, from the board:

| left | centre | right |
|---|---|---|
| Title | Presets | Presets |
| Fib Engine | **Visualisation** | **FIELD** |
| Cadence | Transport | Filter |
| | Key / Scale | Fx |

The freed Envelope slot becomes **FIELD**: floor, depth, curve, attack, and a
readout of the active signature. One control fewer than an ADSR would have
taken, in the same space.

### 1-bit in WebGL

The house language is two colours and dithering, and WebGL will not give that by
default. No anti-aliasing, nearest filtering, integer scaling, geometry snapped
to device pixels, a fixed logical resolution multiplied by an integer factor.
Anything else and it will not read as the same family as the desktop
instruments.

### The default patch

It is doing more work here than in any of the desktop builds, because people
arrive at a web page without commitment and give it ten seconds. That is not an
argument to make it tame — it is an argument to make the default **the most
obviously beautiful thing the engine does, rather than the most representative.**
Show off first; the alien settings are found by people who have already decided
to stay.

`curve` at its logarithmic end must still produce something that reads as a slow
swell, so the first thing a stranger does yields a pad rather than confusion.

---

## 10. Determinism, presets, and the missing button

Every "random" choice in the engine is deterministic and reproducible — the
property `melody.rs` already states as *"same settings, same melody, forever."*

That is not a nicety, it is the mechanism behind the thesis. A generator hands
you a different thing each time and you accept it or reroll. A deterministic
system with a large state space is a **place**: it stays where you left it, so
exploration accumulates instead of evaporating.

Two consequences:

- **Presets are coordinates, not snapshots.** If the whole state reconstructs
  from its parameters, a preset is a short tuple — which on the web is a URL
  fragment. Someone finds something, sends a link, and the other person hears
  the identical instrument. No server, no accounts, no preset file format. None
  of the desktop builds can do this; this one gets it nearly free.
- **There is no randomise button.** It is the generate verb wearing a hat. The
  golden rotation is the better answer already: a walk that covers the space
  without repeating, with a direction and a history. The control is **STEP**,
  forward and back — so you can always return to where you were, which is what
  makes it exploring rather than rerolling.

---

## 11. Build order

1. Scale tables — dissonance curves per algorithm, offline, baked. Everything
   downstream reads them.
2. Voice: 3 operators, the 8 algorithms, the ratio pairs.
3. The field, reworked from `breath.rs` for polyphony.
4. Predictive allocation.
5. Cadence: hold source, harmonisation, voicing, strum.
6. FX bus: plate, hyperchorus.
7. WASM boundary and the worklet.
8. Surface.

Steps 1–6 are testable offline against rendered WAVs, bit-deterministically, the
way `fibonacci-synth` already does it. The browser is the last thing that has to
work, not the first.

---

## Implementation status

`crates/phyllotaxis-tuning` — **built and tested.** Bessel sidebands, the four
partial generators, Plomp–Levelt dissonance, and scale extraction, with 37 tests
covering the method rather than the arithmetic: a harmonic spectrum must return
the fifth, fourth and octave to within 8 ¢; a φ-tuned one must leave the 12-TET
grid; the two variants of every algorithm must disagree by more than 5 ¢.

`cargo run --release -p phyllotaxis-tuning --example tables` prints the roster.
At index 4:

```
fm I      0  498  702  814  884  969 1049 1200   scale
fm II     0  834  884  943 1018 1068 1119 1200   scale
fm fb I   0  435  583  637  782  884 1018 1072   scale
fm fb II  0  561  720  834  894  983 1101 1160   scale
rm I      0  813 1200                            chord
rm II     0  386                                 chord
am I      0  813 1200                            chord
am II     0  386  814 1200                       chord
rect I    0  498  702  884 1200                  scale
rect II   0  386  498  583  702  884  969 1200   scale
```

`fm I` returning 498, 702, 884 and 969 — the just fourth, fifth, sixth and
septimal seventh — is the method working: a harmonic-ratio spectrum recovers the
intervals music already uses, without being told about them.

## Open

- The eight field signatures — which families beyond Fibonacci, Lucas and
  Padovan, and which one stays commensurate.
- Whether STEP walks the whole state or only the Cadence seed.
- Where the instrument's real name comes from. `phyllotaxis` is the workspace,
  the way `fibonacci-synth` is the workspace for Blow Your Phase Off.
