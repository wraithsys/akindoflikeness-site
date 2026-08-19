//! Nearest-note voice leading against a five-voice budget.
//!
//! Everything in **absolute cents**, `c(f) = 1200·log₂(f/8.1757989156)`, so a
//! cent is the same size everywhere. Comparing in Hz would make every
//! threshold register-dependent, and this instrument's tunings are already
//! non-integer cent values — `968.81 ¢` and the like — so there is no grid to
//! fall back on.
//!
//! Three things this has to get right, in order of how easy they are to get
//! wrong:
//!
//! 1. **A common tone must keep its voice and not retrigger.** Restarting a
//!    gesture on a note that never stopped is a fault caused purely by
//!    bookkeeping — inaudible as a bug, merely disappointing as music.
//! 2. **The assignment must be optimal, not greedy.** See the test.
//! 3. **The octave spring decides the octave and nothing else.** Folding it
//!    into the assignment cost makes the optimiser abandon an exact common
//!    tone that happens to sit far from its home register.

use crate::word::PHI;

/// Absolute cents of a frequency. `= MIDI × 100`.
#[inline]
pub fn cents_of(hz: f64) -> f64 {
    1200.0 * (hz / 8.175_798_915_6).log2()
}

/// The inverse.
#[inline]
pub fn hz_of(cents: f64) -> f64 {
    8.175_798_915_6 * (cents / 1200.0).exp2()
}

/// Two pitches this close are the same pitch — half §3's 50 ¢ separation
/// floor. Anything under it cannot be a step between degrees, so it is one
/// degree that has drifted because the computed tuning moves with INDEX.
pub const HOLD_TOLERANCE_CENTS: f64 = 5.0;

/// Weight of the octave spring against raw movement. `1/φ`.
pub const SPRING: f64 = 1.0 / PHI;

pub const OCTAVE: f64 = 1200.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotState {
    Free,
    Held,
    Released,
}

#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub state: SlotState,
    /// Current pitch, absolute cents. Meaningless when `Free`.
    pub cents: f64,
    /// How much this voice's movement costs, 0…1.
    ///
    /// `Held` is 1. `Released` is its predicted field level, so a voice whose
    /// tail has nearly gone is nearly free to take — the allocator's own
    /// prediction, reused rather than re-derived.
    pub weight: f64,
}

/// What happened to one voice.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bind {
    /// Kept its pitch; must NOT be retriggered.
    Hold { target: usize },
    /// A released voice reclaimed for a pitch it already had.
    Resurrect { target: usize },
    /// Moved to a new pitch.
    Move { target: usize, cents: f64 },
    /// Nothing for this voice this chord.
    Idle,
}

/// The register each voice calls home: the golden angle, unwrapped in pitch.
///
/// `H_k = R + k·1200/φ` — 741.64 ¢ apart, which is the same `12/φ ≈ 7.416`
/// semitone step the Phyllotaxis scale places its degrees at. Five voices span
/// 2.47 octaves. The spring pulls a voice back toward its own rung so a pad
/// cannot walk off the top of the keyboard over many chords.
#[inline]
pub fn home(slot: usize, root_cents: f64) -> f64 {
    root_cents + slot as f64 * OCTAVE / PHI
}

/// Choose which octave of `degree` this slot should sing.
///
/// Minimises `|q − s| + (1/φ)·|q − H|`: move a little, and drift home. Only
/// three images need testing — the nearest and its neighbours.
pub fn realise(degree_cents: f64, root_cents: f64, slot_cents: f64, home_cents: f64) -> f64 {
    let base = root_cents + degree_cents;
    let k0 = ((slot_cents - base) / OCTAVE).round();
    [k0 - 1.0, k0, k0 + 1.0]
        .into_iter()
        .map(|k| base + k * OCTAVE)
        .min_by(|a, b| {
            let j = |q: f64| (q - slot_cents).abs() + SPRING * (q - home_cents).abs();
            j(*a).partial_cmp(&j(*b)).expect("no NaN pitches")
        })
        .expect("three candidates")
}

/// Lead the sounding voices to the next chord.
///
/// Returns one [`Bind`] per slot.
pub fn lead(slots: &[Slot], degrees: &[f64], root_cents: f64) -> Vec<Bind> {
    let n = slots.len();
    let mut out = vec![Bind::Idle; n];
    if degrees.is_empty() {
        return out;
    }

    // Realise every degree per slot: the spring picks the octave here, and
    // ONLY here. It must not reach the assignment cost below.
    let realised: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let s = if slots[i].state == SlotState::Free {
                home(i, root_cents)
            } else {
                slots[i].cents
            };
            degrees
                .iter()
                .map(|&d| realise(d, root_cents, s, home(i, root_cents)))
                .collect()
        })
        .collect();

    let mut slot_free: Vec<bool> = vec![true; n];
    let mut target_free: Vec<bool> = vec![true; degrees.len()];

    // Common tones first, held voices before released ones. Provably free:
    // matching an exact common tone never increases total cost when the
    // holder's weight is maximal, which it is.
    for (want, mark) in [
        (SlotState::Held, true),
        (SlotState::Released, false),
    ] {
        for i in 0..n {
            if !slot_free[i] || slots[i].state != want {
                continue;
            }
            for j in 0..degrees.len() {
                if !target_free[j] {
                    continue;
                }
                if (realised[i][j] - slots[i].cents).abs() <= HOLD_TOLERANCE_CENTS {
                    out[i] = if mark {
                        Bind::Hold { target: j }
                    } else {
                        Bind::Resurrect { target: j }
                    };
                    slot_free[i] = false;
                    target_free[j] = false;
                    break;
                }
            }
        }
    }

    // Optimal assignment over what is left.
    //
    // Brute force, not Hungarian. At most five slots and five targets means at
    // most 120 permutations at control rate — the cost of being certainly
    // right is a rounding error, and an assignment bug would be inaudible
    // until it was maddening.
    let free_slots: Vec<usize> = (0..n).filter(|&i| slot_free[i]).collect();
    let free_targets: Vec<usize> = (0..degrees.len()).filter(|&j| target_free[j]).collect();

    let cost = |i: usize, j: usize| -> f64 {
        if slots[i].state == SlotState::Free {
            0.0
        } else {
            slots[i].weight * (realised[i][j] - slots[i].cents).abs()
        }
    };

    let mut best: Option<(f64, Vec<(usize, usize)>)> = None;
    let mut chosen: Vec<(usize, usize)> = Vec::new();
    fn search(
        free_slots: &[usize],
        free_targets: &[usize],
        used: &mut Vec<bool>,
        idx: usize,
        acc: f64,
        chosen: &mut Vec<(usize, usize)>,
        best: &mut Option<(f64, Vec<(usize, usize)>)>,
        cost: &dyn Fn(usize, usize) -> f64,
    ) {
        if idx == free_targets.len() {
            if best.as_ref().map_or(true, |(b, _)| acc < *b - 1e-9) {
                *best = Some((acc, chosen.clone()));
            }
            return;
        }
        if let Some((b, _)) = best.as_ref() {
            if acc >= *b {
                return;
            }
        }
        for (k, &i) in free_slots.iter().enumerate() {
            if used[k] {
                continue;
            }
            used[k] = true;
            let j = free_targets[idx];
            chosen.push((i, j));
            search(free_slots, free_targets, used, idx + 1, acc + cost(i, j), chosen, best, cost);
            chosen.pop();
            used[k] = false;
        }
    }
    let mut used = vec![false; free_slots.len()];
    if free_targets.len() <= free_slots.len() {
        search(&free_slots, &free_targets, &mut used, 0, 0.0, &mut chosen, &mut best, &cost);
    }

    if let Some((_, binds)) = best {
        for (i, j) in binds {
            out[i] = Bind::Move { target: j, cents: realised[i][j] };
        }
    }
    out
}

/// Total movement of a plan, in cents — the thing being minimised.
pub fn total_movement(slots: &[Slot], binds: &[Bind]) -> f64 {
    binds
        .iter()
        .enumerate()
        .map(|(i, b)| match b {
            Bind::Move { cents, .. } if slots[i].state != SlotState::Free => {
                (cents - slots[i].cents).abs()
            }
            _ => 0.0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(c: f64) -> Slot {
        Slot { state: SlotState::Held, cents: c, weight: 1.0 }
    }
    fn released(c: f64, w: f64) -> Slot {
        Slot { state: SlotState::Released, cents: c, weight: w }
    }
    fn free() -> Slot {
        Slot { state: SlotState::Free, cents: 0.0, weight: 0.0 }
    }

    /// **Greedy is wrong, and here is the case.** Two held voices, two targets.
    /// Nearest-first grabs the global minimum (190) and strands the other voice
    /// with a 600-cent leap; the optimal pairing moves both a little and never
    /// crosses.
    #[test]
    fn greedy_nearest_first_is_not_good_enough() {
        let root = 4500.0;
        let slots = [held(4500.0), held(4900.0)];
        // Degrees chosen so the realised targets are 4710 and 5100.
        let degrees = [210.0, 600.0];
        let binds = lead(&slots, &degrees, root);
        let moved = total_movement(&slots, &binds);

        assert!((moved - 410.0).abs() < 1e-6, "optimal is 410 cents, got {moved}");
        // Greedy would have taken 190 + 600 = 790.
        assert!(moved < 790.0);
        // And it must not cross: the lower voice takes the lower target.
        match (binds[0], binds[1]) {
            (Bind::Move { target: a, .. }, Bind::Move { target: b, .. }) => {
                assert!(a < b, "voices crossed: {a} then {b}");
            }
            other => panic!("expected two moves, got {other:?}"),
        }
    }

    /// A pitch that survives the chord change keeps its voice and is marked
    /// HOLD, so nothing downstream retriggers its gesture.
    #[test]
    fn a_common_tone_is_held_not_moved() {
        let root = 4500.0;
        let slots = [held(4500.0), held(5200.0)];
        let degrees = [0.0, 700.0];
        let binds = lead(&slots, &degrees, root);
        assert!(matches!(binds[0], Bind::Hold { .. }), "{:?}", binds[0]);
        assert!(matches!(binds[1], Bind::Hold { .. }), "{:?}", binds[1]);
        assert_eq!(total_movement(&slots, &binds), 0.0);
    }

    /// Tolerance is in cents, so it works on a spectrum-derived tuning whose
    /// degrees drift by a few cents as INDEX moves.
    #[test]
    fn a_degree_that_drifted_with_index_still_counts_as_held() {
        let root = 4500.0;
        let slots = [held(4500.0), held(5220.0), held(5483.0)];
        // The same degrees, moved 3.4 and −2.8 cents by an INDEX change.
        let degrees = [0.0, 723.4, 980.2];
        let binds = lead(&slots, &degrees, root);
        assert!(binds.iter().all(|b| matches!(b, Bind::Hold { .. })), "{binds:?}");
    }

    /// **The spring picks the octave.** A voice that has drifted far above its
    /// home takes a lower image even though it is a bigger leap, because the
    /// register error falls by more than the extra distance costs.
    #[test]
    fn the_octave_spring_pulls_a_drifted_voice_home() {
        let root = 4500.0;
        let h2 = home(2, root);
        assert!((h2 - 5983.281).abs() < 0.01, "home ladder moved: {h2}");
        let got = realise(800.0, root, 7400.0, h2);
        assert!((got - 6500.0).abs() < 1e-6, "expected 6500, got {got}");
    }

    /// **But the spring must not reach the assignment cost.** A slot holding an
    /// exact common tone far from its home must keep it — folding the spring
    /// into the cost would move it for the sake of register.
    #[test]
    fn the_spring_never_costs_a_common_tone() {
        let root = 4500.0;
        // slot 2's home is 5983; park a held voice on an exact degree at 5200.
        let slots = [held(4500.0), free(), held(5200.0)];
        let degrees = [0.0, 700.0];
        let binds = lead(&slots, &degrees, root);
        assert!(
            matches!(binds[2], Bind::Hold { .. }),
            "the spring dragged a common tone off its pitch: {:?}",
            binds[2]
        );
    }

    /// Fewer targets than voices: the surplus goes idle rather than doubling.
    #[test]
    fn a_shrinking_chord_leaves_voices_idle() {
        let root = 4500.0;
        let slots = [held(4500.0), held(4800.0), held(5200.0), held(5500.0)];
        let degrees = [0.0, 700.0];
        let binds = lead(&slots, &degrees, root);
        let busy = binds.iter().filter(|b| !matches!(b, Bind::Idle)).count();
        assert_eq!(busy, 2, "{binds:?}");
        // No target taken twice.
        let mut taken: Vec<usize> = binds
            .iter()
            .filter_map(|b| match b {
                Bind::Hold { target } | Bind::Resurrect { target } | Bind::Move { target, .. } => Some(*target),
                Bind::Idle => None,
            })
            .collect();
        taken.sort_unstable();
        taken.dedup();
        assert_eq!(taken.len(), busy, "a degree was voiced twice");
    }

    /// A released voice whose tail has almost gone is cheap to take, so the
    /// optimiser prefers it over disturbing a held one.
    #[test]
    fn a_faded_release_is_cheaper_than_a_held_voice() {
        let root = 4500.0;
        let slots = [held(4500.0), released(5300.0, 0.05)];
        let degrees = [900.0];
        let binds = lead(&slots, &degrees, root);
        assert!(
            matches!(binds[1], Bind::Move { .. } | Bind::Resurrect { .. }),
            "the faded voice should have taken it: {binds:?}"
        );
        assert!(matches!(binds[0], Bind::Idle), "the held voice was disturbed");
    }

    /// Free slots cost nothing and are used before anything is displaced.
    #[test]
    fn free_slots_are_taken_first() {
        let root = 4500.0;
        let slots = [held(4500.0), free(), free()];
        let degrees = [300.0, 800.0];
        let binds = lead(&slots, &degrees, root);
        assert!(matches!(binds[0], Bind::Idle), "a held voice was moved: {binds:?}");
    }

    /// Cents and Hz round-trip, since the whole module depends on it.
    #[test]
    fn pitch_conversion_round_trips() {
        for hz in [55.0f64, 110.0, 220.0, 440.0, 1760.0] {
            assert!((hz_of(cents_of(hz)) - hz).abs() < 1e-9);
        }
        assert!((cents_of(440.0) - 6900.0).abs() < 0.01, "A4 should be 6900 cents");
    }

    #[test]
    fn degenerate_inputs_are_safe() {
        let root = 4500.0;
        assert!(lead(&[], &[0.0], root).is_empty());
        assert!(lead(&[held(4500.0)], &[], root).iter().all(|b| matches!(b, Bind::Idle)));
    }
}
