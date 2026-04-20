import { useEffect, useRef, useState } from "react";
import { getSkills } from "../ipc";
import type { DomainNode, SkillNode, SkillsResponse } from "../types";

const POLL_INTERVAL_MS = 10_000;

const WORK_TYPE_META: Record<string, { label: string; color: string; emoji: string }> = {
  creation:  { label: "Creation",  color: "#2563eb", emoji: "✦" },
  debugging: { label: "Debugging", color: "#dc2626", emoji: "⬡" },
  analysis:  { label: "Analysis",  color: "#7c3aed", emoji: "◈" },
  research:  { label: "Research",  color: "#0891b2", emoji: "◎" },
  review:    { label: "Review",    color: "#d97706", emoji: "◇" },
  planning:  { label: "Planning",  color: "#059669", emoji: "△" },
};

const FALLBACK_META = { label: "Other", color: "#6b7280", emoji: "○" };

export default function SkillMap() {
  const [data, setData] = useState<SkillsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = async (showSpinner = false) => {
    if (showSpinner) setRefreshing(true);
    try {
      const next = await getSkills();
      setData(next);
      setLastUpdated(new Date());
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      if (showSpinner) setRefreshing(false);
    }
  };

  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(() => refresh(), POLL_INTERVAL_MS);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  if (error) return <p style={{ color: "#ef4444" }}>Error: {error}</p>;
  if (!data) return <p style={{ color: "#9ca3af" }}>Loading…</p>;

  const totalWorkSessions = Object.values(data.work_types ?? {}).reduce((a, b) => a + b, 0);
  const sortedWorkTypes = Object.entries(data.work_types ?? {}).sort((a, b) => b[1] - a[1]);
  const maxDomain = data.domains?.[0]?.strength ?? 1;

  const updatedLabel = lastUpdated
    ? lastUpdated.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })
    : null;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>

      {/* ── Live status bar ── */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div style={{
            width: 7,
            height: 7,
            borderRadius: "50%",
            background: "#22c55e",
            boxShadow: "0 0 6px #22c55e",
            animation: "pulse 2s ease-in-out infinite",
          }} />
          <span style={{ fontSize: 12, color: "#6b7280" }}>
            Live · updates every 10s
            {updatedLabel && <> · last at {updatedLabel}</>}
          </span>
        </div>
        <button
          onClick={() => refresh(true)}
          disabled={refreshing}
          style={{
            background: "transparent",
            border: "1px solid #27272a",
            borderRadius: 6,
            color: refreshing ? "#4b5563" : "#9ca3af",
            cursor: refreshing ? "not-allowed" : "pointer",
            fontSize: 12,
            padding: "4px 10px",
          }}
        >
          {refreshing ? "Refreshing…" : "Refresh now"}
        </button>
      </div>

      {/* ── Work Type Breakdown ── */}
      {sortedWorkTypes.length > 0 && (
        <Section title="Work Type Breakdown" subtitle="How you've been spending your AI sessions">
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            {sortedWorkTypes.map(([type, strength]) => {
              const meta = WORK_TYPE_META[type] ?? FALLBACK_META;
              const pct = totalWorkSessions > 0
                ? Math.round((strength / totalWorkSessions) * 100)
                : 0;
              return (
                <div key={type} style={{ display: "flex", alignItems: "center", gap: 14 }}>
                  <div style={{
                    width: 28,
                    height: 28,
                    borderRadius: 8,
                    background: meta.color + "22",
                    border: `1px solid ${meta.color}44`,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 13,
                    color: meta.color,
                    flexShrink: 0,
                  }}>
                    {meta.emoji}
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 5 }}>
                      <span style={{ fontSize: 13, fontWeight: 500, color: "#e5e7eb" }}>
                        {meta.label}
                      </span>
                      <span style={{ fontSize: 12, color: "#6b7280" }}>
                        {pct}% · {strength} sessions
                      </span>
                    </div>
                    <div style={{ height: 5, background: "#27272a", borderRadius: 3, overflow: "hidden" }}>
                      <div style={{
                        height: "100%",
                        width: `${pct}%`,
                        background: meta.color,
                        borderRadius: 3,
                        transition: "width 0.5s ease",
                      }} />
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </Section>
      )}

      {/* ── Domain Intelligence ── */}
      {(data.domains?.length ?? 0) > 0 && (
        <Section title="Domain Intelligence" subtitle="Topics and fields you work across — classified by your AI tool">
          <div style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(180px, 1fr))",
            gap: 10,
          }}>
            {data.domains.map((d) => (
              <DomainCard key={d.tag} domain={d} max={maxDomain} />
            ))}
          </div>
        </Section>
      )}

      {/* ── Technical Skills ── */}
      {data.skills.length > 0 && (
        <Section title="Technical Skills" subtitle="Keywords extracted from your workflow content">
          <div style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
            gap: 10,
          }}>
            {data.skills.map((skill) => (
              <SkillCard key={skill.id} skill={skill} max={data.skills[0]?.strength ?? 1} />
            ))}
          </div>
        </Section>
      )}

      {data.skills.length === 0 && (data.domains?.length ?? 0) === 0 && (
        <EmptyState />
      )}
    </div>
  );
}

function Section({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <div style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 15, fontWeight: 600, color: "#f3f4f6", margin: 0 }}>{title}</h2>
        <p style={{ fontSize: 12, color: "#6b7280", margin: "4px 0 0" }}>{subtitle}</p>
      </div>
      {children}
    </section>
  );
}

function DomainCard({ domain, max }: { domain: DomainNode; max: number }) {
  const pct = Math.round((domain.strength / max) * 100);
  const label = domain.tag.replace(/_/g, " ");
  return (
    <div style={{
      background: "#18181b",
      borderRadius: 10,
      padding: "12px 14px",
      border: "1px solid #27272a",
      transition: "border-color 0.15s",
    }}>
      <div style={{
        fontSize: 13,
        fontWeight: 500,
        color: "#e5e7eb",
        marginBottom: 8,
        textTransform: "capitalize",
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}>
        {label}
      </div>
      <div style={{ height: 3, background: "#27272a", borderRadius: 2, marginBottom: 7, overflow: "hidden" }}>
        <div style={{
          height: "100%",
          width: `${pct}%`,
          background: "linear-gradient(90deg, #7c3aed, #2563eb)",
          borderRadius: 2,
          transition: "width 0.5s ease",
        }} />
      </div>
      <div style={{ fontSize: 11, color: "#6b7280" }}>
        {domain.session_count} session{domain.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}

function SkillCard({ skill, max }: { skill: SkillNode; max: number }) {
  const pct = Math.round((skill.strength / max) * 100);
  return (
    <div style={{
      background: "#18181b",
      borderRadius: 10,
      padding: "12px 14px",
      border: "1px solid #27272a",
    }}>
      <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 8, color: "#f3f4f6" }}>
        {skill.tag}
      </div>
      <div style={{ height: 3, background: "#27272a", borderRadius: 2, marginBottom: 7, overflow: "hidden" }}>
        <div style={{
          height: "100%",
          width: `${pct}%`,
          background: "#2563eb",
          borderRadius: 2,
          transition: "width 0.5s ease",
        }} />
      </div>
      <div style={{ fontSize: 11, color: "#6b7280" }}>
        {skill.session_count} session{skill.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <div style={{
      textAlign: "center",
      padding: "60px 20px",
      color: "#4b5563",
    }}>
      <div style={{ fontSize: 32, marginBottom: 16 }}>◎</div>
      <div style={{ fontSize: 15, fontWeight: 500, color: "#6b7280", marginBottom: 8 }}>
        No sessions logged yet
      </div>
      <div style={{ fontSize: 13, maxWidth: 320, margin: "0 auto", lineHeight: 1.6 }}>
        Connect Strata to Claude, Cursor, or another MCP-capable tool and your
        skill graph will populate automatically.
      </div>
    </div>
  );
}
