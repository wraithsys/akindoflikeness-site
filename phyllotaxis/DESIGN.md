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

### The dry voice is already chorused — measured, not impressionistic

Billy, on the same bare additive render: *"there was also unison going on"* —
against a file that applies no unison, chorus or reverb anywhere. He is right,
and the reason matters more than the observation. (The rest of that message was
him saying he can identify those characteristics unaided and did not need the
caveat, not a list of what he had found in the file. The finding below stands on
its own measurement either way.)

Sound a chord of inharmonic spectra and the partials of *different notes* land
near each other. Near coincidences beat, and beating at a few Hz is what chorus
is. `examples/coincidences.rs` counts them, over the same four-note chords the
audition renders:

| algorithm | partials sounding | exact unisons | chorusing pairs |
|---|---|---|---|
| fm I | 80 | 27 | 0 |
| **fm II** | **559** | **119** | **354** |
| fm fb I | 68 | 6 | 1 |
| fm fb II | 132 | 2 | 15 |
| rm I / rm II / am I | 9 / 6 / 9 | 1 | 0 |
| am II | 16 | 5 | 1 |
| rect I / rect II | 40 / 36 | 7 / 8 | 5 / 3 |

The file sent as the headline test, `fm II`, arrives with **354 beating pairs and
119 exactly-doubled partials before a single effect is applied.** That is the
chorus and the unison, heard correctly and named correctly. The "reverb" is the
same fact from another angle: 559 partials at slightly differing frequencies is a
dense decorrelated field, which is perceptually what a reverb produces.

**The I/II pair separates here too, and audibly.** fm I has 27 exact coincidences
and *zero* beating — harmonic ratios make partials land exactly on each other. fm
II has 119 exact and 354 beating. So "I = harmonic, II = golden" is not only a
tuning distinction: one variant reinforces and the other shimmers. That was hoped
for in §2 and is now a number.

### Consequence: the player is told, never compensated for

§8 was written as though it were treating a dry signal, and it is not — `fm II`
arrives with 354 beating pairs while `rm I` sits at nine partials and is bone dry.

~~**Proposed and withdrawn the same day: derive FX depth from the algorithm's
partial count**, so a sparse algorithm gets the full plate and a dense one gets
almost none.~~ Withdrawn by Billy, and the reason is worth more than the idea:

> *"I'd say let people drive that — it's too contextual and perceptual. Like if I
> want a distorted sound out of a cleaner synth like constellation I use a
> downsampled tank, increase dampening and ride mix until I find thickening with
> a hint of that downsample grit. Was it designed for that? Absolutely not."*

The derivation contradicts this instrument's own thesis. §10 exists to make the
player **discover** rather than accept what was generated for them; an FX chain
that silently corrects for spectral density is the instrument deciding, and it
closes the misuse a player would otherwise have found. It also fails the house's
plainest rule — the player is always allowed everything. A coupling being
*legal* under Law 7 is not an argument that it should exist.

**What survives is the measurement, as information rather than as automation.**
A player cannot know by looking that one algorithm sounds 354 beating pairs and
another sounds none, and that is exactly the fact they need in order to make the
call themselves. So spectral density becomes a **readout** — drawn, not acted on.
That is not a control at all, so no law governs it, and it turns a hidden
property of the roster into something the surface says out loud.

**Density control, if it is wanted, is a signal-flow design and gets a toggle.**
Billy: *"envelope follower on the verb send and reconcile with a sidechain to
voice output and ride down decay on scheduled voice steal"* — which is a real
mechanism made of hearable parts, not a lookup keyed to the algorithm. It is
built with the FX bus (§8), behind a switch, and it stays only if it earns its
keep with the switch off as the control. Nothing about it is derived from the
roster; it responds to the signal, which is why it is allowed at all.

**One thing does still want correcting automatically, and it is a fault rather
than a character:** level. 559 partials against 9 is a large loudness difference,
and if it is left alone the algorithm selector doubles as a volume control —
already named as a defect in §2. **Normalise gain; never normalise effect.** The
line is that a fault is corrected and a character is only ever reported.

**What was originally written here** — that the ghostliness might be the purity of
an unfiltered sine path, and needed protecting as a preset — was wrong about the
cause but right about the instinct. The thinness is worth a preset; the
ghostliness is the spectrum and travels with it.

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

### Built — and the two mirrors are one mechanism

`phyllotaxis-cadence` implements the word, the mirror and the strum. Each was
derived and then adversarially re-checked before becoming code, because a wrong
test vector locks in a wrong implementation more durably than no test at all.

**The mirrors unify.** The schedule deciding which chords get reflected is
`frac(n(φ−1) + β) < m`: a Sturmian set of density exactly `m`, aperiodic, with
gaps bounded to three distinct values by the three-distance theorem, so mirrored
chords cannot clump. **At `m = 1/φ² ≈ 0.382` that schedule *is* the Fibonacci
word** — asserted symbol-for-symbol over ten thousand steps. Asking for both was
asking for one thing at a particular density.

**`0 → long` is derived, not chosen.** With `L = φ²/√5·T̄` and `S = φ/√5·T̄`, the
n-th Fibonacci block lasts exactly `S·φⁿ`. Reverse the assignment and the
identity vanishes. And it closes a ladder with §5: the gesture of a long step
**is** a short step, exactly, and the rest after a long step is the gesture of a
short one. Two articulations that are one shape at adjacent rungs.

It is also a clock rather than a rubato: every window of every length stays
within `L − S = 1/√5` of the mean, forever. Measured worst case over two
thousand steps and four hundred window lengths: 0.4467, against the bound
0.4472.

**The mirror needed a per-tuning axis, and `fm fb II` proves why.** That tuning
tops out at 1160 ¢, so its octave seam is 40 ¢ — under §3's own 50 ¢ floor. With
a plain 700 ¢ axis its reflection maps 0 → 720 → 1160 → 720 and never returns:
not an involution, therefore not a mirror. Its own near-fifth at 720 ¢ restores
it. Any implementation that ignores the seam is wrong on exactly that table, and
the involution assertion catches a bad axis, a missed seam and a wrong capture
radius in one line.

**The pedal exemption belongs at the voice level, not in the table.** Mirroring
the lowest voice moves the key centre, which is a modulation. But exempting the
tonic *inside* the reflection would send the tonic to itself while the dominant
still mapped to the tonic — a collision, and no longer an involution. So the
table reflects everything and `reflect_chord` skips the pedal.

**Strum offsets land on Fibonacci numbers.** At a 144 ms budget the four-note
ascending strum is 0, 55, 89, 110 ms with gaps 55, 34, 21 — because `1 − φ⁻ⁿ`
is a ratio of Fibonacci numbers, so a Fibonacci budget produces Fibonacci
offsets. Nothing was tuned to make that happen. The budget itself is `Δ/φ⁵`,
which is `gesture/φ⁴` — four rungs below the shape the note is making, on the
same ladder as everything else.

Still open: **voice leading**. Greedy nearest-first is provably wrong — the
derivation supplies a counterexample where it costs 480 cents against an optimal
410 and crosses two voices to do it — so it needs a real assignment, not a
sort.

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
2b. **The gesture had a ceiling on it, and the derivation was not happening.**
   Found by the Cadence verification pass, in code already built and passing
   its own tests. `breath.rs` uses `LONE_NOTE_S` (987 ms) for two different
   jobs — the fallback when no interval is known, *and* the upper clamp on
   `gesture_s()` — and copying its clamp brought the conflation across. Any
   interval above 1.597 s stopped producing `interval/φ`:

   | interval | golden section | what was produced |
   |---|---|---|
   | 1.0 s | 0.618 s | 0.618 s |
   | 2.0 s | 1.236 s | **0.987 s** |
   | 3.2 s | 1.978 s | **0.987 s** |
   | 6.0 s | 3.708 s | **0.987 s** |

   3.2 s is the chord rate the renders were made at, so every A/B so far heard a
   gesture roughly half its specified length. The ceiling is gone: `interval_s`
   is already bounded to 8 s, so the gesture is bounded by construction at
   `8/φ ≈ 4.94 s` and a second ceiling protected nothing.

   **The test was shaped around the bug.** It checked exactness only
   `if interval < LONE_NOTE_S / GESTURE_FRACTION` — a guard whose condition is
   exactly the region where the ceiling did not bite. It passed, for the wrong
   reason, in the one place it was supposed to look. A guard that excludes the
   interesting case is not a guard, and that is worth remembering more than the
   bug is.

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
5. **Eight signatures, one per roster entry — and the inherited picking rule
   does not port.** `breath.rs` chose each mode's integers by a stated rule:
   *the sequence whose ratio limit **is** that mode's own constant.* That rule
   cannot extend here. A Variant supplies only two constants — 2/1 and 13/8 ≈ φ
   — for eight slots, and the modulation types have no constant of their own,
   so four entries would have to share each signature.

   The replacement rule is derived from something this instrument measures and
   BYPO did not: **an entry's signature period is set by whether its partials
   reinforce or shimmer.** `fm I` has 27 exactly-coincident partial pairs and
   *zero* beating pairs; every other entry beats. So `fm I` takes the one
   commensurate signature, and the seven that shimmer take long ones. The mode
   you would expect to repeat is still the only one that repeats — but now it is
   derived rather than noticed.

   **"Commensurate" is now a computed property, not a comment.** `breath.rs`
   says the non-harmonic modes never repeat because they are built on irrational
   limits. They do repeat: normalising integers by an integer yields rationals,
   so every signature is periodic. The real quantity is the period, and it has a
   closed form — with rates `aᵢ/max(a)`, all five realign after `T = max(a) /
   gcd(a)` base cycles. Harmonic is `T = 5`; Fibonacci is `T = 34`. A short
   period against a long one, which is what the original prose was reaching for,
   and a test asserts exactly one entry sits below ten.

   | entry | family | terms | period |
   |---|---|---|---|
   | fm I | lucas | 29 18 11 7 4 | 29 |
   | fm II | fibonacci | 34 21 13 8 5 | 34 |
   | **rm I** | **harmonic** | **30 24 18 12 6** | **5** |
   | rm II | lucas mirrored | 4 7 11 18 29 | 29 |
   | am I | padovan | 28 21 16 12 9 | 28 |
   | am II | perrin | 29 22 17 12 10 | 29 |
   | rect I | tribonacci | 44 24 13 7 4 | 44 |
   | rect II | pell | 29 12 5 2 1 | 29 |

   **The commensurate one was assigned to the wrong entry, on a number that had
   gone stale.** The first version gave it to `fm I`, citing 27 exact
   coincidences and zero beating pairs. That measurement was taken *before* the
   roster was corrected from ten entries to eight, and the labels moved
   underneath it — what was `fm I` then was `Fm1 × Harmonic`; what is `fm I` now
   is `Fm1 × Golden`. Re-measured on the roster that exists, `fm I` is the
   **most** shimmering entry on it: 140 partials and **442** beating pairs
   across a chord, against `fm II`'s 13. The comment in the file was arguing
   confidently for exactly the wrong choice.

   It belongs to `rm I` — three partials, zero beating pairs, and
   `Variant::Harmonic`, whose ratio is the low convergent 2/1. The entry you
   would expect to repeat is the only one that repeats, which is the property
   BYPO noticed and nobody chose, restored honestly rather than by coincidence.

   *A number carried across a refactor is not evidence; it is a memory of
   evidence.*

   **A mirrored signature collapses unless two things are true**, and in BYPO
   they are not. Reversed terms normalise to the same rate *multiset*, so with
   equal component amplitudes the pair is identical; and `begin()` there resets
   all five phases to one value, so after any note the two modes are
   bit-identical — measured at a mean absolute difference of 2.3e-17, which its
   own distinctness test never catches because it only compares the opening
   state. Here amplitude falls by φ with component *index*, so reversal changes
   which rate leads — a flutter against a heave — and `strike()` never touches
   phase, so the per-voice offset survives a note as well. Both are asserted by
   tests that render four seconds and compare.

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
2. **Schedule the steal — where the timescales allow it, which is not
   everywhere.** Inside the strum's budget, steal at the victim's next local
   minimum *below its present level*. A steal at the bottom of a voice's own
   breath is inaudible before any fade applies.

   **Built, and the original arrangement was backwards.** The field's rate is
   `freq/φ¹³`, so its period at pad pitch is measured in seconds — 4.7 s at
   110 Hz, 2.4 s at 220 Hz — while a strum window is one or two hundred
   milliseconds. There is no local minimum in that window and there never will
   be, so a steal on a low voice cannot be scheduled at all. It works where the
   field moves fast enough, which by construction is the **top** of a chord:
   880 Hz finds a trough inside half a second, 2 kHz inside a tenth.

   So trough-scheduling is opportunistic, not primary, and **level inheritance
   is what carries every case it cannot reach** — the reverse of what was
   written here before building it. Both are needed and neither is redundant.

   One correctness note kept because a test caught it: a local minimum is not
   necessarily *below* the present level. If the field is rising at t=0 it can
   crest and fall to a trough still louder than where it started, and
   scheduling a steal there is worse than stealing immediately. The search
   requires quieter-than-now, not merely a turning point.

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

### Built, and coprime delays turned out to be necessary but nowhere near sufficient

Three things the arithmetic could not have told us, all found by measurement:

**Coprimality does not prevent flutter; topology does.** With diffusion only at
the input, the tank is four comb filters in a ring — an impulse recirculates
recognisably and the tail rings at the loop period, whatever the delay lengths.
Measured short-lag autocorrelation: **0.98**. Adding allpasses *inside* the loop
took it to 0.53; replacing the ring coupling with a **Hadamard** mix — orthogonal,
so it scatters every line into every other on each pass without changing the
energy — is what actually made it dense. Coprime lengths stop echoes coinciding;
they do nothing about a structure that reproduces its input.

**The requirement is a coincidence period, not a gcd.** Four consecutive terms
`F(n…n+3)` are all-pairs coprime only when `3 ∤ n`, since `gcd(F(n), F(n+3)) =
F(gcd(n,3))` — the tank at `F(18)…F(21)` shares a factor of 2. But `lcm` for
that pair is 14.1 million samples, **294 seconds**. Demanding `gcd = 1` would
have pushed the tank up into 369 ms delays to satisfy arithmetic nobody can
hear. The rule is: no two echoes *in the same feedback loop* may coincide inside
a minute. It does not apply to the input diffusers at all — they are allpasses
in series, they smear phase rather than emitting echoes, and being short, 55 and
89 have an lcm of a tenth of a second while being perfectly coprime.

**Lengths must be disjoint, which a gcd test catches for free.** The
cross-network worst gcd came back as 1597 — not a shared factor but the same
length used twice, `F(17)` sitting in both the loop allpasses and the tank.

Final allocation, all three sets disjoint: diffusers `F(10)…F(13)`, loop
allpasses `F(14)…F(17)`, tank `F(18)…F(21)`.

**The chorus does not pulse, and "never re-aligns" is not a property any rates
have.** Six irrational rotations come arbitrarily close eventually — Weyl
equidistribution guarantees it. What matters is that the first re-alignment
falls outside any span a pad is held for: **78 seconds**. Two earlier versions
of that test measured nothing — one asked whether the rate ratios were near
rationals with small denominators, which is true of every real number once the
numerator is free (`φ⁵ ≈ 122/11`); the other counted `t = 0`, where the phases
are aligned by definition.

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

### The bare voice is the benchmark, and everything after it has to beat it

Recorded 2026-08-19, on hearing the first renders of the real signal path — three
operators, two-voice unison, a cosine swell, and **nothing else**: no field, no
filter, no FX, no Cadence.

> *"Every decision on audio is stellar — I don't know a single thing even in my
> own roster that comes close to sounding like this."*

That verdict is on a signal path missing five of the eight things this document
specifies. Which makes the remaining build unusually dangerous, in the ordinary
way instruments get dulled: **nobody adds the bad thing.** Everyone adds a
defensible thing, each stage is individually justified, and the sum is worse than
the part that already worked.

So, for steps 3 through 8:

1. **Every stage renders an A/B against the bare voice** — same chord, same
   algorithm, the stage in and the stage out — and the pair is listened to before
   the stage is called done. `phyllotaxis-voice/examples/audition.rs` is the
   harness; adding a stage means adding its comparison.
2. **Anything that does not clearly improve that A/B gets a switch, or gets
   cut.** Billy's standard for the density control, generalised: *"have a toggle
   that turns it on and off and we will see if it earns its keep."*
3. **The bare voice stays reachable as a preset.** Not as a debug mode — as a
   patch, with a name, that a player can get back to.

This is the only place in the document where a *process* is recorded rather than
a design. It is here because the thing being protected already exists, and the
threat to it is the rest of the plan.

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
