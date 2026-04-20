export type Tab = "skills" | "consent" | "growth";

export interface SkillNode {
  id: string;
  tag: string;
  strength: number;
  last_seen: string;
  session_count: number;
}

export interface DomainNode {
  tag: string;
  strength: number;
  session_count: number;
}

export interface SkillsResponse {
  summary: string;
  skills: SkillNode[];
  work_types: Record<string, number>;
  domains: DomainNode[];
  tool_usage: Record<string, number>;
}

export interface PreferencesResponse {
  preferences: Record<string, string>;
}

export interface AuditEntry {
  event: string;
  detail: string | null;
  occurred_at: string;
}

export interface AuditLogResponse {
  entries: AuditEntry[];
}

export interface WeeklySnapshot {
  week: string;
  top_tags: string[];
  total_sessions: number;
}

export interface SkillHistoryResponse {
  weeks: WeeklySnapshot[];
}
