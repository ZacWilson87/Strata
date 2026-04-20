/// IPC bridge to the Tauri backend.
///
/// In development (non-Tauri), falls back to mock data so the UI can run
/// with `npm run dev` without a compiled Rust backend.
import type {
  AuditLogResponse,
  PreferencesResponse,
  SkillHistoryResponse,
  SkillsResponse,
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
  return { preferences: {} };
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
