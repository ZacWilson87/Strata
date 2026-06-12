export type Tab = "skills" | "consent" | "growth" | "setup";

export interface SkillNode {
  id: string;
  tag: string;
  /** Lifetime occurrence count — never decreases. */
  strength: number;
  /** Recency-weighted strength (30-day half-life). Decays when a skill goes unused. */
  recent_strength?: number;
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

export type VelocityDirection = "accelerating" | "stable" | "declining" | "new";

export interface SkillVelocity {
  tag: string;
  direction: VelocityDirection;
  delta: number;
  recent_sessions: number;
}

export interface CoOccurrenceSummary {
  tag: string;
  co_occurrence: number;
}

export interface SkillWithVelocity {
  id: string;
  tag: string;
  strength: number;
  last_seen: string;
  session_count: number;
  velocity: SkillVelocity;
  co_occurrences: CoOccurrenceSummary[];
}

export interface GrowthResponse {
  skills: SkillWithVelocity[];
  recent_strengths: Record<string, number>;
}

export interface TopicSummaryEntry {
  timestamp_ms: number;
  summary: string;
  conversation_id: string | null;
}

export interface TopicSummariesResponse {
  summaries: TopicSummaryEntry[];
}

/** A craft insight produced by the local rules engine (ADR 0005). */
export interface Insight {
  id: string;
  rule: string;
  title: string;
  body: string;
  evidence: string;
  window_days: number;
}

export interface InsightsResponse {
  insights: Insight[];
}

/** Aggregate session mechanics measured locally from transcripts (ADR 0008). */
export interface SessionMechanics {
  window_days: number;
  sessions: number;
  avg_prompts: number;
  median_duration_min: number;
  interrupted_sessions: number;
  tool_calls: number;
  tool_errors: number;
  avg_first_prompt_chars: number;
}

/** Result of scanning local AI-session transcripts (counts only, no content). */
export interface ScanReport {
  projects: number;
  sessions_total: number;
  sessions_new: number;
  earliest_day: string | null;
  latest_day: string | null;
}

/** Result of importing local transcripts through the privacy pipeline. */
export interface BackfillReport {
  sessions_ingested: number;
  sessions_self_reported: number;
  sessions_duplicate: number;
  sessions_empty: number;
  skills_touched: number;
}

/** Status of one AI-client integration target. */
export interface IntegrationStatus {
  id: string;
  name: string;
  detected: boolean;
  installed: boolean;
  auto_installable: boolean;
  manual_command: string | null;
}

export interface IntegrationsResponse {
  integrations: IntegrationStatus[];
}
