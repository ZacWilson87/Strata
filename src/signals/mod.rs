/// Workflow signal collection and in-memory processing.
///
/// Raw content is consumed and discarded here — only derived `WorkflowSignal`s
/// and `SkillTag`s cross the module boundary.
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::private_mode::{DerivedSummary, RawSignal, SkillTag};

/// A processed, privacy-safe workflow event. Contains no raw content.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowSignal {
    pub timestamp: DateTime<Utc>,
    /// Name of the AI tool that generated this signal (e.g., "claude", "cursor").
    pub tool_used: String,
    /// Optional domain context provided by the client (e.g., "rust", "database").
    pub domain_hint: Option<String>,
    /// Skill tags extracted from the raw signal.
    pub skill_tags: Vec<SkillTag>,
}

/// Payload received from an AI client via the MCP ingest endpoint.
/// The `content` field holds raw user context — processed in-memory and discarded.
#[derive(Debug, serde::Deserialize)]
pub struct IngestPayload {
    pub tool_used: String,
    pub domain_hint: Option<String>,
    /// Raw content from the AI session. Never persisted.
    pub content: String,
}

/// Extract skill tags from a `RawSignal` using keyword heuristics.
///
/// The raw signal is consumed — it cannot leak after this call.
pub fn extract_skills(signal: RawSignal) -> Vec<SkillTag> {
    let content = signal.0.to_lowercase();
    let mut tags = Vec::new();

    let keywords: &[(&str, &str)] = &[
        ("rust", "rust"),
        ("async", "async"),
        ("sql", "sql"),
        ("database", "database"),
        ("python", "python"),
        ("typescript", "typescript"),
        ("javascript", "javascript"),
        ("react", "react"),
        ("api", "api"),
        ("testing", "testing"),
        ("refactor", "refactoring"),
        ("debug", "debugging"),
        ("performance", "performance"),
        ("security", "security"),
        ("architecture", "architecture"),
        ("docker", "docker"),
        ("git", "git"),
        ("ci", "ci-cd"),
        ("deploy", "deployment"),
        ("cli", "cli"),
    ];

    for (keyword, tag) in keywords {
        if content.contains(keyword) {
            tags.push(SkillTag::new(*tag));
        }
    }

    tags
}

/// Process an ingest payload into a `WorkflowSignal`, discarding raw content.
pub fn process_ingest(payload: IngestPayload) -> WorkflowSignal {
    let raw = RawSignal::new(payload.content);
    // Domain hint also contributes tags
    let mut tags = extract_skills(raw);

    if let Some(ref hint) = payload.domain_hint {
        let hint_tag = SkillTag::new(hint.to_lowercase());
        if !tags.contains(&hint_tag) {
            tags.push(hint_tag);
        }
    }

    WorkflowSignal {
        timestamp: Utc::now(),
        tool_used: payload.tool_used,
        domain_hint: payload.domain_hint,
        skill_tags: tags,
    }
}

/// Process a batch of payloads, returning derived signals only.
pub fn process_batch(payloads: Vec<IngestPayload>) -> Vec<WorkflowSignal> {
    payloads.into_iter().map(process_ingest).collect()
}

/// Aggregate skill tag frequencies across a set of signals.
pub fn aggregate_skill_counts(signals: &[WorkflowSignal]) -> HashMap<SkillTag, usize> {
    let mut counts: HashMap<SkillTag, usize> = HashMap::new();
    for signal in signals {
        for tag in &signal.skill_tags {
            *counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Summarize signals into a human-readable `DerivedSummary`.
pub fn summarize(signals: &[WorkflowSignal]) -> DerivedSummary {
    let counts = aggregate_skill_counts(signals);
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<String> = sorted.into_iter().take(5).map(|(t, _)| t.0).collect();
    DerivedSummary::new(top.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(content: &str, tool: &str, hint: Option<&str>) -> IngestPayload {
        IngestPayload {
            tool_used: tool.into(),
            domain_hint: hint.map(Into::into),
            content: content.into(),
        }
    }

    #[test]
    fn extract_skills_finds_rust_keyword() {
        let raw = RawSignal::new("Help me write a Rust async function".into());
        let tags = extract_skills(raw);
        assert!(tags.contains(&SkillTag::new("rust")));
        assert!(tags.contains(&SkillTag::new("async")));
    }

    #[test]
    fn extract_skills_empty_content_returns_empty() {
        let raw = RawSignal::new(String::new());
        let tags = extract_skills(raw);
        assert!(tags.is_empty());
    }

    #[test]
    fn process_ingest_discards_raw_content() {
        let payload = make_payload("Write a SQL query for me", "claude", Some("database"));
        let signal = process_ingest(payload);
        // WorkflowSignal has no raw content field — only derived tags
        assert!(signal.skill_tags.contains(&SkillTag::new("sql")));
        assert!(signal.skill_tags.contains(&SkillTag::new("database")));
    }

    #[test]
    fn process_ingest_domain_hint_added_as_tag() {
        let payload = make_payload("", "cursor", Some("typescript"));
        let signal = process_ingest(payload);
        assert!(signal.skill_tags.contains(&SkillTag::new("typescript")));
    }

    #[test]
    fn process_ingest_no_duplicate_tags() {
        let payload = make_payload("typescript code", "claude", Some("typescript"));
        let signal = process_ingest(payload);
        let ts_count = signal
            .skill_tags
            .iter()
            .filter(|t| t.as_str() == "typescript")
            .count();
        assert_eq!(ts_count, 1);
    }

    #[test]
    fn process_batch_returns_one_signal_per_payload() {
        let payloads = vec![
            make_payload("rust code", "claude", None),
            make_payload("python script", "cursor", None),
        ];
        let signals = process_batch(payloads);
        assert_eq!(signals.len(), 2);
    }

    #[test]
    fn aggregate_skill_counts_sums_correctly() {
        let payloads = vec![
            make_payload("rust async", "claude", None),
            make_payload("rust database", "claude", None),
        ];
        let signals = process_batch(payloads);
        let counts = aggregate_skill_counts(&signals);
        assert_eq!(*counts.get(&SkillTag::new("rust")).unwrap(), 2);
    }

    #[test]
    fn summarize_returns_derived_summary_no_raw_content() {
        let payloads = vec![make_payload(
            "sensitive user data in prompt",
            "claude",
            Some("rust"),
        )];
        let signals = process_batch(payloads);
        let summary = summarize(&signals);
        // Summary must not contain the raw content
        assert!(!summary.as_str().contains("sensitive user data in prompt"));
    }
}
