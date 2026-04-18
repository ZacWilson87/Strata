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
