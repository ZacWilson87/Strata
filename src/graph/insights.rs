/// Local insights rules engine — turns accumulated session signals and
/// objective session mechanics into actionable "Craft" insight cards.
///
/// Pure functions over already-derived `SessionSignalRow` (ADR 0005) and
/// `SessionMetricsRow` (ADR 0008) data: no I/O, no raw content, trivially
/// testable. The metric rules are self-relative — every comparison is against
/// the user's own baseline, never an external norm.
use std::collections::{HashMap, HashSet};

use super::queries::{SessionMetricsRow, SessionSignalRow};

/// The rolling window (in days) that friction insights are computed over.
pub const INSIGHT_WINDOW_DAYS: i64 = 30;

/// The rolling window (in days) that mechanics insights are computed over —
/// wider, because mechanics exist for all history via backfill.
pub const METRIC_WINDOW_DAYS: i64 = 90;

/// Maximum accepted length for an insight id.
pub const MAX_INSIGHT_ID_LEN: usize = 128;

/// An actionable workflow insight derived from accumulated session signals.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Insight {
    /// Stable identifier: `"<rule>:<domain>"`.
    pub id: String,
    /// The rule that produced this insight (e.g. `"repeated_context"`).
    pub rule: String,
    /// Short human-readable headline.
    pub title: String,
    /// One-to-two sentence practical recommendation.
    pub body: String,
    /// Human-readable evidence summary (counts only, never content).
    pub evidence: String,
    /// Size of the window the evidence was gathered over, in days.
    pub window_days: i64,
}

/// Returns true if `id` is a well-formed insight id: non-empty, at most
/// [`MAX_INSIGHT_ID_LEN`] bytes, and composed only of `[a-z0-9_:-]`.
pub fn is_valid_insight_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_INSIGHT_ID_LEN
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | ':' | '-'))
}

/// Static definition of a single insight rule.
struct RuleSpec {
    rule: &'static str,
    title: &'static str,
    body: &'static str,
    /// Minimum number of triggering rows for the rule to fire.
    threshold: usize,
    /// Verb phrase used in the evidence sentence, e.g. "flagged repeated context".
    evidence_verb: &'static str,
    matcher: fn(&SessionSignalRow) -> bool,
}

fn has_repeated_context(row: &SessionSignalRow) -> bool {
    row.friction.iter().any(|f| f == "repeated_context")
}

fn has_many_corrections(row: &SessionSignalRow) -> bool {
    row.friction.iter().any(|f| f == "many_corrections")
}

fn has_restarted_approach(row: &SessionSignalRow) -> bool {
    row.friction.iter().any(|f| f == "restarted_approach")
}

fn is_unresolved_debugging(row: &SessionSignalRow) -> bool {
    row.work_type.as_deref() == Some("debugging")
        && matches!(row.outcome.as_deref(), Some("unresolved") | Some("partial"))
}

const RULES: &[RuleSpec] = &[
    RuleSpec {
        rule: "repeated_context",
        title: "Context is being re-explained",
        body: "Capture project context once in a CLAUDE.md, project memory, or an MCP \
               memory server so sessions start warm instead of re-explaining the basics.",
        threshold: 3,
        evidence_verb: "flagged repeated context",
        matcher: has_repeated_context,
    },
    RuleSpec {
        rule: "many_corrections",
        title: "High correction rate",
        body: "Stating constraints, examples, and the expected output format up front \
               usually works better than iterating through corrections afterward.",
        threshold: 4,
        evidence_verb: "flagged a high correction count",
        matcher: has_many_corrections,
    },
    RuleSpec {
        rule: "unresolved_debugging",
        title: "Debugging sessions ending unresolved",
        body: "Reproducing the failure first, then adding a verification step before \
               ending the session, helps debugging work land as confirmed fixes.",
        threshold: 3,
        evidence_verb: "were debugging sessions that ended unresolved or partial",
        matcher: is_unresolved_debugging,
    },
    RuleSpec {
        rule: "restarted_approach",
        title: "Approaches getting restarted",
        body: "A short planning pass (for example, plan mode) before building tends to \
               surface dead ends earlier than restarting mid-implementation.",
        threshold: 3,
        evidence_verb: "flagged a restarted approach",
        matcher: has_restarted_approach,
    },
];

/// Return the most frequent domain among `rows`, or `"all"` if none of the
/// rows carry domains. Ties break alphabetically for determinism.
fn dominant_domain(rows: &[&SessionSignalRow]) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for row in rows {
        for domain in &row.domains {
            *counts.entry(domain.as_str()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(domain, _)| domain.to_string())
        .unwrap_or_else(|| "all".to_string())
}

/// Compute actionable insights from a window of session signal rows.
///
/// Pure function: the caller fetches the rows (typically the last
/// [`INSIGHT_WINDOW_DAYS`] days) and the set of dismissed insight ids.
/// Output is sorted by evidence strength (triggering row count, descending),
/// with dismissed insights filtered out.
pub fn compute_insights(rows: &[SessionSignalRow], dismissed: &HashSet<String>) -> Vec<Insight> {
    let mut scored: Vec<(usize, Insight)> = Vec::new();

    for spec in RULES {
        let triggering: Vec<&SessionSignalRow> =
            rows.iter().filter(|r| (spec.matcher)(r)).collect();
        let count = triggering.len();
        if count < spec.threshold {
            continue;
        }

        let domain = dominant_domain(&triggering);
        let id = format!("{}:{}", spec.rule, domain);
        if dismissed.contains(&id) {
            continue;
        }

        let evidence = if domain == "all" {
            format!(
                "{count} sessions in the last {INSIGHT_WINDOW_DAYS} days {}",
                spec.evidence_verb
            )
        } else {
            format!(
                "{count} sessions in the last {INSIGHT_WINDOW_DAYS} days {} (mostly {domain})",
                spec.evidence_verb
            )
        };

        scored.push((
            count,
            Insight {
                id,
                rule: spec.rule.to_string(),
                title: spec.title.to_string(),
                body: spec.body.to_string(),
                evidence,
                window_days: INSIGHT_WINDOW_DAYS,
            },
        ));
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, insight)| insight).collect()
}

// ── mechanics insights (ADR 0008) ────────────────────────────────────────────

/// Minimum sessions before any distribution-style mechanics rule may fire.
/// Below this, "patterns" are noise.
const MIN_SESSIONS_FOR_MECHANICS: usize = 6;

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let v: Vec<f64> = values.collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn push_insight(
    out: &mut Vec<Insight>,
    dismissed: &HashSet<String>,
    rule: &str,
    title: &str,
    body: &str,
    evidence: String,
) {
    let id = format!("{rule}:all");
    if !dismissed.contains(&id) {
        out.push(Insight {
            id,
            rule: rule.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            evidence,
            window_days: METRIC_WINDOW_DAYS,
        });
    }
}

/// Compute self-relative insights from objective session mechanics.
///
/// Every rule compares the user against their own history over the window —
/// the evidence strings carry the actual numbers so each card is checkable.
pub fn compute_metric_insights(
    rows: &[SessionMetricsRow],
    dismissed: &HashSet<String>,
) -> Vec<Insight> {
    let mut out = Vec::new();
    if rows.len() < MIN_SESSIONS_FOR_MECHANICS {
        return out;
    }

    // ── debugging_drag: debugging sessions cost ≥2x the back-and-forth ──────
    let debug: Vec<&SessionMetricsRow> = rows
        .iter()
        .filter(|r| r.work_type.as_deref() == Some("debugging"))
        .collect();
    let other: Vec<&SessionMetricsRow> = rows
        .iter()
        .filter(|r| r.work_type.as_deref() != Some("debugging"))
        .collect();
    if debug.len() >= 3 && other.len() >= 3 {
        let debug_avg = mean(debug.iter().map(|r| r.prompts as f64)).unwrap_or(0.0);
        let other_avg = mean(other.iter().map(|r| r.prompts as f64)).unwrap_or(0.0);
        if other_avg > 0.0 && debug_avg >= 2.0 * other_avg {
            push_insight(
                &mut out,
                dismissed,
                "debugging_drag",
                "Debugging costs you the most back-and-forth",
                "Debugging sessions take far more prompting than your other work. \
                 Pasting the full error, the failing input, and what you already ruled \
                 out in the first message usually collapses the search.",
                format!(
                    "debugging sessions average {} prompts vs {} for everything else \
                     ({} vs {} sessions, last {METRIC_WINDOW_DAYS} days)",
                    round1(debug_avg),
                    round1(other_avg),
                    debug.len(),
                    other.len()
                ),
            );
        }
    }

    // ── thin_first_prompt: short openers correlate with more follow-ups ─────
    let mut openers: Vec<i64> = rows
        .iter()
        .filter(|r| r.prompts > 0)
        .map(|r| r.first_prompt_chars)
        .collect();
    if openers.len() >= MIN_SESSIONS_FOR_MECHANICS {
        openers.sort_unstable();
        let median = openers[openers.len() / 2];
        let short: Vec<f64> = rows
            .iter()
            .filter(|r| r.prompts > 0 && r.first_prompt_chars < median)
            .map(|r| r.prompts as f64)
            .collect();
        let long: Vec<f64> = rows
            .iter()
            .filter(|r| r.prompts > 0 && r.first_prompt_chars >= median)
            .map(|r| r.prompts as f64)
            .collect();
        if short.len() >= 3 && long.len() >= 3 {
            let short_avg = mean(short.into_iter()).unwrap_or(0.0);
            let long_avg = mean(long.into_iter()).unwrap_or(0.0);
            if long_avg > 0.0 && short_avg >= 1.5 * long_avg {
                push_insight(
                    &mut out,
                    dismissed,
                    "thin_first_prompt",
                    "Short opening prompts cost you follow-ups",
                    "Sessions you open with a brief prompt take noticeably more \
                     back-and-forth than ones where you front-load context. Leading \
                     with constraints, file references, and the expected outcome pays \
                     for itself.",
                    format!(
                        "sessions opening under {median} characters average {} prompts \
                         vs {} when you front-load (last {METRIC_WINDOW_DAYS} days)",
                        round1(short_avg),
                        round1(long_avg)
                    ),
                );
            }
        }
    }

    // ── interruption_habit: ≥30% of sessions get interrupted mid-task ───────
    let interrupted = rows.iter().filter(|r| r.interruptions > 0).count();
    if interrupted * 10 >= rows.len() * 3 {
        push_insight(
            &mut out,
            dismissed,
            "interruption_habit",
            "You often stop work mid-flight",
            "Interrupting usually means the approach drifted from what you wanted. \
             A short planning pass before execution — or tighter scoping in the \
             first prompt — tends to reduce these course corrections.",
            format!(
                "{interrupted} of {} sessions were interrupted mid-task \
                 (last {METRIC_WINDOW_DAYS} days)",
                rows.len()
            ),
        );
    }

    // ── tool_error_drag: ≥10% of tool calls error ────────────────────────────
    let calls: i64 = rows.iter().map(|r| r.tool_calls).sum();
    let errors: i64 = rows.iter().map(|r| r.tool_errors).sum();
    if calls >= 50 && errors * 10 >= calls {
        push_insight(
            &mut out,
            dismissed,
            "tool_error_drag",
            "Tool calls are failing often",
            "A high tool-error rate is usually environmental — permissions, paths, \
             or missing dependencies — and every failure burns a round trip. Worth \
             fixing the recurring offenders once.",
            format!(
                "{errors} of {calls} tool calls errored \
                 ({}%, last {METRIC_WINDOW_DAYS} days)",
                round1(errors as f64 * 100.0 / calls as f64)
            ),
        );
    }

    // ── recent_slowdown: this fortnight needs ≥1.5x your baseline prompts ───
    if rows.len() >= 10 {
        // "Recent" = the 14 calendar days ending at the newest session.
        let cutoff = rows
            .iter()
            .map(|r| r.day.as_str())
            .max()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .map(|d| (d - chrono::Duration::days(14)).to_string());
        if let Some(cutoff) = cutoff {
            let recent: Vec<f64> = rows
                .iter()
                .filter(|r| r.day > cutoff)
                .map(|r| r.prompts as f64)
                .collect();
            let baseline: Vec<f64> = rows
                .iter()
                .filter(|r| r.day <= cutoff)
                .map(|r| r.prompts as f64)
                .collect();
            if recent.len() >= 4 && baseline.len() >= 6 {
                let recent_avg = mean(recent.into_iter()).unwrap_or(0.0);
                let baseline_avg = mean(baseline.into_iter()).unwrap_or(0.0);
                if baseline_avg > 0.0 && recent_avg >= 1.5 * baseline_avg {
                    push_insight(
                        &mut out,
                        dismissed,
                        "recent_slowdown",
                        "Sessions are taking more effort than your baseline",
                        "Recent sessions need noticeably more prompting than your \
                         norm. Often that's a new domain or a context gap — worth \
                         capturing what you keep re-explaining into project memory.",
                        format!(
                            "recent sessions average {} prompts vs your baseline of {} \
                             (last {METRIC_WINDOW_DAYS} days)",
                            round1(recent_avg),
                            round1(baseline_avg)
                        ),
                    );
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        friction: &[&str],
        work_type: Option<&str>,
        outcome: Option<&str>,
        domains: &[&str],
    ) -> SessionSignalRow {
        SessionSignalRow {
            day: "2026-06-01".to_string(),
            tool: "claude-code".to_string(),
            work_type: work_type.map(str::to_string),
            domains: domains.iter().map(|s| s.to_string()).collect(),
            friction: friction.iter().map(|s| s.to_string()).collect(),
            features: vec![],
            outcome: outcome.map(str::to_string),
        }
    }

    fn ids(insights: &[Insight]) -> Vec<&str> {
        insights.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn empty_input_produces_no_insights() {
        let insights = compute_insights(&[], &HashSet::new());
        assert!(insights.is_empty());
    }

    #[test]
    fn repeated_context_fires_at_three_not_below() {
        let rows: Vec<_> = (0..2)
            .map(|_| row(&["repeated_context"], None, None, &[]))
            .collect();
        assert!(compute_insights(&rows, &HashSet::new()).is_empty());

        let rows: Vec<_> = (0..3)
            .map(|_| row(&["repeated_context"], None, None, &[]))
            .collect();
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(ids(&insights), vec!["repeated_context:all"]);
        assert_eq!(insights[0].rule, "repeated_context");
        assert_eq!(insights[0].window_days, INSIGHT_WINDOW_DAYS);
        assert!(insights[0].evidence.contains("3 sessions"));
    }

    #[test]
    fn many_corrections_fires_at_four_not_below() {
        let rows: Vec<_> = (0..3)
            .map(|_| row(&["many_corrections"], None, None, &[]))
            .collect();
        assert!(compute_insights(&rows, &HashSet::new()).is_empty());

        let rows: Vec<_> = (0..4)
            .map(|_| row(&["many_corrections"], None, None, &[]))
            .collect();
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(ids(&insights), vec!["many_corrections:all"]);
    }

    #[test]
    fn unresolved_debugging_fires_at_three_not_below() {
        let rows = vec![
            row(&[], Some("debugging"), Some("unresolved"), &[]),
            row(&[], Some("debugging"), Some("partial"), &[]),
        ];
        assert!(compute_insights(&rows, &HashSet::new()).is_empty());

        let rows = vec![
            row(&[], Some("debugging"), Some("unresolved"), &[]),
            row(&[], Some("debugging"), Some("partial"), &[]),
            row(&[], Some("debugging"), Some("unresolved"), &[]),
        ];
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(ids(&insights), vec!["unresolved_debugging:all"]);
    }

    #[test]
    fn unresolved_debugging_ignores_resolved_and_other_work_types() {
        let rows = vec![
            row(&[], Some("debugging"), Some("resolved"), &[]),
            row(&[], Some("debugging"), Some("resolved"), &[]),
            row(&[], Some("debugging"), Some("resolved"), &[]),
            row(&[], Some("analysis"), Some("unresolved"), &[]),
            row(&[], Some("analysis"), Some("unresolved"), &[]),
            row(&[], Some("analysis"), Some("unresolved"), &[]),
            row(&[], None, Some("unresolved"), &[]),
        ];
        assert!(compute_insights(&rows, &HashSet::new()).is_empty());
    }

    #[test]
    fn restarted_approach_fires_at_three_not_below() {
        let rows: Vec<_> = (0..2)
            .map(|_| row(&["restarted_approach"], None, None, &[]))
            .collect();
        assert!(compute_insights(&rows, &HashSet::new()).is_empty());

        let rows: Vec<_> = (0..3)
            .map(|_| row(&["restarted_approach"], None, None, &[]))
            .collect();
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(ids(&insights), vec!["restarted_approach:all"]);
    }

    #[test]
    fn dismissed_ids_are_filtered_out() {
        let rows: Vec<_> = (0..3)
            .map(|_| row(&["repeated_context"], None, None, &["rust"]))
            .collect();
        let mut dismissed = HashSet::new();
        dismissed.insert("repeated_context:rust".to_string());
        assert!(compute_insights(&rows, &dismissed).is_empty());
    }

    #[test]
    fn domain_attribution_picks_most_frequent_domain() {
        let rows = vec![
            row(&["repeated_context"], None, None, &["rust", "sqlite"]),
            row(&["repeated_context"], None, None, &["rust"]),
            row(&["repeated_context"], None, None, &["python"]),
        ];
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(ids(&insights), vec!["repeated_context:rust"]);
        assert!(insights[0].evidence.contains("(mostly rust)"));
    }

    #[test]
    fn domain_falls_back_to_all_when_no_domains_present() {
        let rows: Vec<_> = (0..3)
            .map(|_| row(&["repeated_context"], None, None, &[]))
            .collect();
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(insights[0].id, "repeated_context:all");
        assert!(!insights[0].evidence.contains("mostly"));
    }

    #[test]
    fn insights_sorted_by_evidence_count_descending() {
        let mut rows: Vec<_> = (0..3)
            .map(|_| row(&["repeated_context"], None, None, &[]))
            .collect();
        rows.extend((0..5).map(|_| row(&["restarted_approach"], None, None, &[])));
        let insights = compute_insights(&rows, &HashSet::new());
        assert_eq!(
            ids(&insights),
            vec!["restarted_approach:all", "repeated_context:all"]
        );
    }

    #[test]
    fn is_valid_insight_id_accepts_well_formed_ids() {
        assert!(is_valid_insight_id("repeated_context:rust"));
        assert!(is_valid_insight_id("many_corrections:all"));
        assert!(is_valid_insight_id("a-b:c_d0"));
    }

    #[test]
    fn is_valid_insight_id_rejects_malformed_ids() {
        assert!(!is_valid_insight_id(""));
        assert!(!is_valid_insight_id("Has_Uppercase:all"));
        assert!(!is_valid_insight_id("spaces here"));
        assert!(!is_valid_insight_id("drop table;"));
        assert!(!is_valid_insight_id(&"x".repeat(129)));
        assert!(is_valid_insight_id(&"x".repeat(128)));
    }

    // ── mechanics insights ────────────────────────────────────────────────

    fn metric(
        day: &str,
        work_type: Option<&str>,
        prompts: i64,
        first_prompt_chars: i64,
        interruptions: i64,
        tool_calls: i64,
        tool_errors: i64,
    ) -> SessionMetricsRow {
        SessionMetricsRow {
            session_id: format!("s-{day}-{prompts}-{first_prompt_chars}"),
            day: day.to_string(),
            tool: "claude-code".to_string(),
            work_type: work_type.map(str::to_string),
            prompts,
            assistant_turns: prompts * 2,
            duration_min: 10.0,
            interruptions,
            tool_calls,
            tool_errors,
            first_prompt_chars,
            avg_prompt_chars: first_prompt_chars,
        }
    }

    #[test]
    fn too_few_sessions_produce_no_mechanics_insights() {
        let rows: Vec<_> = (0..5)
            .map(|i| metric("2026-06-01", Some("debugging"), 40 + i, 20, 1, 20, 10))
            .collect();
        assert!(compute_metric_insights(&rows, &HashSet::new()).is_empty());
    }

    #[test]
    fn debugging_drag_fires_when_debugging_doubles_prompts() {
        let mut rows: Vec<_> = (0..3)
            .map(|_| metric("2026-06-01", Some("debugging"), 20, 200, 0, 0, 0))
            .collect();
        rows.extend((0..3).map(|_| metric("2026-06-02", Some("creation"), 5, 200, 0, 0, 0)));
        let insights = compute_metric_insights(&rows, &HashSet::new());
        let drag = insights
            .iter()
            .find(|i| i.rule == "debugging_drag")
            .unwrap();
        assert!(
            drag.evidence.contains("20 prompts vs 5"),
            "{}",
            drag.evidence
        );
        assert_eq!(drag.window_days, METRIC_WINDOW_DAYS);
    }

    #[test]
    fn debugging_drag_quiet_when_ratio_is_normal() {
        let mut rows: Vec<_> = (0..3)
            .map(|_| metric("2026-06-01", Some("debugging"), 7, 200, 0, 0, 0))
            .collect();
        rows.extend((0..3).map(|_| metric("2026-06-02", Some("creation"), 5, 200, 0, 0, 0)));
        let insights = compute_metric_insights(&rows, &HashSet::new());
        assert!(!insights.iter().any(|i| i.rule == "debugging_drag"));
    }

    #[test]
    fn thin_first_prompt_fires_when_short_openers_cost_followups() {
        let mut rows: Vec<_> = (0..4)
            .map(|i| metric("2026-06-01", None, 12, 10 + i, 0, 0, 0))
            .collect();
        rows.extend((0..4).map(|i| metric("2026-06-02", None, 4, 500 + i, 0, 0, 0)));
        let insights = compute_metric_insights(&rows, &HashSet::new());
        let thin = insights
            .iter()
            .find(|i| i.rule == "thin_first_prompt")
            .unwrap();
        assert!(thin.evidence.contains("12 prompts"), "{}", thin.evidence);
    }

    #[test]
    fn interruption_habit_fires_at_thirty_percent() {
        // 3 of 10 interrupted → fires.
        let mut rows: Vec<_> = (0..3)
            .map(|i| metric("2026-06-01", None, 5, 200 + i, 2, 0, 0))
            .collect();
        rows.extend((0..7).map(|i| metric("2026-06-02", None, 5, 300 + i, 0, 0, 0)));
        let insights = compute_metric_insights(&rows, &HashSet::new());
        let habit = insights
            .iter()
            .find(|i| i.rule == "interruption_habit")
            .unwrap();
        assert!(habit.evidence.contains("3 of 10"), "{}", habit.evidence);

        // 2 of 10 → quiet.
        let mut rows: Vec<_> = (0..2)
            .map(|i| metric("2026-06-01", None, 5, 200 + i, 1, 0, 0))
            .collect();
        rows.extend((0..8).map(|i| metric("2026-06-02", None, 5, 300 + i, 0, 0, 0)));
        let insights = compute_metric_insights(&rows, &HashSet::new());
        assert!(!insights.iter().any(|i| i.rule == "interruption_habit"));
    }

    #[test]
    fn tool_error_drag_fires_at_ten_percent_of_fifty_calls() {
        let rows: Vec<_> = (0..6)
            .map(|i| metric("2026-06-01", None, 5, 200 + i, 0, 10, 1))
            .collect();
        let insights = compute_metric_insights(&rows, &HashSet::new());
        let drag = insights
            .iter()
            .find(|i| i.rule == "tool_error_drag")
            .unwrap();
        assert!(drag.evidence.contains("6 of 60"), "{}", drag.evidence);
    }

    #[test]
    fn recent_slowdown_fires_against_own_baseline() {
        // Baseline: 8 sessions across old days at 4 prompts.
        let mut rows: Vec<_> = (0..8)
            .map(|i| metric(&format!("2026-03-{:02}", i + 1), None, 4, 200 + i, 0, 0, 0))
            .collect();
        // Recent: 4 sessions at 9 prompts.
        rows.extend(
            (0..4).map(|i| metric(&format!("2026-06-{:02}", i + 1), None, 9, 200 + i, 0, 0, 0)),
        );
        let insights = compute_metric_insights(&rows, &HashSet::new());
        let slow = insights
            .iter()
            .find(|i| i.rule == "recent_slowdown")
            .unwrap();
        assert!(slow.evidence.contains("9 prompts"), "{}", slow.evidence);
    }

    #[test]
    fn dismissed_mechanics_insights_are_filtered() {
        let mut rows: Vec<_> = (0..3)
            .map(|_| metric("2026-06-01", Some("debugging"), 20, 200, 0, 0, 0))
            .collect();
        rows.extend((0..3).map(|_| metric("2026-06-02", Some("creation"), 5, 200, 0, 0, 0)));
        let mut dismissed = HashSet::new();
        dismissed.insert("debugging_drag:all".to_string());
        let insights = compute_metric_insights(&rows, &dismissed);
        assert!(!insights.iter().any(|i| i.rule == "debugging_drag"));
    }
}
