/// IPC bridge to the Tauri backend.
///
/// In development (non-Tauri), falls back to mock data so the UI can run
/// with `npm run dev` without a compiled Rust backend.
import type { SkillsResponse, PreferencesResponse } from "./types";

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
