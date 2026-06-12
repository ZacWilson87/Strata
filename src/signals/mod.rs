/// Workflow signal collection and in-memory processing.
///
/// Raw content is consumed and discarded here — only derived `WorkflowSignal`s
/// and `SkillTag`s cross the module boundary.
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::private_mode::{DerivedSummary, RawSignal, SkillTag, WorkType};

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
    /// One-sentence derived summary provided by the AI tool. Never raw content.
    pub topic_summary: Option<String>,
    /// Optional stable identifier grouping work units from the same conversation.
    pub conversation_id: Option<String>,
    /// Whitelisted friction flags reported by the AI tool (ADR 0005).
    pub friction_signals: Vec<String>,
    /// Sanitized names of tool features the session exercised.
    pub features_used: Vec<String>,
    /// Validated session outcome: "resolved" | "partial" | "unresolved".
    pub outcome: Option<String>,
}

/// Payload received from an AI client via the MCP ingest endpoint.
///
/// The `content` field holds raw user context — processed in-memory and discarded.
/// When `work_type`, `domain_tags`, or `topic_summary` are provided by the AI tool,
/// `content` may be empty — the AI has already done the classification.
///
/// Intended to be called once per completed work unit (task, analysis, debug session, etc.),
/// not once per conversation. Multiple calls per conversation are expected and correct.
#[derive(Debug, serde::Deserialize)]
pub struct IngestPayload {
    pub tool_used: String,
    /// Raw content from the AI session. Never persisted. May be empty when pre-classified.
    #[serde(default)]
    pub content: String,
    pub domain_hint: Option<String>,
    /// Work type pre-classified by the AI tool (e.g. "analysis", "debugging").
    /// When present, skips structural fallback detection.
    pub work_type: Option<String>,
    /// Domain tags pre-classified by the AI tool (e.g. ["food_science", "fermentation"]).
    /// Stored with a `dt:` prefix. Universal — works for any domain.
    pub domain_tags: Option<Vec<String>>,
    /// One-sentence derived summary from the AI tool. No PII, no raw content.
    /// Stored in preferences under a timestamped key; max 50 retained.
    pub topic_summary: Option<String>,
    /// Optional stable identifier for the conversation this work unit belongs to.
    /// Allows multiple work units from the same conversation to be grouped later.
    pub conversation_id: Option<String>,
    /// Derived friction flags from the AI tool (e.g. ["repeated_context"]).
    /// Validated against a whitelist — unknown flags are dropped.
    pub friction_signals: Option<Vec<String>>,
    /// Tool features the session exercised (e.g. ["plan_mode", "subagents"]).
    pub features_used: Option<Vec<String>>,
    /// How the session ended: "resolved", "partial", or "unresolved".
    pub outcome: Option<String>,
}

/// Maximum tags stored from a single ingest call. Bounds DB growth and the
/// O(n²) co-occurrence loop against malicious or buggy clients.
const MAX_TAGS_PER_INGEST: usize = 32;
/// Maximum AI-provided domain tags considered per call.
const MAX_DOMAIN_TAGS: usize = 10;
/// Maximum bytes of raw content processed (in-memory only, but scanning an
/// unbounded string is a cheap DoS vector).
const MAX_CONTENT_BYTES: usize = 256 * 1024;
/// Maximum length of a single client-supplied tag.
const MAX_TAG_CHARS: usize = 64;
/// Maximum length of a client-supplied conversation id.
const MAX_CONVERSATION_ID_CHARS: usize = 64;
/// Maximum friction flags / feature names retained per ingest.
const MAX_SESSION_SIGNALS: usize = 8;

/// Canonical friction vocabulary (ADR 0005). Flags outside this list are dropped —
/// a fixed enum keeps the insights rules deterministic and prevents freeform
/// content from leaking in through the side door.
pub const FRICTION_WHITELIST: &[&str] = &[
    "repeated_context",
    "many_corrections",
    "restarted_approach",
    "manual_repetition",
    "context_lost",
];

/// Valid session outcomes.
pub const OUTCOME_WHITELIST: &[&str] = &["resolved", "partial", "unresolved"];

/// Validate and normalise a client-supplied tag fragment.
///
/// Returns `None` for empty or oversized tags, or tags containing characters
/// outside `[a-z0-9_+#.-]`. Colons are rejected so clients cannot forge the
/// reserved namespace prefixes (`wt:`, `dt:`, `tool:`).
fn sanitize_tag(raw: &str) -> Option<String> {
    let tag = raw.trim().to_lowercase();
    if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS {
        return None;
    }
    let valid = tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '#' | '.'));
    valid.then_some(tag)
}

/// Validate and normalise a user-preference key (the `strata_set_preference`
/// write path). Same charset rules as tags — lowercase `[a-z0-9_.-]`, max 64
/// chars, colons rejected so clients cannot write into Strata's internal
/// preference namespaces.
pub fn sanitize_preference_key(raw: &str) -> Option<String> {
    let key = raw.trim().to_lowercase();
    if key.is_empty() || key.chars().count() > MAX_TAG_CHARS {
        return None;
    }
    let valid = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    valid.then_some(key)
}

/// Sanitize a conversation id: keep `[A-Za-z0-9_-]`, cap length, drop if empty.
fn sanitize_conversation_id(raw: &str) -> Option<String> {
    let id: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(MAX_CONVERSATION_ID_CHARS)
        .collect();
    (!id.is_empty()).then_some(id)
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

/// Detect work type from raw content using structural pattern matching.
///
/// This is a fallback used when the AI tool has not pre-classified the payload.
/// The raw content is consumed here — it cannot leak after this call.
pub fn detect_work_type(signal: &RawSignal) -> WorkType {
    let content = signal.0.to_lowercase();

    // Ordered by specificity — first match wins.
    let patterns: &[(&[&str], WorkType)] = &[
        (
            &[
                "error",
                "exception",
                "traceback",
                "not working",
                "failed",
                "broken",
                "bug",
                "fix",
                "crash",
            ],
            WorkType::Debugging,
        ),
        (
            &[
                "analyze",
                "analysis",
                "data",
                "results",
                "findings",
                "correlation",
                "trend",
                "pattern",
                "statistics",
                "metrics",
            ],
            WorkType::Analysis,
        ),
        (
            &[
                "review", "feedback", "check", "validate", "verify", "audit", "approve",
            ],
            WorkType::Review,
        ),
        (
            &[
                "design",
                "plan",
                "architect",
                "structure",
                "approach",
                "strategy",
                "roadmap",
                "scope",
            ],
            WorkType::Planning,
        ),
        (
            &[
                "what is",
                "how does",
                "explain",
                "why does",
                "understand",
                "learn",
                "research",
                "investigate",
            ],
            WorkType::Research,
        ),
        (
            &[
                "create",
                "build",
                "implement",
                "write",
                "generate",
                "make",
                "develop",
                "add",
            ],
            WorkType::Creation,
        ),
    ];

    for (terms, work_type) in patterns {
        if terms.iter().any(|t| content.contains(t)) {
            return work_type.clone();
        }
    }

    WorkType::Other
}

/// Process an ingest payload into a `WorkflowSignal`, discarding raw content.
///
/// When `work_type`, `domain_tags`, or `topic_summary` are pre-classified by the AI tool,
/// they are used directly. Keyword extraction still runs on non-empty `content` for
/// technology skill tags (rust, python, etc.), but is skipped when content is empty.
///
/// All client-supplied tag inputs are validated (charset, length, count caps);
/// invalid entries are skipped rather than failing the whole ingest.
pub fn process_ingest(payload: IngestPayload) -> WorkflowSignal {
    // Bound raw content size before any processing.
    let mut content = payload.content;
    if content.len() > MAX_CONTENT_BYTES {
        let mut end = MAX_CONTENT_BYTES;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        content.truncate(end);
    }

    let raw = RawSignal::new(content.clone());
    let mut tags: Vec<SkillTag> = Vec::new();

    // --- Tool usage tracking ---
    // Pushed first so the tag cap can never silently drop tool attribution.
    // Whitespace is normalised to hyphens ("Claude Desktop" → "claude-desktop")
    // before validation so a space doesn't cost the user attribution.
    let tool_used =
        sanitize_tag(&payload.tool_used.replace(char::is_whitespace, "-")).unwrap_or_default();
    if !tool_used.is_empty() {
        tags.push(SkillTag::new(format!("tool:{tool_used}")));
    }

    // --- Technology skill tags (keyword extraction) ---
    // Only run if content is non-empty; AI-pre-classified payloads may omit content.
    if !content.is_empty() {
        for tag in extract_skills(RawSignal::new(content)) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }

    // --- Work type tag ---
    // Use AI-provided work_type if present; otherwise detect from content structure.
    let work_type = if let Some(ref wt) = payload.work_type {
        WorkType::from_str_loose(wt)
    } else if !raw.0.is_empty() {
        detect_work_type(&raw)
    } else {
        WorkType::Other
    };
    // Only store non-trivial work types to keep the graph clean.
    if work_type != WorkType::Other {
        let wt_tag = work_type.as_tag();
        if !tags.contains(&wt_tag) {
            tags.push(wt_tag);
        }
    }

    // --- Domain tags (AI-provided, universal vocabulary) ---
    if let Some(ref domain_tags) = payload.domain_tags {
        for dt in domain_tags.iter().take(MAX_DOMAIN_TAGS) {
            let Some(clean) = sanitize_tag(dt) else {
                continue;
            };
            let dt_tag = SkillTag::new(format!("dt:{clean}"));
            if !tags.contains(&dt_tag) {
                tags.push(dt_tag);
            }
        }
    }

    // --- Legacy domain hint ---
    let domain_hint = payload.domain_hint.as_deref().and_then(sanitize_tag);
    if let Some(ref hint) = domain_hint {
        let hint_tag = SkillTag::new(hint.clone());
        if !tags.contains(&hint_tag) {
            tags.push(hint_tag);
        }
    }

    tags.truncate(MAX_TAGS_PER_INGEST);

    // --- Session signals (ADR 0005) ---
    // Friction flags must match the canonical vocabulary exactly; feature names
    // go through the same tag sanitizer as everything else client-supplied.
    let mut friction_signals: Vec<String> = Vec::new();
    if let Some(ref flags) = payload.friction_signals {
        for flag in flags.iter().take(MAX_SESSION_SIGNALS) {
            let f = flag.trim().to_lowercase();
            if FRICTION_WHITELIST.contains(&f.as_str()) && !friction_signals.contains(&f) {
                friction_signals.push(f);
            }
        }
    }
    let mut features_used: Vec<String> = Vec::new();
    if let Some(ref features) = payload.features_used {
        for feature in features.iter().take(MAX_SESSION_SIGNALS) {
            if let Some(f) = sanitize_tag(feature) {
                if !features_used.contains(&f) {
                    features_used.push(f);
                }
            }
        }
    }
    let outcome = payload.outcome.as_deref().and_then(|o| {
        let o = o.trim().to_lowercase();
        OUTCOME_WHITELIST.contains(&o.as_str()).then_some(o)
    });

    WorkflowSignal {
        timestamp: Utc::now(),
        tool_used,
        domain_hint,
        skill_tags: tags,
        topic_summary: payload.topic_summary,
        conversation_id: payload
            .conversation_id
            .as_deref()
            .and_then(sanitize_conversation_id),
        friction_signals,
        features_used,
        outcome,
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
            content: content.into(),
            domain_hint: hint.map(Into::into),
            work_type: None,
            domain_tags: None,
            topic_summary: None,
            conversation_id: None,
            friction_signals: None,
            features_used: None,
            outcome: None,
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

    // --- detect_work_type coverage ---

    #[test]
    fn detect_work_type_debugging_patterns() {
        for keyword in &[
            "error",
            "exception",
            "traceback",
            "not working",
            "failed",
            "broken",
            "bug",
            "fix",
            "crash",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Debugging,
                "keyword '{keyword}' should produce Debugging"
            );
        }
    }

    #[test]
    fn detect_work_type_analysis_patterns() {
        for keyword in &[
            "analyze",
            "analysis",
            "data",
            "results",
            "findings",
            "correlation",
            "trend",
            "statistics",
            "metrics",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Analysis,
                "keyword '{keyword}' should produce Analysis"
            );
        }
    }

    #[test]
    fn detect_work_type_review_patterns() {
        for keyword in &[
            "review", "feedback", "validate", "verify", "audit", "approve",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Review,
                "keyword '{keyword}' should produce Review"
            );
        }
    }

    #[test]
    fn detect_work_type_planning_patterns() {
        for keyword in &[
            "design",
            "plan",
            "architect",
            "structure",
            "approach",
            "strategy",
            "roadmap",
            "scope",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Planning,
                "keyword '{keyword}' should produce Planning"
            );
        }
    }

    #[test]
    fn detect_work_type_research_patterns() {
        for keyword in &[
            "what is",
            "how does",
            "explain",
            "why does",
            "understand",
            "learn",
            "research",
            "investigate",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Research,
                "keyword '{keyword}' should produce Research"
            );
        }
    }

    #[test]
    fn detect_work_type_creation_patterns() {
        for keyword in &[
            "create",
            "build",
            "implement",
            "generate",
            "make",
            "develop",
        ] {
            let raw = RawSignal::new(keyword.to_string());
            assert_eq!(
                detect_work_type(&raw),
                WorkType::Creation,
                "keyword '{keyword}' should produce Creation"
            );
        }
    }

    #[test]
    fn detect_work_type_empty_returns_other() {
        let raw = RawSignal::new(String::new());
        assert_eq!(detect_work_type(&raw), WorkType::Other);
    }

    #[test]
    fn detect_work_type_unrecognized_returns_other() {
        let raw = RawSignal::new("hello world nothing specific here".into());
        assert_eq!(detect_work_type(&raw), WorkType::Other);
    }

    #[test]
    fn detect_work_type_first_match_wins() {
        // "error" (Debugging) is checked before "data" (Analysis)
        let raw = RawSignal::new("error in the data".into());
        assert_eq!(detect_work_type(&raw), WorkType::Debugging);
    }

    // --- extract_skills extended coverage ---

    #[test]
    fn extract_skills_all_remaining_keywords() {
        let cases: &[(&str, &str)] = &[
            ("python script", "python"),
            ("javascript code", "javascript"),
            ("react component", "react"),
            ("api endpoint", "api"),
            ("testing framework", "testing"),
            ("refactor this module", "refactoring"),
            ("debug the issue", "debugging"),
            ("performance optimization", "performance"),
            ("security audit", "security"),
            ("architecture overview", "architecture"),
            ("docker container", "docker"),
            ("git commit history", "git"),
            ("ci pipeline config", "ci-cd"),
            ("deploy to production", "deployment"),
            ("cli tool usage", "cli"),
        ];
        for (content, expected_tag) in cases {
            let raw = RawSignal::new(content.to_string());
            let tags = extract_skills(raw);
            assert!(
                tags.contains(&SkillTag::new(*expected_tag)),
                "content '{content}' should produce tag '{expected_tag}'"
            );
        }
    }

    #[test]
    fn extract_skills_is_case_insensitive() {
        let raw = RawSignal::new("RUST ASYNC SQL".into());
        let tags = extract_skills(raw);
        assert!(tags.contains(&SkillTag::new("rust")));
        assert!(tags.contains(&SkillTag::new("async")));
        assert!(tags.contains(&SkillTag::new("sql")));
    }

    // --- process_ingest edge cases ---

    #[test]
    fn process_ingest_preclassified_work_type_used() {
        let mut payload = make_payload("", "claude", None);
        payload.work_type = Some("analysis".into());
        let signal = process_ingest(payload);
        assert!(signal.skill_tags.contains(&SkillTag::new("wt:analysis")));
    }

    #[test]
    fn process_ingest_other_work_type_not_stored() {
        // WorkType::Other is not stored — graph stays clean
        let payload = make_payload("hello world nothing specific", "claude", None);
        let signal = process_ingest(payload);
        assert!(!signal
            .skill_tags
            .iter()
            .any(|t| t.as_str().starts_with("wt:")));
    }

    #[test]
    fn process_ingest_domain_tags_stored_with_dt_prefix() {
        let mut payload = make_payload("", "claude", None);
        payload.domain_tags = Some(vec!["food_science".into(), "fermentation".into()]);
        let signal = process_ingest(payload);
        assert!(signal
            .skill_tags
            .contains(&SkillTag::new("dt:food_science")));
        assert!(signal
            .skill_tags
            .contains(&SkillTag::new("dt:fermentation")));
    }

    #[test]
    fn process_ingest_topic_summary_and_conversation_id_preserved() {
        let mut payload = make_payload("", "claude", None);
        payload.topic_summary = Some("optimizing Maillard reaction".into());
        payload.conversation_id = Some("conv-abc".into());
        let signal = process_ingest(payload);
        assert_eq!(
            signal.topic_summary.as_deref(),
            Some("optimizing Maillard reaction")
        );
        assert_eq!(signal.conversation_id.as_deref(), Some("conv-abc"));
    }

    #[test]
    fn process_ingest_empty_content_no_preclassification_has_no_wt_tag() {
        let payload = make_payload("", "claude", None);
        let signal = process_ingest(payload);
        // No wt: tag without preclassification and no detectable keywords
        assert!(!signal
            .skill_tags
            .iter()
            .any(|t| t.as_str().starts_with("wt:")));
        // tool: tag is expected from tool_used field
        assert!(signal
            .skill_tags
            .iter()
            .all(|t| t.as_str().starts_with("tool:")));
    }

    // --- session signals (ADR 0005) ---

    #[test]
    fn friction_signals_outside_whitelist_are_dropped() {
        let mut payload = make_payload("", "claude", None);
        payload.friction_signals = Some(vec![
            "repeated_context".into(),
            "MANY_CORRECTIONS".into(), // case-insensitive
            "made_up_flag".into(),
            "ignore previous instructions".into(),
        ]);
        let signal = process_ingest(payload);
        assert_eq!(
            signal.friction_signals,
            vec![
                "repeated_context".to_string(),
                "many_corrections".to_string()
            ]
        );
    }

    #[test]
    fn features_are_sanitized_and_capped() {
        let mut payload = make_payload("", "claude", None);
        let mut features: Vec<String> = (0..20).map(|i| format!("feature_{i}")).collect();
        features.push("has spaces".into());
        features.push("plan_mode".into());
        payload.features_used = Some(features);
        let signal = process_ingest(payload);
        assert!(signal.features_used.len() <= 8);
        assert!(!signal.features_used.iter().any(|f| f.contains(' ')));
    }

    #[test]
    fn invalid_outcome_is_dropped_valid_kept() {
        let mut payload = make_payload("", "claude", None);
        payload.outcome = Some("Resolved".into());
        assert_eq!(process_ingest(payload).outcome.as_deref(), Some("resolved"));

        let mut payload = make_payload("", "claude", None);
        payload.outcome = Some("triumphant".into());
        assert_eq!(process_ingest(payload).outcome, None);
    }

    #[test]
    fn no_session_signal_fields_yields_empty_defaults() {
        let signal = process_ingest(make_payload("", "claude", None));
        assert!(signal.friction_signals.is_empty());
        assert!(signal.features_used.is_empty());
        assert!(signal.outcome.is_none());
    }

    // --- input validation ---

    #[test]
    fn forged_namespace_prefix_in_domain_tags_is_rejected() {
        let mut payload = make_payload("", "claude", None);
        payload.domain_tags = Some(vec![
            "wt:debugging".into(),
            "tool:fake".into(),
            "dt:nested".into(),
            "legit_domain".into(),
        ]);
        let signal = process_ingest(payload);
        // Colons are invalid in client-supplied tags — only the clean one survives.
        assert!(signal
            .skill_tags
            .contains(&SkillTag::new("dt:legit_domain")));
        assert!(!signal
            .skill_tags
            .iter()
            .any(|t| t.as_str() == "wt:debugging"));
        assert!(!signal.skill_tags.iter().any(|t| t.as_str() == "tool:fake"));
        assert!(!signal
            .skill_tags
            .iter()
            .any(|t| t.as_str().contains("nested")));
    }

    #[test]
    fn forged_prefix_in_domain_hint_is_rejected() {
        let payload = make_payload("", "claude", Some("wt:planning"));
        let signal = process_ingest(payload);
        assert!(
            !signal
                .skill_tags
                .iter()
                .any(|t| t.as_str() == "wt:planning"),
            "domain_hint must not be able to forge a work-type tag"
        );
    }

    #[test]
    fn domain_tags_capped() {
        let mut payload = make_payload("", "claude", None);
        payload.domain_tags = Some((0..100).map(|i| format!("domain_{i}")).collect());
        let signal = process_ingest(payload);
        let dt_count = signal
            .skill_tags
            .iter()
            .filter(|t| t.as_str().starts_with("dt:"))
            .count();
        assert!(dt_count <= 10, "domain tags must be capped, got {dt_count}");
    }

    #[test]
    fn oversized_and_invalid_tags_are_skipped() {
        let mut payload = make_payload("", "claude", None);
        payload.domain_tags = Some(vec![
            "x".repeat(200),
            "has spaces in it".into(),
            "<script>".into(),
            "".into(),
            "ok-tag".into(),
        ]);
        let signal = process_ingest(payload);
        let dts: Vec<&str> = signal
            .skill_tags
            .iter()
            .filter(|t| t.as_str().starts_with("dt:"))
            .map(|t| t.as_str())
            .collect();
        assert_eq!(dts, vec!["dt:ok-tag"]);
    }

    #[test]
    fn tool_used_with_spaces_keeps_attribution() {
        let payload = make_payload("", "Claude Desktop", None);
        let signal = process_ingest(payload);
        assert_eq!(signal.tool_used, "claude-desktop");
        assert!(signal
            .skill_tags
            .contains(&SkillTag::new("tool:claude-desktop")));
    }

    #[test]
    fn conversation_id_is_sanitized() {
        let mut payload = make_payload("", "claude", None);
        payload.conversation_id = Some("conv:abc/../def 123".into());
        let signal = process_ingest(payload);
        let id = signal.conversation_id.unwrap();
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')),
            "conversation_id should be stripped to a safe charset: {id}"
        );
    }

    #[test]
    fn oversized_content_is_truncated_not_rejected() {
        // 300 KB of filler ending with a keyword that lands inside the cap.
        let mut content = "rust ".to_string();
        content.push_str(&"y".repeat(300 * 1024));
        let payload = make_payload(&content, "claude", None);
        let signal = process_ingest(payload);
        assert!(signal.skill_tags.contains(&SkillTag::new("rust")));
    }

    #[test]
    fn total_tags_capped_per_ingest() {
        // Max out every tag source at once.
        let mut payload = make_payload(
            "rust async sql database python typescript javascript react api testing \
             refactor debug performance security architecture docker git ci deploy cli error",
            "claude",
            Some("extra_hint"),
        );
        payload.domain_tags = Some((0..50).map(|i| format!("domain_{i}")).collect());
        let signal = process_ingest(payload);
        assert!(
            signal.skill_tags.len() <= 32,
            "tag count must be bounded, got {}",
            signal.skill_tags.len()
        );
        // Tool attribution must survive the cap.
        assert!(signal.skill_tags.contains(&SkillTag::new("tool:claude")));
    }

    #[test]
    fn process_batch_empty_input_returns_empty() {
        let signals = process_batch(vec![]);
        assert!(signals.is_empty());
    }

    #[test]
    fn aggregate_skill_counts_empty_signals_returns_empty() {
        let counts = aggregate_skill_counts(&[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn summarize_empty_signals_returns_empty_string() {
        let summary = summarize(&[]);
        assert_eq!(summary.as_str(), "");
    }

    #[test]
    fn summarize_takes_top_five_only() {
        // 6 distinct tags across two payloads; summarize must cap at 5
        let payloads = vec![
            make_payload("rust async sql database python", "claude", None),
            make_payload("rust async sql database python", "claude", None),
            make_payload("typescript javascript react api testing", "claude", None),
        ];
        let signals = process_batch(payloads);
        let summary = summarize(&signals);
        // Non-empty summary has at most 5 comma-separated entries
        let count = summary.as_str().split(", ").count();
        assert!(
            count <= 5,
            "expected ≤ 5 tags, got {count}: {}",
            summary.as_str()
        );
    }
}
