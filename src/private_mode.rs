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
}
