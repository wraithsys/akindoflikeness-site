# Harmonic dimensions — the what-to-weight map

Research-agent deliverable, 2026-08-21, digested the same day. Frame: **nothing
is an axiom — every dimension gets a weight and a confidence, estimated from
Billy's 1–5 ratings** ("perhaps it's all non zero", 2026-08-21). Model
vocabulary: each node is a bass/context pitch with 12 weighted destination
degrees above it and 12 weighted outgoing bass edges, conditioned on the
previous edge (n−1 memory).

Probe voicings live in the tasting keyboard's deck (`harmony-tastings.html`,
JOURNEYS array) — that JSON is ground truth, not the sketches here. Deck 2
(K01–K30 + carried J-cards) covers groups 1, 2, 5, 6 and one probe each from
3 and 4; the rest queue for deck 3.

## 1. Vertical (single chord)

| dimension | weight target in the node map |
|---|---|
| bass position / inversion / slash bass | the node-identity mapping itself: same pc-set under a different context note is a different degree-weight lookup |
| added-colour taxonomy (add9, 6/9, maj7, ♯11, sus, ♭9) | the 12 degree weights per node, split by node quality-context |
| shell vs full voicing (5th omitted) | per-degree *inclusion* weights, separate from colour weights |
| density (3–6 notes) | cardinality prior regularizing the degree-weight sum |
| spacing close/open (same pcs) | adjacent-voice interval prior, register-conditioned |
| register placement | global register offset term (between-passage probes only — never restate within one passage, teleport aversion swamps it) |
| doubling choice | bonus weight on repeated-pc destinations |

## 2. Transition (chord pair)

| dimension | weight target |
|---|---|
| common-tone count 0/1/2/3 (cliff or gradient?) | the connective-tissue multiplier on every edge — the single most important learned curve |
| bass motion interval class | the 12 outgoing bass-edge weights per node — the backbone table |
| semitone vs whole-tone voice motion | per-voice displacement weights by \|Δ\| |
| direction asymmetry (same move up vs down) | direction/momentum term conditioned on previous edge |
| voice-leading total displacement | edge cost temperature after the holdover multiplier |
| contrary vs parallel motion | motion-pattern flag (planing love predicts parallel is NOT penalized) |

## 3. Functional / modal

Borrowed-chord inventory (iv, ♭VI, ♭VII, ♭II, ii°, Picardy) → out-of-mode
degree weights per home node. Chromatic mediants (4 types) → bass edges at
±3/±4 crossed with quality-change flag. Deceptive resolutions → edges
conditioned on a previous dominant-quality edge. Secondary dominants vs shell
substitutes → alteration weights gated by the following edge.

## 4. Modulation methods (4–6 chords)

Common-tone · chromatic/planing · pivot-chord · parallel-key switch ·
sequential · enharmonic (dim7 re-read). Weight targets range from "does one
held tone buy a whole node-map re-index" (common-tone) to "do edges attach to
spelled function or raw pc-set" (enharmonic — the only discriminator). All
landings must be coloured unless bare-major arrival is itself the probe.

## 5. Trajectory (passage level)

| dimension | weight target |
|---|---|
| **debt-and-delay** (rub owed, paid after N chords) — the key experiment | decay constant on unresolved tension across edges; makes conditioning deeper than one step |
| returns exact vs varied | revisit multiplier conditioned on a modification flag |
| where home lands (mid / penultimate / final / never) | positional prior on the home node |
| tension-curve shapes (arch / ramp / late spike) | small set of passage-level curve priors |
| pedal point | self-edge weights: degree-sets churning over an unchanged node |
| sequence real vs tonal | interpolation between chromatic and mode-projected edge tables |

## 6. Frame effects

- **Major-with-colour vs bare-major arrival** — terminal-node degree weights
  as a separate table from interior ones. Finds the minimum colour dose that
  redeems a major ending.
- **Mode of the home node** — is home minor-only? One flag, re-keys all 12
  degree weights.
- **Transposition invariance** — one shared degree-indexed map or twelve
  absolute ones. Also decides whether the Scriabin colours are structural to
  the ear or visual language only.

## Out of scope — needs a different engine

Rhythm, meter, duration, the 1.4 s grid itself, dynamics, arpeggiation,
articulation, timbre, melody-over-harmony. All plausibly weighted; none
probeable with block chords. Parked — do not let block-chord ratings silently
absorb their variance.

## Confound register (verbatim from the deliverable)

- Inversion ↔ spacing: freeze upper voices vs re-space — run both pairs.
- Density ↔ spacing: add notes only into existing gaps.
- Common tones ↔ VL distance ↔ bass interval: the three-way tangle; every
  transition deck fixes two and sweeps one.
- Any major-key probe ↔ bare-major-final dislike: always end coloured unless
  that is the thing being probed.
- Transposition ↔ register: decorrelate direction of transposition across decks.
- Register placement ↔ teleport dislike: between-passage comparison only.
