/// Local insights rules engine — turns accumulated session signals into
/// actionable "Craft" insight cards.
///
/// Pure functions over already-derived `SessionSignalRow` data: no I/O, no
/// raw content, trivially testable. See ADR 0005 (derived friction signals).
use std::collections::{HashMap, HashSet};

use super::queries::SessionSignalRow;

/// The rolling window (in days) that insights are computed over.
pub const INSIGHT_WINDOW_DAYS: i64 = 30;

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
}
