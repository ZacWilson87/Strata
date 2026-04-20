//! Privacy newtype wrappers enforcing compile-time boundaries.
//!
//! `RawSignal` is intentionally non-serializable — it must never reach disk or
//! cross into the graph layer. Only `DerivedSummary` and `SkillTag` may persist.

/// Raw user content received from AI clients. Processed in-memory only; never stored.
pub struct RawSignal(pub(crate) String);

impl RawSignal {
    pub fn new(content: String) -> Self {
        Self(content)
    }
}

/// A derived, privacy-safe summary produced from one or more `RawSignal`s.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DerivedSummary(pub String);

impl DerivedSummary {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The type of work being performed in a session. Derived by the AI tool, never from raw content.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WorkType {
    Research,  // learning, reading, investigating
    Analysis,  // interpreting data, results, findings
    Creation,  // building, writing, generating
    Debugging, // fixing errors, troubleshooting
    Review,    // reviewing, validating, checking
    Planning,  // designing, architecting, scoping
    #[default]
    Other,
}

impl std::fmt::Display for WorkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WorkType::Research => "research",
            WorkType::Analysis => "analysis",
            WorkType::Creation => "creation",
            WorkType::Debugging => "debugging",
            WorkType::Review => "review",
            WorkType::Planning => "planning",
            WorkType::Other => "other",
        };
        write!(f, "{s}")
    }
}

impl WorkType {
    /// Parse from a string, case-insensitive. Falls back to `Other`.
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "research" => WorkType::Research,
            "analysis" | "analyze" => WorkType::Analysis,
            "creation" | "create" | "building" | "build" => WorkType::Creation,
            "debugging" | "debug" => WorkType::Debugging,
            "review" => WorkType::Review,
            "planning" | "plan" => WorkType::Planning,
            _ => WorkType::Other,
        }
    }

    /// The tag prefix used when stored in the skills table.
    pub fn as_tag(&self) -> SkillTag {
        SkillTag::new(format!("wt:{self}"))
    }
}

/// A skill tag extracted from workflow signals. Safe to persist.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SkillTag(pub String);

impl SkillTag {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SkillTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_signal_not_persisted() {
        // RawSignal intentionally omits Serialize — confirms invariant by construction.
        let signal = RawSignal::new("sensitive prompt content".into());
        // Can only access inner value within crate (pub(crate))
        assert!(!signal.0.is_empty());
    }

    #[test]
    fn derived_summary_roundtrips_json() {
        let s = DerivedSummary::new("Rust, async, SQLite".into());
        let json = serde_json::to_string(&s).unwrap();
        let back: DerivedSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(s.as_str(), back.as_str());
    }

    #[test]
    fn skill_tag_display() {
        let tag = SkillTag::new("rust");
        assert_eq!(tag.to_string(), "rust");
    }

    #[test]
    fn skill_tag_equality() {
        assert_eq!(SkillTag::new("async"), SkillTag::new("async"));
        assert_ne!(SkillTag::new("async"), SkillTag::new("sync"));
    }

    #[test]
    fn work_type_from_str_loose_all_variants() {
        assert_eq!(WorkType::from_str_loose("research"), WorkType::Research);
        assert_eq!(WorkType::from_str_loose("analysis"), WorkType::Analysis);
        assert_eq!(WorkType::from_str_loose("analyze"), WorkType::Analysis);
        assert_eq!(WorkType::from_str_loose("creation"), WorkType::Creation);
        assert_eq!(WorkType::from_str_loose("create"), WorkType::Creation);
        assert_eq!(WorkType::from_str_loose("building"), WorkType::Creation);
        assert_eq!(WorkType::from_str_loose("build"), WorkType::Creation);
        assert_eq!(WorkType::from_str_loose("debugging"), WorkType::Debugging);
        assert_eq!(WorkType::from_str_loose("debug"), WorkType::Debugging);
        assert_eq!(WorkType::from_str_loose("review"), WorkType::Review);
        assert_eq!(WorkType::from_str_loose("planning"), WorkType::Planning);
        assert_eq!(WorkType::from_str_loose("plan"), WorkType::Planning);
        assert_eq!(WorkType::from_str_loose("unknown_value"), WorkType::Other);
        assert_eq!(WorkType::from_str_loose(""), WorkType::Other);
    }

    #[test]
    fn work_type_from_str_loose_is_case_insensitive() {
        assert_eq!(WorkType::from_str_loose("RESEARCH"), WorkType::Research);
        assert_eq!(WorkType::from_str_loose("Debugging"), WorkType::Debugging);
        assert_eq!(WorkType::from_str_loose("ANALYSIS"), WorkType::Analysis);
        assert_eq!(WorkType::from_str_loose("BUILD"), WorkType::Creation);
    }

    #[test]
    fn work_type_as_tag_has_wt_prefix() {
        assert_eq!(WorkType::Research.as_tag(), SkillTag::new("wt:research"));
        assert_eq!(WorkType::Analysis.as_tag(), SkillTag::new("wt:analysis"));
        assert_eq!(WorkType::Creation.as_tag(), SkillTag::new("wt:creation"));
        assert_eq!(WorkType::Debugging.as_tag(), SkillTag::new("wt:debugging"));
        assert_eq!(WorkType::Review.as_tag(), SkillTag::new("wt:review"));
        assert_eq!(WorkType::Planning.as_tag(), SkillTag::new("wt:planning"));
        assert_eq!(WorkType::Other.as_tag(), SkillTag::new("wt:other"));
    }

    #[test]
    fn derived_summary_as_str_returns_inner() {
        let s = DerivedSummary::new("Rust, async, SQLite".into());
        assert_eq!(s.as_str(), "Rust, async, SQLite");
    }

    #[test]
    fn skill_tag_as_str_returns_inner() {
        let t = SkillTag::new("rust");
        assert_eq!(t.as_str(), "rust");
    }

    #[test]
    fn work_type_default_is_other() {
        assert_eq!(WorkType::default(), WorkType::Other);
    }

    #[test]
    fn work_type_display_all_variants() {
        assert_eq!(WorkType::Research.to_string(), "research");
        assert_eq!(WorkType::Analysis.to_string(), "analysis");
        assert_eq!(WorkType::Creation.to_string(), "creation");
        assert_eq!(WorkType::Debugging.to_string(), "debugging");
        assert_eq!(WorkType::Review.to_string(), "review");
        assert_eq!(WorkType::Planning.to_string(), "planning");
        assert_eq!(WorkType::Other.to_string(), "other");
    }
}
