//! Scrape `WORKING ON` text + model + runtime out of a pane's scrollback tail.
//!
//! Extracted from [`crate::fleet`] — same lenient spirit as `panes::classify`
//! and the bash scrapers in `watcher-board.sh` / `fleet-tick.sh`: every field
//! is optional, a miss just yields an emptier row rather than dropping it.

/// What a pane is doing, scraped from its scrollback tail. Separate from
/// [`crate::panes::PaneState`] (the *status*); this is the human-readable
/// *detail* the artboard's `WORKING ON` column shows.
pub struct PaneActivity {
    /// The headline task line, e.g. `scaffold rust/fleet-ui`.
    pub working_on: String,
    /// Model label, e.g. `gpt-5.5 xhigh`, lifted from the status line.
    pub model_label: Option<String>,
    /// Runtime hint, e.g. `10m 28s`, lifted from `Working (…)`.
    pub runtime: Option<String>,
}

/// Scrape the activity triple out of a pane's scrollback tail.
///
/// Walks newest-to-oldest so the *last* match wins (the pane's current line,
/// not a stale one higher in the scrollback). Each field is filled by its own
/// extractor; the first successful match for each field is kept.
pub fn scrape_activity(tail: &str) -> PaneActivity {
    let mut working_on = String::new();
    let mut model_label = None;
    let mut runtime = None;

    for line in tail.lines().rev() {
        let t = line.trim();

        if runtime.is_none() {
            runtime = extract_runtime(t);
        }
        if model_label.is_none() {
            model_label = extract_model_label(t);
        }
        if working_on.is_empty() {
            if let Some(headline) = extract_headline(t) {
                working_on = headline;
            }
        }

        if !working_on.is_empty() && model_label.is_some() && runtime.is_some() {
            break;
        }
    }

    PaneActivity {
        working_on,
        model_label,
        runtime,
    }
}

/// Pull the runtime span out of a `● Working (10m 28s · esc to interrupt)`
/// status line. Returns the trimmed `10m 28s` token, or `None` if the line
/// doesn't carry a `Working (` prefix or the span is empty.
fn extract_runtime(line: &str) -> Option<String> {
    let after = line.split_once("Working (").map(|(_, a)| a)?;
    let span: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == 'm' || *c == 's' || *c == ' ')
        .collect();
    let span = span.trim();
    if span.is_empty() {
        None
    } else {
        Some(span.to_string())
    }
}

/// Known model-family prefixes we'll sniff out of a pane's status line.
///
/// fleet-launcher spawns codex (`gpt-…`), claude (`claude-…`), gemini
/// (`gemini-…`), and claw panes. Each CLI renders its own status line, so a
/// single hard-coded prefix would leave non-codex panes unlabelled. We pick
/// the prefix with the *earliest* occurrence in the line — same lenient
/// substring spirit as the original `gpt-` sniff, just generalized.
const MODEL_PREFIXES: &[&str] = &[
    "gpt-",
    "claude-",
    "gemini-",
    "o1-",
    "o3-",
    "sonnet-",
    "opus-",
    "haiku-",
];

/// Cheap substring sniff for a pane's status-line model label
/// (`gpt-5.5 xhigh`, `claude-opus-4-7 high`, `gemini-2.5-pro`, …).
/// Returns `model` plus at most one trailing effort word — good enough for
/// the dim subtitle without a full status-line parser.
fn extract_model_label(line: &str) -> Option<String> {
    // Pick the prefix with the smallest `find` index. When two families both
    // appear in the line, the one that shows up first wins — that's the
    // pane's actual status line; later occurrences are usually noise from
    // scrollback (e.g. a prompt that mentions another model).
    let idx = MODEL_PREFIXES
        .iter()
        .filter_map(|p| line.find(p))
        .min()?;
    let rest = &line[idx..];
    let span: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() || *c == ' ')
        .take(20)
        .collect();
    let mut it = span.split_whitespace();
    let model = it.next()?;
    Some(match it.next() {
        Some(effort) => format!("{model} {effort}"),
        None => model.to_string(),
    })
}

/// First non-empty, non-status, non-chrome line is the task headline. Codex
/// UI furniture (status glyphs, the `gpt-…` model line, `Working (…)` and
/// `tokens used` chrome) is skipped so we land on actual task text.
fn extract_headline(line: &str) -> Option<String> {
    if line.is_empty()
        || line.starts_with('●')
        || line.starts_with('⚠')
        || line.starts_with('✓')
        || line.starts_with('›')
        || line.starts_with('└')
        || line.starts_with("gpt-")
        || line.contains("Working (")
        || line.contains("tokens used")
    {
        return None;
    }
    Some(line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_runtime_parses_minutes_seconds() {
        assert_eq!(
            extract_runtime("● Working (10m 28s · esc to interrupt)").as_deref(),
            Some("10m 28s")
        );
    }

    #[test]
    fn extract_runtime_handles_seconds_only() {
        assert_eq!(
            extract_runtime("● Working (45s)").as_deref(),
            Some("45s")
        );
    }

    #[test]
    fn extract_runtime_missing_returns_none() {
        assert!(extract_runtime("port plan.rs").is_none());
        assert!(extract_runtime("").is_none());
    }

    #[test]
    fn extract_runtime_empty_parens_is_none() {
        // `Working ()` has no digit/m/s span to collect; should not yield Some("").
        assert!(extract_runtime("● Working ()").is_none());
    }

    #[test]
    fn extract_model_label_keeps_model_plus_effort() {
        assert_eq!(
            extract_model_label("gpt-5.5 xhigh · 37% left   49% context").as_deref(),
            Some("gpt-5.5 xhigh")
        );
    }

    #[test]
    fn extract_model_label_model_only_when_no_effort() {
        assert_eq!(
            extract_model_label("gpt-5.5").as_deref(),
            Some("gpt-5.5")
        );
    }

    #[test]
    fn extract_model_label_finds_model_mid_line() {
        // `find("gpt-")` is a substring search — embedded matches are kept.
        assert_eq!(
            extract_model_label("  prefix gpt-5 high  suffix").as_deref(),
            Some("gpt-5 high")
        );
    }

    #[test]
    fn extract_model_label_missing_returns_none() {
        assert!(extract_model_label("port plan.rs").is_none());
        assert!(extract_model_label("").is_none());
    }

    #[test]
    fn extract_model_label_claude_with_effort() {
        assert_eq!(
            extract_model_label("claude-opus-4-7 high").as_deref(),
            Some("claude-opus-4-7 high")
        );
    }

    #[test]
    fn extract_model_label_gemini() {
        // Trailing-word rule picks up the next whitespace-separated token
        // after the model (the lenient sniff doesn't know which trailing
        // tokens are "effort" vs. prose — it just grabs one).
        assert_eq!(
            extract_model_label("  using gemini-2.5-pro for this task").as_deref(),
            Some("gemini-2.5-pro for")
        );
        // Bare model at end of line yields model-only.
        assert_eq!(
            extract_model_label("gemini-2.5-pro").as_deref(),
            Some("gemini-2.5-pro")
        );
    }

    #[test]
    fn extract_model_label_o3() {
        // 20-char window after the match is "o3-mini reasoning..." —
        // splitting yields `o3-mini` + one trailing word `reasoning...`.
        assert_eq!(
            extract_model_label("o3-mini reasoning...").as_deref(),
            Some("o3-mini reasoning...")
        );
    }

    #[test]
    fn extract_model_label_picks_earliest_prefix() {
        // Both `claude-` and `gpt-` appear; `claude-` is earlier, so it wins.
        assert_eq!(
            extract_model_label("claude-opus-4-7 high (last run was gpt-5.5)").as_deref(),
            Some("claude-opus-4-7 high")
        );
        // And the reverse — `gpt-` earlier, claude later in the line.
        assert_eq!(
            extract_model_label("gpt-5.5 high (prev claude-opus-4-7)").as_deref(),
            Some("gpt-5.5 high")
        );
    }

    #[test]
    fn extract_headline_skips_chrome_glyphs() {
        assert!(extract_headline("● Working (1m)").is_none());
        assert!(extract_headline("⚠ warn").is_none());
        assert!(extract_headline("✓ done").is_none());
        assert!(extract_headline("› prompt").is_none());
        assert!(extract_headline("└ branch").is_none());
    }

    #[test]
    fn extract_headline_skips_status_line_and_token_chrome() {
        assert!(extract_headline("gpt-5.5 xhigh · 37% left").is_none());
        assert!(extract_headline("foo Working (1m) bar").is_none());
        assert!(extract_headline("12345 tokens used").is_none());
    }

    #[test]
    fn extract_headline_keeps_plain_task_text() {
        assert_eq!(
            extract_headline("scaffold rust/fleet-ui").as_deref(),
            Some("scaffold rust/fleet-ui")
        );
    }

    #[test]
    fn extract_headline_empty_is_none() {
        assert!(extract_headline("").is_none());
    }

    #[test]
    fn scrape_activity_picks_runtime_and_model() {
        let tail = "port plan.rs\n\
                    ● Working (12m 04s)\n\
                    gpt-5.5 xhigh · 37% left   49% context";
        let act = scrape_activity(tail);
        assert_eq!(act.working_on, "port plan.rs");
        assert_eq!(act.runtime.as_deref(), Some("12m 04s"));
        assert_eq!(act.model_label.as_deref(), Some("gpt-5.5 xhigh"));
    }

    #[test]
    fn scrape_activity_walks_newest_first_for_headline() {
        // Two candidate headlines; walking the tail in reverse means the
        // *last* line (newest) wins.
        let tail = "older task line\n\
                    newer task line";
        let act = scrape_activity(tail);
        assert_eq!(act.working_on, "newer task line");
    }

    #[test]
    fn scrape_activity_multi_line_skips_chrome_to_reach_headline() {
        let tail = "first real task\n\
                    ● Working (1m 02s)\n\
                    gpt-5.5 high\n\
                    ✓ ran tests\n\
                    › prompt suggestion";
        let act = scrape_activity(tail);
        assert_eq!(act.working_on, "first real task");
        assert_eq!(act.runtime.as_deref(), Some("1m 02s"));
        assert_eq!(act.model_label.as_deref(), Some("gpt-5.5 high"));
    }

    #[test]
    fn scrape_activity_empty_input_yields_all_empty() {
        let act = scrape_activity("");
        assert!(act.working_on.is_empty());
        assert!(act.runtime.is_none());
        assert!(act.model_label.is_none());
    }

    #[test]
    fn scrape_activity_missing_fields_stay_none() {
        // Only a headline, no model/runtime chrome.
        let act = scrape_activity("doing a thing\nmore detail");
        assert_eq!(act.working_on, "more detail");
        assert!(act.runtime.is_none());
        assert!(act.model_label.is_none());
    }

    #[test]
    fn scrape_activity_runtime_only_keeps_working_on_empty_when_all_chrome() {
        // Every line is chrome — no headline candidate survives the filter.
        let tail = "● Working (5m)\ngpt-5.5 xhigh";
        let act = scrape_activity(tail);
        assert!(act.working_on.is_empty());
        assert_eq!(act.runtime.as_deref(), Some("5m"));
        assert_eq!(act.model_label.as_deref(), Some("gpt-5.5 xhigh"));
    }
}
