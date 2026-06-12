/// IPC bridge to the Tauri backend.
///
/// In development (non-Tauri), falls back to mock data so the UI can run
/// with `npm run dev` without a compiled Rust backend.
import type {
  AuditLogResponse,
  BackfillReport,
  GrowthResponse,
  InsightsResponse,
  IntegrationsResponse,
  PreferencesResponse,
  ScanReport,
  SkillHistoryResponse,
  SkillsResponse,
  TopicSummariesResponse,
} from "./types";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<any>;

function getTauriInvoke(): TauriInvoke | null {
  // @ts-expect-error — __TAURI__ is injected by the Tauri runtime
  if (typeof window !== "undefined" && window.__TAURI__) {
    // @ts-expect-error
    return window.__TAURI__.core.invoke;
  }
  return null;
}

const invoke = getTauriInvoke();

const MOCK_SKILLS: SkillsResponse = {
  summary: "rust, async, typescript (mock data)",
  skills: [
    { id: "1", tag: "rust", strength: 12, last_seen: new Date().toISOString(), session_count: 12 },
    { id: "2", tag: "async", strength: 8, last_seen: new Date().toISOString(), session_count: 8 },
    { id: "3", tag: "typescript", strength: 5, last_seen: new Date().toISOString(), session_count: 5 },
    { id: "4", tag: "sql", strength: 3, last_seen: new Date().toISOString(), session_count: 3 },
  ],
  work_types: { creation: 14, debugging: 9, analysis: 6, review: 3 },
  domains: [
    { tag: "rust", strength: 12, session_count: 12 },
    { tag: "mcp-protocol", strength: 8, session_count: 8 },
    { tag: "food_science", strength: 4, session_count: 4 },
    { tag: "fermentation", strength: 4, session_count: 4 },
    { tag: "sqlite", strength: 4, session_count: 4 },
    { tag: "tauri", strength: 3, session_count: 3 },
  ],
  tool_usage: { "claude-code": 18, "cursor": 6 },
};

const MOCK_AUDIT_LOG: AuditLogResponse = {
  entries: [
    { event: "skill_ingested", detail: "count=5", occurred_at: new Date().toISOString() },
    { event: "skill_queried", detail: null, occurred_at: new Date(Date.now() - 60_000).toISOString() },
    { event: "consent_granted", detail: null, occurred_at: new Date(Date.now() - 3_600_000).toISOString() },
  ],
};

const MOCK_SKILL_HISTORY: SkillHistoryResponse = {
  weeks: [
    { week: "2026-W14", top_tags: ["rust", "async", "sql"], total_sessions: 8 },
    { week: "2026-W15", top_tags: ["rust", "typescript", "react"], total_sessions: 12 },
    { week: "2026-W16", top_tags: ["rust", "tauri", "mcp"], total_sessions: 15 },
  ],
};

const MOCK_GROWTH: GrowthResponse = {
  skills: [
    {
      id: "1",
      tag: "rust",
      strength: 12,
      last_seen: new Date().toISOString(),
      session_count: 12,
      velocity: { tag: "rust", direction: "accelerating", delta: 0.42, recent_sessions: 6 },
      co_occurrences: [
        { tag: "async", co_occurrence: 7 },
        { tag: "sqlite", co_occurrence: 4 },
      ],
    },
    {
      id: "2",
      tag: "typescript",
      strength: 5,
      last_seen: new Date(Date.now() - 86_400_000).toISOString(),
      session_count: 5,
      velocity: { tag: "typescript", direction: "stable", delta: 0.03, recent_sessions: 3 },
      co_occurrences: [{ tag: "react", co_occurrence: 4 }],
    },
    {
      id: "3",
      tag: "sqlite",
      strength: 4,
      last_seen: new Date(Date.now() - 5 * 86_400_000).toISOString(),
      session_count: 4,
      velocity: { tag: "sqlite", direction: "declining", delta: -0.21, recent_sessions: 1 },
      co_occurrences: [{ tag: "rust", co_occurrence: 4 }],
    },
    {
      id: "4",
      tag: "react",
      strength: 2,
      last_seen: new Date(Date.now() - 2 * 86_400_000).toISOString(),
      session_count: 2,
      velocity: { tag: "react", direction: "new", delta: 1.0, recent_sessions: 2 },
      co_occurrences: [{ tag: "typescript", co_occurrence: 2 }],
    },
  ],
  recent_strengths: { rust: 9.4, typescript: 3.8, sqlite: 1.9, react: 2.0 },
};

const MOCK_TOPIC_SUMMARIES: TopicSummariesResponse = {
  summaries: [
    {
      timestamp_ms: Date.now() - 1 * 86_400_000,
      summary: "Debugged async deadlock in MCP server request routing",
      conversation_id: "conv-018",
    },
    {
      timestamp_ms: Date.now() - 2 * 86_400_000,
      summary: "Implemented skill velocity queries over weekly snapshots",
      conversation_id: "conv-017",
    },
    {
      timestamp_ms: Date.now() - 4 * 86_400_000,
      summary: "Reviewed React dashboard component test coverage",
      conversation_id: null,
    },
    {
      timestamp_ms: Date.now() - 6 * 86_400_000,
      summary: "Researched SQLite WAL checkpointing behavior under concurrent readers",
      conversation_id: "conv-014",
    },
    {
      timestamp_ms: Date.now() - 9 * 86_400_000,
      summary: "Planned consent audit log retention policy",
      conversation_id: "conv-011",
    },
    {
      timestamp_ms: Date.now() - 11 * 86_400_000,
      summary: "Created Tauri IPC bridge commands for the skill graph",
      conversation_id: "conv-009",
    },
    {
      timestamp_ms: Date.now() - 13 * 86_400_000,
      summary: "Analyzed keyword extraction accuracy for domain tagging",
      conversation_id: null,
    },
  ],
};

export async function getSkills(): Promise<SkillsResponse> {
  if (invoke) return invoke("get_skills");
  return MOCK_SKILLS;
}

export async function getContext(): Promise<{ context: string }> {
  if (invoke) return invoke("get_context");
  return { context: "Active in: rust, async (mock data)" };
}

export async function getPreferences(): Promise<PreferencesResponse> {
  if (invoke) return invoke("get_preferences");
  return {
    preferences: {
      commit_style: "never use emojis in commit messages",
      planning: "outline the approach before writing code",
    },
  };
}

export async function setUserPreference(key: string, value: string): Promise<void> {
  if (invoke) return invoke("set_user_preference", { key, value });
}

export async function deleteUserPreference(key: string): Promise<void> {
  if (invoke) return invoke("delete_user_preference", { key });
}

export async function getConsentStatus(): Promise<string> {
  if (invoke) return invoke("get_consent_status");
  return "granted";
}

export async function pauseConsent(): Promise<void> {
  if (invoke) return invoke("pause_consent");
}

export async function resumeConsent(): Promise<void> {
  if (invoke) return invoke("resume_consent");
}

export async function revokeConsent(): Promise<void> {
  if (invoke) return invoke("revoke_consent");
}

export async function getAuditLog(): Promise<AuditLogResponse> {
  if (invoke) return invoke("get_audit_log");
  return MOCK_AUDIT_LOG;
}

export async function getSkillHistory(): Promise<SkillHistoryResponse> {
  if (invoke) return invoke("get_skill_history");
  return MOCK_SKILL_HISTORY;
}

export async function getGrowth(): Promise<GrowthResponse> {
  if (invoke) return invoke("get_growth");
  return MOCK_GROWTH;
}

export async function getTopicSummaries(): Promise<TopicSummariesResponse> {
  if (invoke) return invoke("get_topic_summaries");
  return MOCK_TOPIC_SUMMARIES;
}

const MOCK_INSIGHTS: InsightsResponse = {
  insights: [
    {
      id: "repeated_context:rust",
      rule: "repeated_context",
      title: "Context is being re-explained",
      body: "Capture your project context once in a CLAUDE.md or project memory so each session starts warm instead of re-deriving it.",
      evidence: "5 sessions in the last 30 days flagged repeated context (mostly rust)",
      window_days: 30,
    },
    {
      id: "restarted_approach:all",
      rule: "restarted_approach",
      title: "Approaches getting restarted",
      body: "A short planning pass before building — plan mode, or a one-paragraph approach check — tends to catch dead ends before they cost a rebuild.",
      evidence: "3 sessions in the last 30 days abandoned an approach and started over",
      window_days: 30,
    },
  ],
};

export async function getInsights(): Promise<InsightsResponse> {
  if (invoke) return invoke("get_insights");
  return MOCK_INSIGHTS;
}

export async function dismissInsight(id: string): Promise<void> {
  if (invoke) return invoke("dismiss_insight", { id });
}

const MOCK_SCAN: ScanReport = {
  projects: 4,
  sessions_total: 38,
  sessions_new: 31,
  earliest_day: "2026-01-12",
  latest_day: "2026-06-10",
};

const MOCK_BACKFILL: BackfillReport = {
  sessions_ingested: 29,
  sessions_self_reported: 2,
  sessions_duplicate: 7,
  sessions_empty: 0,
  skills_touched: 14,
};

const MOCK_INTEGRATIONS: IntegrationsResponse = {
  integrations: [
    {
      id: "claude_desktop",
      name: "Claude Desktop",
      detected: true,
      installed: true,
      auto_installable: true,
      manual_command: null,
    },
    {
      id: "cursor",
      name: "Cursor",
      detected: true,
      installed: false,
      auto_installable: true,
      manual_command: null,
    },
    {
      id: "windsurf",
      name: "Windsurf",
      detected: true,
      installed: false,
      auto_installable: true,
      manual_command: null,
    },
    {
      id: "claude_code_hook",
      name: "Claude Code — session capture hook",
      detected: true,
      installed: false,
      auto_installable: true,
      manual_command: null,
    },
    {
      id: "claude_code_mcp",
      name: "Claude Code — MCP server",
      detected: true,
      installed: false,
      auto_installable: false,
      manual_command: 'claude mcp add --scope user strata -- "/usr/local/bin/strata"',
    },
  ],
};

export async function scanTranscripts(): Promise<ScanReport> {
  if (invoke) return invoke("scan_transcripts");
  return MOCK_SCAN;
}

export async function runBackfill(): Promise<BackfillReport> {
  if (invoke) return invoke("run_backfill");
  return MOCK_BACKFILL;
}

export async function getIntegrations(): Promise<IntegrationsResponse> {
  if (invoke) return invoke("get_integrations");
  return MOCK_INTEGRATIONS;
}

export async function installIntegration(id: string): Promise<IntegrationsResponse> {
  if (invoke) return invoke("install_integration", { id });
  return {
    integrations: MOCK_INTEGRATIONS.integrations.map((i) =>
      i.id === id ? { ...i, installed: true } : i,
    ),
  };
}
