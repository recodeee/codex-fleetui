//! Spotlight overlay fuzzy filter.
//!
//! Wraps the `SkimMatcherV2` algorithm from the `fuzzy-matcher` crate (the
//! standalone-published version of skim's matcher) into a small, ratatui-
//! friendly API: feed it a query and a slice of items, get back ranked
//! hits with the matched character indices for highlighting.
//!
//! The Spotlight overlay (rust/fleet-ui/src/overlay/spotlight.rs and the
//! fleet-tui-poc demo) is the primary consumer. The filter itself is
//! generic over `T: AsRef<str>` so it works on plain string lists, on
//! struct items via a `key: String` field, or on borrowed Cow values.
//!
//! Algorithm choice: SkimMatcherV2 is what `fzf` users expect — bonus
//! points for prefix / word-boundary / camelCase matches, penalties for
//! gaps. Tuned for human-readable queries against human-readable lists
//! (commands, file paths, worker names). For lots of items or programmatic
//! pattern matching, prefer regex.

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// A ranked match returned by [`SpotlightFilter::rank`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Index into the original items slice.
    pub index: usize,
    /// Score from the matcher (higher = better match).
    pub score: i64,
    /// Character indices in the item string that matched the query — use
    /// these to underline / highlight the matched glyphs in the overlay.
    pub indices: Vec<usize>,
}

/// Fuzzy filter for the Spotlight overlay.
///
/// Holds a configured `SkimMatcherV2` so callers can reuse the same
/// instance across renders without rebuilding the case-fold tables.
pub struct SpotlightFilter {
    matcher: SkimMatcherV2,
}

impl SpotlightFilter {
    pub fn new() -> Self {
        Self {
            // Default config: case-insensitive, smart-case off (we want
            // "Plan" and "plan" to match identically — UX bias).
            matcher: SkimMatcherV2::default().ignore_case(),
        }
    }

    /// Empty-query shortcut: returns all items in their original order
    /// with score 0 and no indices. Spotlight wants the unfiltered list
    /// visible the moment the overlay opens, before any typing.
    pub fn all<T: AsRef<str>>(&self, items: &[T]) -> Vec<Hit> {
        items
            .iter()
            .enumerate()
            .map(|(index, _)| Hit {
                index,
                score: 0,
                indices: Vec::new(),
            })
            .collect()
    }

    /// Rank `items` against `query`. Returns at most `limit` hits sorted by
    /// score descending. An empty query returns [`SpotlightFilter::all`]
    /// truncated to `limit`.
    pub fn rank<T: AsRef<str>>(&self, query: &str, items: &[T], limit: usize) -> Vec<Hit> {
        if query.is_empty() {
            let mut all = self.all(items);
            all.truncate(limit);
            return all;
        }
        let mut hits: Vec<Hit> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.matcher
                    .fuzzy_indices(item.as_ref(), query)
                    .map(|(score, indices)| Hit { index, score, indices })
            })
            .collect();
        // Descending score — best match first.
        hits.sort_by(|a, b| b.score.cmp(&a.score));
        hits.truncate(limit);
        hits
    }
}

impl Default for SpotlightFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all_in_order() {
        let filter = SpotlightFilter::new();
        let items = vec!["overview", "fleet", "plan", "waves", "review"];
        let hits = filter.rank("", &items, 10);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0].index, 0);
        assert_eq!(hits[4].index, 4);
    }

    #[test]
    fn empty_query_respects_limit() {
        let filter = SpotlightFilter::new();
        let items = vec!["a", "b", "c", "d", "e"];
        let hits = filter.rank("", &items, 2);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn ranks_better_matches_first() {
        let filter = SpotlightFilter::new();
        let items = vec!["overview", "fleet", "plan", "playlist", "plant"];
        let hits = filter.rank("pl", &items, 10);
        // All three items containing "pl" should match; the shorter / more
        // prefix-ish ones rank higher than longer ones.
        assert!(hits.len() >= 3);
        let top_label = items[hits[0].index];
        assert!(
            top_label == "plan" || top_label == "plant" || top_label == "playlist",
            "expected pl* match, got {top_label}"
        );
    }

    #[test]
    fn no_match_returns_empty() {
        let filter = SpotlightFilter::new();
        let items = vec!["overview", "fleet", "plan"];
        let hits = filter.rank("xyz", &items, 10);
        assert!(hits.is_empty());
    }

    #[test]
    fn is_case_insensitive() {
        let filter = SpotlightFilter::new();
        let items = vec!["Overview", "FLEET", "plan"];
        // Lowercase query matches mixed-case items.
        let hits = filter.rank("fleet", &items, 10);
        assert!(hits.iter().any(|h| items[h.index] == "FLEET"));
    }

    #[test]
    fn includes_match_indices_for_highlighting() {
        let filter = SpotlightFilter::new();
        let items = vec!["overview"];
        let hits = filter.rank("ovr", &items, 10);
        assert_eq!(hits.len(), 1);
        // 'o', 'v', 'r' should be at positions 0, 1, 5 (or similar — exact
        // positions depend on matcher tuning; just confirm indices are
        // populated and within bounds).
        assert!(!hits[0].indices.is_empty());
        assert!(hits[0].indices.iter().all(|&i| i < items[0].len()));
    }

    #[test]
    fn limit_caps_result_count() {
        let filter = SpotlightFilter::new();
        let items: Vec<String> = (0..20).map(|i| format!("item-{i}")).collect();
        let hits = filter.rank("item", &items, 5);
        assert_eq!(hits.len(), 5);
    }
}
