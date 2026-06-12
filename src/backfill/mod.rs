//! Local transcript backfill — instant time-to-value (ADR 0006).
//!
//! Claude Code already stores full session transcripts on this machine
//! (`~/.claude/projects/<project>/<session-id>.jsonl`). With consent, this
//! module parses that existing history *locally*, runs the user-prompt text
//! through the same in-memory privacy pipeline as live ingest
//! (`signals::process_ingest`), and writes only derived skill tags — attributed
//! to the day each session actually happened.
//!
//! The same parser powers two entry points:
//! - **Bulk backfill** from the dashboard onboarding flow (`scan` + `run`)
//! - **Session-end hook** (`strata hook session-end`) for deterministic
//!   capture of every future session (`ingest_hook_event`)
//!
//! Privacy properties:
//! - Raw transcript text is held in a non-serializable [`RawSignal`] and
//!   consumed in-memory; it is never persisted, logged, or returned.
//! - Only sessions never seen before are ingested (`ingested_sessions` table),
//!   and sessions that already self-reported through the `strata_ingest` MCP
//!   tool are skipped entirely — no double counting between the taxonomizer
//!   path and the transcript path.
//! - Every entry point checks the consent gate first.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::consent::{AuditEvent, ConsentError, ConsentGate};
use crate::graph::{GraphError, GraphHandle};
use crate::private_mode::RawSignal;
use crate::signals::{process_ingest, IngestPayload};

/// Errors from backfill operations.
#[derive(Debug, thiserror::Error)]
pub enum BackfillError {
    #[error("consent error: {0}")]
    Consent(#[from] ConsentError),

    #[error("graph error: {0}")]
    Graph(#[from] GraphError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid hook payload: {0}")]
    BadHookPayload(String),
}

/// Cap on collected prompt text per session. Matches the live-ingest content
/// cap; `process_ingest` re-truncates defensively.
const MAX_SESSION_CONTENT_BYTES: usize = 256 * 1024;

/// Tool attribution for transcript-derived sessions.
const TRANSCRIPT_TOOL: &str = "claude-code";

/// A session parsed from a transcript file. Raw prompt text lives in a
/// non-serializable `RawSignal` and never leaves this module.
pub struct ParsedSession {
    /// Session id — the transcript file stem (Claude Code names each
    /// transcript `<session-id>.jsonl`). Used as the dedupe key everywhere.
    pub session_id: String,
    /// UTC day (`YYYY-MM-DD`) of the session's last activity.
    pub day: String,
    /// RFC3339 timestamp of the session's last activity.
    pub last_seen: String,
    /// Number of genuine user prompts found.
    pub prompt_count: usize,
    /// Whether the session already reported itself via the `strata_ingest`
    /// MCP tool — if so the transcript path must not count it again.
    pub self_reported: bool,
    /// Objective session mechanics (ADR 0008). Pure numbers, no content.
    pub mechanics: SessionMechanics,
    /// Concatenated user prompt text (capped). Consumed by `ingest_session`.
    content: RawSignal,
}

/// Objective mechanics computed during the transcript scan. All fields are
/// counts and durations — no content of any kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionMechanics {
    /// Assistant turns (messages with content).
    pub assistant_turns: i64,
    /// Active wall-clock minutes: gaps over 30 min between consecutive
    /// entries (session resumed later) are excluded.
    pub active_minutes: f64,
    /// User interruptions of in-flight work ("[Request interrupted…").
    pub interruptions: i64,
    /// Tool invocations the assistant made.
    pub tool_calls: i64,
    /// Tool results that returned an error.
    pub tool_errors: i64,
    /// Character length of the first genuine user prompt.
    pub first_prompt_chars: i64,
    /// Total characters across genuine user prompts (for the average).
    pub total_prompt_chars: i64,
}

/// Outcome of ingesting (or skipping) one session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    /// Derived tags were written to the graph.
    Ingested,
    /// Already in `ingested_sessions` — nothing written.
    Duplicate,
    /// Session self-reported via `strata_ingest` — marked seen, nothing written.
    SelfReported,
    /// No usable prompts or timestamps — marked seen, nothing written.
    Empty,
}

/// Result of scanning the transcript directory (no content is read).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanReport {
    /// Number of project directories containing transcripts.
    pub projects: usize,
    /// Total transcript sessions found on disk.
    pub sessions_total: usize,
    /// Sessions not yet ingested.
    pub sessions_new: usize,
    /// Earliest session day (`YYYY-MM-DD`, from file mtime), if any.
    pub earliest_day: Option<String>,
    /// Latest session day (`YYYY-MM-DD`, from file mtime), if any.
    pub latest_day: Option<String>,
}

/// Result of a backfill run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillReport {
    /// Sessions whose derived tags were written to the graph.
    pub sessions_ingested: usize,
    /// Sessions skipped because they already self-reported via `strata_ingest`.
    pub sessions_self_reported: usize,
    /// Sessions skipped as already ingested.
    pub sessions_duplicate: usize,
    /// Sessions with no usable prompts.
    pub sessions_empty: usize,
    /// Distinct skill tags touched across the run.
    pub skills_touched: usize,
}

/// Default transcript root: `~/.claude/projects`.
pub fn default_transcripts_root() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("projects"))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// List transcript files as `(session_id, path)` pairs, one directory level
/// deep (`<root>/<project>/<session>.jsonl`).
fn list_transcripts(root: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    for project in std::fs::read_dir(root)? {
        let project = project?;
        if !project.file_type()?.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(project.path())? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file() || path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            if let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(sanitize_session_id)
            {
                out.push((id, path));
            }
        }
    }
    Ok(out)
}

/// Keep `[A-Za-z0-9_-]`, cap at 64 chars — same charset as conversation ids.
fn sanitize_session_id(raw: &str) -> Option<String> {
    let id: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(64)
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Scan the transcript directory without reading any transcript content.
/// Date range comes from file modification times.
pub fn scan(root: &Path, graph: &GraphHandle) -> Result<ScanReport, BackfillError> {
    let transcripts = list_transcripts(root)?;
    let mut projects: HashSet<PathBuf> = HashSet::new();
    let mut sessions_new = 0;
    let mut earliest: Option<DateTime<Utc>> = None;
    let mut latest: Option<DateTime<Utc>> = None;

    for (session_id, path) in &transcripts {
        if let Some(parent) = path.parent() {
            projects.insert(parent.to_path_buf());
        }
        if !graph.is_session_ingested(session_id)? {
            sessions_new += 1;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                let mtime: DateTime<Utc> = mtime.into();
                if earliest.is_none_or(|e| mtime < e) {
                    earliest = Some(mtime);
                }
                if latest.is_none_or(|l| mtime > l) {
                    latest = Some(mtime);
                }
            }
        }
    }

    Ok(ScanReport {
        projects: projects.len(),
        sessions_total: transcripts.len(),
        sessions_new,
        earliest_day: earliest.map(|t| t.date_naive().to_string()),
        latest_day: latest.map(|t| t.date_naive().to_string()),
    })
}

/// Parse one transcript file, collecting genuine user prompts in-memory.
///
/// Returns `None` when the file contains no usable prompts or no timestamps.
/// Entry filtering: only `type == "user"` lines that are real typed prompts —
/// not tool results (`toolUseResult`), not subagent sidechains, not meta
/// entries, and not harness-injected content (skipped via the leading-`<`
/// heuristic for command/system wrappers).
pub fn parse_transcript(path: &Path) -> std::io::Result<Option<ParsedSession>> {
    let reader = BufReader::new(File::open(path)?);

    /// Gaps longer than this between consecutive entries are treated as the
    /// session being parked (laptop closed, resumed next day) and excluded
    /// from active time.
    const ACTIVE_GAP_CAP_SECS: i64 = 30 * 60;

    let mut content = String::new();
    let mut prompt_count = 0usize;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut prev_ts: Option<DateTime<Utc>> = None;
    let mut active_secs: i64 = 0;
    let mut self_reported = false;
    let mut mech = SessionMechanics::default();

    for line in reader.lines() {
        // Tolerate isolated bad lines (truncated writes, encoding issues).
        let Ok(line) = line else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if let Some(ts) = v
            .get("timestamp")
            .and_then(|s| s.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        {
            let ts = ts.with_timezone(&Utc);
            if let Some(prev) = prev_ts {
                let gap = (ts - prev).num_seconds();
                if (0..=ACTIVE_GAP_CAP_SECS).contains(&gap) {
                    active_secs += gap;
                }
            }
            prev_ts = Some(ts);
            if last_ts.is_none_or(|prev| ts > prev) {
                last_ts = Some(ts);
            }
        }

        match v.get("type").and_then(|t| t.as_str()) {
            Some("user") => {
                let flag = |key: &str| v.get(key).and_then(|b| b.as_bool()).unwrap_or(false);
                if flag("isSidechain") {
                    continue;
                }
                // Tool results ride on user-type entries; count errors there.
                if v.get("toolUseResult").is_some() {
                    if let Some(blocks) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                        mech.tool_errors += blocks
                            .iter()
                            .filter(|b| {
                                b.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false)
                            })
                            .count() as i64;
                    }
                    continue;
                }
                if flag("isMeta") {
                    continue;
                }
                // Interruption markers arrive as text blocks in an array.
                if let Some(blocks) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                    if blocks.iter().any(|b| {
                        b.get("text")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t.trim_start().starts_with("[Request interrupted"))
                    }) {
                        mech.interruptions += 1;
                    }
                    continue;
                }
                let Some(text) = v.pointer("/message/content").and_then(|c| c.as_str()) else {
                    continue;
                };
                let text = text.trim();
                if text.starts_with("[Request interrupted") {
                    mech.interruptions += 1;
                    continue;
                }
                if text.is_empty() || text.starts_with('<') {
                    continue;
                }
                let chars = text.chars().count() as i64;
                if prompt_count == 0 {
                    mech.first_prompt_chars = chars;
                }
                mech.total_prompt_chars += chars;
                prompt_count += 1;
                if content.len() < MAX_SESSION_CONTENT_BYTES {
                    content.push_str(text);
                    content.push('\n');
                }
            }
            Some("assistant") => {
                if v.get("isSidechain")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
                {
                    continue;
                }
                if let Some(blocks) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                    mech.assistant_turns += 1;
                    for b in blocks {
                        if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            mech.tool_calls += 1;
                            if !self_reported
                                && b.get("name")
                                    .and_then(|n| n.as_str())
                                    .is_some_and(|n| n.ends_with("strata_ingest"))
                            {
                                self_reported = true;
                            }
                        }
                    }
                } else if v.pointer("/message/content").is_some() {
                    mech.assistant_turns += 1;
                }
            }
            _ => {}
        }
    }
    mech.active_minutes = active_secs as f64 / 60.0;

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(sanitize_session_id);
    let (Some(session_id), Some(last_ts)) = (session_id, last_ts) else {
        return Ok(None);
    };
    if prompt_count == 0 {
        return Ok(None);
    }

    Ok(Some(ParsedSession {
        session_id,
        day: last_ts.date_naive().to_string(),
        last_seen: last_ts.to_rfc3339(),
        prompt_count,
        self_reported,
        mechanics: mech,
        content: RawSignal::new(content),
    }))
}

/// Run one parsed session through the live privacy pipeline and write the
/// derived tags, attributed to the session's real day. Returns the tags written.
fn ingest_session(graph: &GraphHandle, session: ParsedSession) -> Result<Vec<String>, GraphError> {
    // Raw content is consumed by process_ingest — same boundary as live ingest.
    let signal = process_ingest(IngestPayload {
        tool_used: TRANSCRIPT_TOOL.into(),
        content: session.content.0,
        domain_hint: None,
        work_type: None,
        domain_tags: None,
        topic_summary: None,
        conversation_id: Some(session.session_id.clone()),
        friction_signals: None,
        features_used: None,
        outcome: None,
    });

    for tag in &signal.skill_tags {
        graph.upsert_skill_on_day(tag, &session.day, &session.last_seen)?;
    }
    let tags = &signal.skill_tags;
    for i in 0..tags.len() {
        for j in (i + 1)..tags.len() {
            if tags[i] != tags[j] {
                graph.record_co_occurrence(&tags[i], &tags[j])?;
            }
        }
    }
    graph.mark_session_ingested(&session.session_id, &session.day)?;

    Ok(signal
        .skill_tags
        .into_iter()
        .map(|t| t.as_str().to_string())
        .collect())
}

/// Build the metrics row for a parsed session. Work type comes from the same
/// structural detector the keyword fallback uses (it borrows, not consumes).
fn metrics_row(session: &ParsedSession) -> crate::graph::SessionMetricsRow {
    let work_type = match crate::signals::detect_work_type(&session.content) {
        crate::private_mode::WorkType::Other => None,
        wt => wt.as_tag().as_str().strip_prefix("wt:").map(str::to_string),
    };
    let m = &session.mechanics;
    crate::graph::SessionMetricsRow {
        session_id: session.session_id.clone(),
        day: session.day.clone(),
        tool: TRANSCRIPT_TOOL.into(),
        work_type,
        prompts: session.prompt_count as i64,
        assistant_turns: m.assistant_turns,
        duration_min: (m.active_minutes * 10.0).round() / 10.0,
        interruptions: m.interruptions,
        tool_calls: m.tool_calls,
        tool_errors: m.tool_errors,
        first_prompt_chars: m.first_prompt_chars,
        avg_prompt_chars: if session.prompt_count > 0 {
            m.total_prompt_chars / session.prompt_count as i64
        } else {
            0
        },
    }
}

/// Ingest a single transcript file end-to-end: parse, dedupe, write, mark.
///
/// Tags and mechanics dedupe independently: mechanics are objective and apply
/// to every session (including self-reported ones, and sessions whose tags
/// were ingested before the metrics layer existed — they get backfilled on
/// the next run).
fn ingest_transcript_file(
    graph: &GraphHandle,
    session_id: &str,
    path: &Path,
    skills_touched: &mut HashSet<String>,
) -> Result<SessionOutcome, BackfillError> {
    let tags_done = graph.is_session_ingested(session_id)?;
    let metrics_done = graph.has_session_metrics(session_id)?;
    if tags_done && metrics_done {
        return Ok(SessionOutcome::Duplicate);
    }
    let today = Utc::now().date_naive().to_string();
    match parse_transcript(path)? {
        None => {
            // Mark so future scans don't reparse a permanently empty file.
            graph.mark_session_ingested(session_id, &today)?;
            Ok(SessionOutcome::Empty)
        }
        Some(session) => {
            if !metrics_done {
                graph.record_session_metrics(&metrics_row(&session))?;
            }
            if tags_done {
                return Ok(SessionOutcome::Duplicate);
            }
            if session.self_reported {
                // The session already described itself (better taxonomy) via
                // the strata_ingest MCP tool — never double-count its tags.
                graph.mark_session_ingested(&session.session_id, &session.day)?;
                return Ok(SessionOutcome::SelfReported);
            }
            for tag in ingest_session(graph, session)? {
                skills_touched.insert(tag);
            }
            Ok(SessionOutcome::Ingested)
        }
    }
}

/// Run a consent-gated bulk backfill over every transcript under `root`.
pub fn run(
    root: &Path,
    graph: &GraphHandle,
    consent: &ConsentGate,
) -> Result<BackfillReport, BackfillError> {
    consent.check()?;

    let mut report = BackfillReport {
        sessions_ingested: 0,
        sessions_self_reported: 0,
        sessions_duplicate: 0,
        sessions_empty: 0,
        skills_touched: 0,
    };
    let mut skills_touched: HashSet<String> = HashSet::new();

    for (session_id, path) in list_transcripts(root)? {
        match ingest_transcript_file(graph, &session_id, &path, &mut skills_touched) {
            Ok(SessionOutcome::Ingested) => report.sessions_ingested += 1,
            Ok(SessionOutcome::SelfReported) => report.sessions_self_reported += 1,
            Ok(SessionOutcome::Duplicate) => report.sessions_duplicate += 1,
            Ok(SessionOutcome::Empty) => report.sessions_empty += 1,
            // One unreadable transcript must not abort the whole import.
            Err(BackfillError::Io(e)) => {
                tracing::warn!("skipping unreadable transcript: {e}");
                report.sessions_empty += 1;
            }
            Err(e) => return Err(e),
        }
    }
    report.skills_touched = skills_touched.len();

    consent.record(AuditEvent::BackfillRun {
        sessions: report.sessions_ingested,
    })?;
    Ok(report)
}

/// Handle a Claude Code `SessionEnd` hook event (JSON on stdin).
///
/// Expects at least `{"transcript_path": "..."}`. The path must be a `.jsonl`
/// file inside the user's `~/.claude` directory — the hook refuses to read
/// anything else.
pub fn ingest_hook_event(
    graph: &GraphHandle,
    consent: &ConsentGate,
    raw_event: &str,
) -> Result<SessionOutcome, BackfillError> {
    consent.check()?;

    let event: serde_json::Value = serde_json::from_str(raw_event)
        .map_err(|e| BackfillError::BadHookPayload(format!("not valid JSON: {e}")))?;
    let transcript_path = event
        .get("transcript_path")
        .and_then(|p| p.as_str())
        .ok_or_else(|| BackfillError::BadHookPayload("missing transcript_path".into()))?;

    let path = Path::new(transcript_path)
        .canonicalize()
        .map_err(|e| BackfillError::BadHookPayload(format!("transcript_path: {e}")))?;
    if path.extension().is_none_or(|e| e != "jsonl") {
        return Err(BackfillError::BadHookPayload(
            "transcript_path must be a .jsonl file".into(),
        ));
    }
    let claude_dir = home_dir()
        .map(|h| h.join(".claude"))
        .and_then(|d| d.canonicalize().ok());
    if claude_dir.is_none_or(|d| !path.starts_with(&d)) {
        return Err(BackfillError::BadHookPayload(
            "transcript_path must be inside ~/.claude".into(),
        ));
    }

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(sanitize_session_id)
        .ok_or_else(|| BackfillError::BadHookPayload("unusable session id".into()))?;

    let mut touched = HashSet::new();
    let outcome = ingest_transcript_file(graph, &session_id, &path, &mut touched)?;
    if outcome == SessionOutcome::Ingested {
        consent.record(AuditEvent::SkillIngested {
            count: touched.len(),
            tool: TRANSCRIPT_TOOL.into(),
        })?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_transcript(dir: &Path, name: &str, lines: &[serde_json::Value]) -> PathBuf {
        let path = dir.join(format!("{name}.jsonl"));
        let mut f = File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    fn user_line(text: &str, ts: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": text },
            "timestamp": ts,
            "sessionId": "11111111-2222-3333-4444-555555555555",
            "isSidechain": false
        })
    }

    fn setup() -> (tempfile::TempDir, GraphHandle, ConsentGate) {
        let dir = tempfile::tempdir().unwrap();
        let graph = GraphHandle::open_in_memory().unwrap();
        let consent = ConsentGate::open_in_memory().unwrap();
        (dir, graph, consent)
    }

    #[test]
    fn parse_extracts_prompts_and_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_transcript(
            dir.path(),
            "abc-123",
            &[
                user_line(
                    "help me write a rust async function",
                    "2026-04-18T15:06:21.287Z",
                ),
                user_line("now add sql persistence", "2026-04-18T16:10:00.000Z"),
            ],
        );
        let session = parse_transcript(&path).unwrap().unwrap();
        assert_eq!(session.session_id, "abc-123");
        assert_eq!(session.day, "2026-04-18");
        assert_eq!(session.prompt_count, 2);
        assert!(!session.self_reported);
        assert!(session.content.0.contains("rust async"));
    }

    #[test]
    fn parse_skips_tool_results_meta_sidechains_and_wrappers() {
        let dir = tempfile::tempdir().unwrap();
        let ts = "2026-04-18T15:06:21Z";
        let path = write_transcript(
            dir.path(),
            "abc-123",
            &[
                user_line("real prompt about docker", ts),
                serde_json::json!({"type":"user","message":{"content":"tool output"},"toolUseResult":{},"timestamp":ts}),
                serde_json::json!({"type":"user","message":{"content":"sidechain text"},"isSidechain":true,"timestamp":ts}),
                serde_json::json!({"type":"user","message":{"content":"meta"},"isMeta":true,"timestamp":ts}),
                serde_json::json!({"type":"user","message":{"content":"<command-name>/compact</command-name>"},"timestamp":ts}),
                serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]},"timestamp":ts}),
            ],
        );
        let session = parse_transcript(&path).unwrap().unwrap();
        assert_eq!(session.prompt_count, 1);
        assert!(session.content.0.contains("docker"));
        assert!(!session.content.0.contains("sidechain"));
        assert!(!session.content.0.contains("tool output"));
    }

    #[test]
    fn parse_detects_self_reported_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let ts = "2026-04-18T15:06:21Z";
        let path = write_transcript(
            dir.path(),
            "abc-123",
            &[
                user_line("work on the rust server", ts),
                serde_json::json!({"type":"assistant","message":{"content":[
                    {"type":"tool_use","name":"mcp__strata__strata_ingest","input":{}}
                ]},"timestamp":ts}),
            ],
        );
        let session = parse_transcript(&path).unwrap().unwrap();
        assert!(session.self_reported);
    }

    #[test]
    fn parse_empty_transcript_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let ts = "2026-04-18T15:06:21Z";
        let path = write_transcript(
            dir.path(),
            "abc-123",
            &[serde_json::json!({"type":"system","timestamp":ts})],
        );
        assert!(parse_transcript(&path).unwrap().is_none());
    }

    #[test]
    fn parse_tolerates_garbage_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.jsonl");
        let mut f = File::create(&path).unwrap();
        writeln!(f, "not json at all").unwrap();
        writeln!(
            f,
            "{}",
            user_line("debug a python crash", "2026-04-18T15:06:21Z")
        )
        .unwrap();
        let session = parse_transcript(&path).unwrap().unwrap();
        assert_eq!(session.prompt_count, 1);
    }

    #[test]
    fn run_backfills_day_attributed_skills_and_dedupes() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("-Users-test-proj");
        std::fs::create_dir(&project).unwrap();
        write_transcript(
            &project,
            "aaaaaaaa-0000-0000-0000-000000000001",
            &[user_line(
                "write a rust sql migration",
                "2026-03-01T10:00:00Z",
            )],
        );

        let report = run(dir.path(), &graph, &consent).unwrap();
        assert_eq!(report.sessions_ingested, 1);
        assert!(report.skills_touched >= 2); // rust, sql, tool:claude-code

        // Events landed on the session's real day, not today.
        let skills = graph.get_top_skills(20).unwrap();
        assert!(skills.iter().any(|s| s.tag == "rust"));
        assert!(skills.iter().any(|s| s.tag == "sql"));

        // Second run: everything is a duplicate, nothing re-ingested.
        let report2 = run(dir.path(), &graph, &consent).unwrap();
        assert_eq!(report2.sessions_ingested, 0);
        assert_eq!(report2.sessions_duplicate, 1);
        let rust = graph
            .get_top_skills(20)
            .unwrap()
            .into_iter()
            .find(|s| s.tag == "rust")
            .unwrap();
        assert_eq!(rust.session_count, 1);
    }

    #[test]
    fn run_skips_self_reported_sessions() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        let ts = "2026-03-01T10:00:00Z";
        write_transcript(
            &project,
            "bbbbbbbb-0000-0000-0000-000000000001",
            &[
                user_line("rust work", ts),
                serde_json::json!({"type":"assistant","message":{"content":[
                    {"type":"tool_use","name":"mcp__strata__strata_ingest","input":{}}
                ]},"timestamp":ts}),
            ],
        );
        let report = run(dir.path(), &graph, &consent).unwrap();
        assert_eq!(report.sessions_ingested, 0);
        assert_eq!(report.sessions_self_reported, 1);
        assert!(graph.get_top_skills(10).unwrap().is_empty());
    }

    #[test]
    fn run_blocked_when_consent_paused() {
        let (dir, graph, consent) = setup();
        consent.pause().unwrap();
        assert!(matches!(
            run(dir.path(), &graph, &consent),
            Err(BackfillError::Consent(ConsentError::Paused))
        ));
    }

    #[test]
    fn scan_counts_without_reading_content() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        write_transcript(
            &project,
            "cccccccc-0000-0000-0000-000000000001",
            &[user_line("rust", "2026-03-01T10:00:00Z")],
        );
        write_transcript(
            &project,
            "cccccccc-0000-0000-0000-000000000002",
            &[user_line("sql", "2026-03-02T10:00:00Z")],
        );

        let report = scan(dir.path(), &graph).unwrap();
        assert_eq!(report.projects, 1);
        assert_eq!(report.sessions_total, 2);
        assert_eq!(report.sessions_new, 2);
        assert!(report.earliest_day.is_some());

        run(dir.path(), &graph, &consent).unwrap();
        let report = scan(dir.path(), &graph).unwrap();
        assert_eq!(report.sessions_new, 0);
    }

    #[test]
    fn scan_missing_root_returns_empty() {
        let graph = GraphHandle::open_in_memory().unwrap();
        let report = scan(Path::new("/nonexistent/strata-test"), &graph).unwrap();
        assert_eq!(report.sessions_total, 0);
        assert_eq!(report.projects, 0);
    }

    #[test]
    fn hook_event_rejects_paths_outside_claude_dir() {
        let (dir, graph, consent) = setup();
        let path = write_transcript(
            dir.path(),
            "dddddddd-0000-0000-0000-000000000001",
            &[user_line("rust", "2026-03-01T10:00:00Z")],
        );
        let event = serde_json::json!({ "transcript_path": path }).to_string();
        assert!(matches!(
            ingest_hook_event(&graph, &consent, &event),
            Err(BackfillError::BadHookPayload(_))
        ));
    }

    #[test]
    fn hook_event_rejects_malformed_payloads() {
        let (_dir, graph, consent) = setup();
        for raw in ["not json", "{}", r#"{"transcript_path": 42}"#] {
            assert!(matches!(
                ingest_hook_event(&graph, &consent, raw),
                Err(BackfillError::BadHookPayload(_))
            ));
        }
    }

    #[test]
    fn backfilled_last_seen_never_rewinds_live_data() {
        let (dir, graph, consent) = setup();
        // Live ingest happens first (today).
        graph
            .upsert_skill(&crate::private_mode::SkillTag::new("rust"))
            .unwrap();
        let live_last_seen = graph.get_top_skills(1).unwrap()[0].last_seen;

        // Backfill an old session mentioning rust.
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        write_transcript(
            &project,
            "eeeeeeee-0000-0000-0000-000000000001",
            &[user_line("old rust work", "2020-01-01T10:00:00Z")],
        );
        run(dir.path(), &graph, &consent).unwrap();

        let rust = graph
            .get_top_skills(10)
            .unwrap()
            .into_iter()
            .find(|s| s.tag == "rust")
            .unwrap();
        assert_eq!(rust.session_count, 2);
        assert!(
            rust.last_seen >= live_last_seen,
            "last_seen must not rewind"
        );
    }

    // ── session mechanics (ADR 0008) ──────────────────────────────────────

    #[test]
    fn parse_computes_session_mechanics() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_transcript(
            dir.path(),
            "mech-1",
            &[
                user_line("fix this rust bug please", "2026-04-18T15:00:00Z"),
                serde_json::json!({"type":"assistant","message":{"content":[
                    {"type":"text","text":"looking"},
                    {"type":"tool_use","name":"Bash","input":{}}
                ]},"timestamp":"2026-04-18T15:01:00Z"}),
                serde_json::json!({"type":"user","message":{"content":[
                    {"type":"tool_result","is_error":true,"content":"boom"}
                ]},"toolUseResult":{},"timestamp":"2026-04-18T15:02:00Z"}),
                user_line("[Request interrupted by user]", "2026-04-18T15:03:00Z"),
                user_line("try the other file", "2026-04-18T15:04:00Z"),
                serde_json::json!({"type":"assistant","message":{"content":[
                    {"type":"text","text":"done"}
                ]},"timestamp":"2026-04-18T16:30:00Z"}),
            ],
        );
        let session = parse_transcript(&path).unwrap().unwrap();
        let m = &session.mechanics;
        assert_eq!(session.prompt_count, 2);
        assert_eq!(m.assistant_turns, 2);
        assert_eq!(m.tool_calls, 1);
        assert_eq!(m.tool_errors, 1);
        assert_eq!(m.interruptions, 1);
        assert_eq!(
            m.first_prompt_chars,
            "fix this rust bug please".len() as i64
        );
        // The 86-minute parked gap before the last entry is excluded: only the
        // four 1-minute gaps count as active time.
        assert!(
            (m.active_minutes - 4.0).abs() < 0.01,
            "{}",
            m.active_minutes
        );
    }

    #[test]
    fn run_records_metrics_for_all_sessions_including_self_reported() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        let ts = "2026-03-01T10:00:00Z";
        write_transcript(
            &project,
            "plain-0000-0000-0000-000000000001",
            &[user_line("debug this rust error", ts)],
        );
        write_transcript(
            &project,
            "selfrep-0000-0000-0000-000000000002",
            &[
                user_line("rust work", ts),
                serde_json::json!({"type":"assistant","message":{"content":[
                    {"type":"tool_use","name":"mcp__strata__strata_ingest","input":{}}
                ]},"timestamp":ts}),
            ],
        );

        run(dir.path(), &graph, &consent).unwrap();
        let metrics = graph.get_session_metrics_since(36500).unwrap();
        assert_eq!(
            metrics.len(),
            2,
            "self-reported sessions still get mechanics"
        );
        let plain = metrics
            .iter()
            .find(|m| m.session_id.starts_with("plain"))
            .unwrap();
        assert_eq!(plain.work_type.as_deref(), Some("debugging"));
        assert_eq!(plain.day, "2026-03-01");
    }

    #[test]
    fn metrics_backfill_retroactively_for_previously_ingested_sessions() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        write_transcript(
            &project,
            "old-0000-0000-0000-000000000001",
            &[user_line("rust work", "2026-03-01T10:00:00Z")],
        );

        // Session was tag-ingested before the metrics layer existed.
        graph
            .mark_session_ingested("old-0000-0000-0000-000000000001", "2026-03-01")
            .unwrap();
        assert!(graph.get_session_metrics_since(36500).unwrap().is_empty());

        let report = run(dir.path(), &graph, &consent).unwrap();
        assert_eq!(report.sessions_duplicate, 1, "tags stay deduped");
        let metrics = graph.get_session_metrics_since(36500).unwrap();
        assert_eq!(metrics.len(), 1, "mechanics get backfilled anyway");

        // And a second run is a clean no-op.
        run(dir.path(), &graph, &consent).unwrap();
        assert_eq!(graph.get_session_metrics_since(36500).unwrap().len(), 1);
    }

    #[test]
    fn revocation_wipes_session_metrics() {
        let (dir, graph, consent) = setup();
        let project = dir.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        write_transcript(
            &project,
            "wipe-0000-0000-0000-000000000001",
            &[user_line("rust work", "2026-03-01T10:00:00Z")],
        );
        run(dir.path(), &graph, &consent).unwrap();
        assert!(!graph.get_session_metrics_since(36500).unwrap().is_empty());
        graph.delete_all_data().unwrap();
        assert!(graph.get_session_metrics_since(36500).unwrap().is_empty());
    }
}
