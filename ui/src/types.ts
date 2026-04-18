export type Tab = "skills" | "consent";

export interface SkillNode {
  id: string;
  tag: string;
  strength: number;
  last_seen: string;
  session_count: number;
}

export interface SkillsResponse {
  summary: string;
  skills: SkillNode[];
}

export interface PreferencesResponse {
  preferences: Record<string, string>;
}
