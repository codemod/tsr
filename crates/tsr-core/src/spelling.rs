//! "Did you mean …?" — the closest known name to one the user got wrong.
//!
//! Ported from `getSpellingSuggestion` (`internal/core/core.go:567`) and
//! `levenshteinWithMax` (`:627`) at the pinned commit.
//!
//! # Why this is not any edit-distance crate
//!
//! The distances are **fractional and asymmetric**, and the constants are what
//! decide whether `tsc` says "did you mean" at all:
//!
//! - A case-only substitution costs **0.1**, an ordinary substitution **2**, and
//!   an insertion or deletion **1**. So `--Target` is nearly free to correct and
//!   `--tarjet` is expensive — which is the behaviour a user expects and which a
//!   uniform-cost Levenshtein cannot express.
//! - The search gives up once a column's minimum exceeds the budget, and the
//!   budget starts at `floor(len * 0.4) + 0.9` and *shrinks* as better
//!   candidates are found.
//!
//! Getting any of those wrong changes which suggestion is printed, and the
//! suggestion is in the baselined output.

// The whole module is Go's float arithmetic transliterated, and the casts are
// the transliteration. `len()` to `f64` cannot lose precision for any option
// name in existence, and the `f64`-to-integer casts are bounded by `s2.len()`
// immediately afterwards. Allowed at module scope rather than at a dozen
// statements, because a per-cast allow reads as a considered exception at each
// site and these are one decision.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

/// The candidate closest to `name`, or `None` if nothing is close enough.
///
/// Ported from `getSpellingSuggestion`. `candidates` is walked in order and
/// `compare` breaks ties, exactly as upstream's `compare(candidate,
/// bestCandidate) < 0` does — so a caller passing names in declaration order and
/// comparing lexically gets upstream's answer.
///
/// Two candidates are skipped outright: one that equals `name` (you cannot
/// suggest what was written), and one shorter than three characters that differs
/// by more than case — upstream's reasoning is that a user notices a
/// two-character name being wrong without help.
pub fn get_spelling_suggestion<'a, T>(
    name: &str,
    candidates: impl IntoIterator<Item = &'a T>,
    get_name: impl Fn(&T) -> &str,
    compare: impl Fn(&T, &T) -> std::cmp::Ordering,
) -> Option<&'a T>
where
    T: 'a,
{
    let name_chars: Vec<char> = name.chars().collect();
    let maximum_length_difference = std::cmp::max(2, (name_chars.len() as f64 * 0.34) as usize);
    let mut best_distance = (name_chars.len() as f64 * 0.4).floor() + 0.9;

    let mut best: Option<&'a T> = None;
    for candidate in candidates {
        let candidate_name = get_name(candidate);
        // Upstream compares `len(candidateName)` — bytes — against
        // `len(runeName)`, which is *characters*. Reproduced rather than
        // corrected: it only matters for a non-ASCII option name, and no
        // declaration has one, but silently disagreeing with upstream about a
        // threshold is how a suggestion changes for a reason nobody can find.
        let candidate_len = candidate_name.len();
        let max_len = std::cmp::max(candidate_len, name_chars.len());
        let min_len = std::cmp::min(candidate_len, name_chars.len());
        if candidate_name.is_empty() || max_len - min_len > maximum_length_difference {
            continue;
        }
        if candidate_name == name {
            continue;
        }
        if candidate_len < 3 && !candidate_name.eq_ignore_ascii_case(name) {
            continue;
        }

        let candidate_chars: Vec<char> = candidate_name.chars().collect();
        let Some(distance) = levenshtein_with_max(&name_chars, &candidate_chars, best_distance)
        else {
            continue;
        };

        if distance < best_distance {
            best_distance = distance;
            best = Some(candidate);
        } else if best.is_none_or(|current| compare(candidate, current).is_lt()) {
            best = Some(candidate);
        }
    }
    best
}

/// Edit distance, or `None` once it is certain to exceed `max_value`
/// (`levenshteinWithMax`).
///
/// The early exit is not only an optimisation: it is what makes the caller's
/// shrinking budget correct, because a candidate that cannot beat the current
/// best is never scored at all.
fn levenshtein_with_max(s1: &[char], s2: &[char], max_value: f64) -> Option<f64> {
    let width = s2.len() + 1;
    let big = max_value + 0.01;

    let mut previous: Vec<f64> = (0..width).map(|index| index as f64).collect();
    let mut current: Vec<f64> = vec![0.0; width];

    for i in 1..=s1.len() {
        let c1 = s1[i - 1];
        let min_j = std::cmp::max((i as f64 - max_value).ceil() as i64, 1) as usize;
        let max_j = std::cmp::min((max_value + i as f64).floor() as i64, s2.len() as i64) as usize;

        let mut col_min = i as f64;
        current[0] = col_min;
        for entry in current.iter_mut().take(min_j).skip(1) {
            *entry = big;
        }
        for j in min_j..=max_j {
            // A case-only difference is nearly free; a real substitution costs
            // twenty times as much.
            let substitution_distance = if c1.to_lowercase().eq(s2[j - 1].to_lowercase()) {
                previous[j - 1] + 0.1
            } else {
                previous[j - 1] + 2.0
            };
            let distance = if c1 == s2[j - 1] {
                previous[j - 1]
            } else {
                (previous[j] + 1.0).min((current[j - 1] + 1.0).min(substitution_distance))
            };
            current[j] = distance;
            col_min = col_min.min(distance);
        }
        for entry in current.iter_mut().take(s2.len() + 1).skip(max_j + 1) {
            *entry = big;
        }
        if col_min > max_value {
            // Everything in this column is already over budget and no later
            // column can improve on it.
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let result = previous[s2.len()];
    if result > max_value { None } else { Some(result) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggest<'a>(name: &str, candidates: &'a [&'static str]) -> Option<&'a &'static str> {
        get_spelling_suggestion(name, candidates, |candidate| candidate, Ord::cmp)
    }

    const OPTIONS: &[&str] = &["target", "module", "strict", "noEmit", "declaration", "jsx"];

    #[test]
    fn a_case_difference_is_almost_free() {
        assert_eq!(suggest("Target", OPTIONS), Some(&"target"));
        assert_eq!(suggest("NOEMIT", OPTIONS), Some(&"noEmit"));
    }

    #[test]
    fn a_transposition_is_found() {
        assert_eq!(suggest("taget", OPTIONS), Some(&"target"));
        assert_eq!(suggest("delcaration", OPTIONS), Some(&"declaration"));
    }

    #[test]
    fn something_unrelated_suggests_nothing() {
        // The budget is what stops `tsc` guessing wildly; without it every
        // unknown option would get a suggestion.
        assert_eq!(suggest("wibble", OPTIONS), None);
        assert_eq!(suggest("xyzzyplugh", OPTIONS), None);
    }

    #[test]
    fn the_exact_name_is_never_suggested() {
        // Reachable: the command-line table and the config table are consulted
        // separately, so a name valid in one can be unknown in the other.
        assert_eq!(suggest("target", OPTIONS), None);
    }

    #[test]
    fn a_very_short_candidate_needs_a_case_only_difference() {
        let candidates = ["ab", "jsx"];
        // "ab" differs from "ax" by a real substitution and is under three
        // characters, so it is skipped.
        assert_eq!(suggest("ax", &candidates), None);
        // Same length, differing only by case, so it is allowed.
        assert_eq!(suggest("AB", &candidates), Some(&"ab"));
    }

    #[test]
    fn a_much_longer_name_is_not_a_candidate() {
        // The length-difference gate: `max(2, len * 0.34)`.
        assert_eq!(suggest("t", OPTIONS), None);
    }
}
