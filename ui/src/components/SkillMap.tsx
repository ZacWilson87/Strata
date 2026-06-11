import { useEffect, useRef, useState } from "react";
import { getSkills } from "../ipc";
import type { DomainNode, SkillNode, SkillsResponse } from "../types";

const POLL_INTERVAL_MS = 10_000;

const WORK_TYPE_META: Record<string, { label: string; color: string; glyph: string }> = {
  creation:  { label: "Creation",  color: "var(--ochre)",  glyph: "✦" },
  debugging: { label: "Debugging", color: "var(--rust)",   glyph: "⬡" },
  analysis:  { label: "Analysis",  color: "var(--violet)", glyph: "◈" },
  research:  { label: "Research",  color: "var(--slate)",  glyph: "◎" },
  review:    { label: "Review",    color: "var(--sand)",   glyph: "◇" },
  planning:  { label: "Planning",  color: "var(--moss)",   glyph: "△" },
};

const FALLBACK_META = { label: "Other", color: "var(--ink-faint)", glyph: "○" };

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

  if (error) return <p className="sub" style={{ color: "var(--rust)" }}>Error: {error}</p>;
  if (!data) return <p className="sub">Loading…</p>;

  const totalWorkSessions = Object.values(data.work_types ?? {}).reduce((a, b) => a + b, 0);
  const sortedWorkTypes = Object.entries(data.work_types ?? {}).sort((a, b) => b[1] - a[1]);
  const maxDomain = data.domains?.[0]?.strength ?? 1;

  const updatedLabel = lastUpdated
    ? lastUpdated.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })
    : null;

  return (
    <div>
      <header className="page-head rise">
        <div className="kicker">Derived intelligence</div>
        <h1 className="h-display">Skill Map</h1>
      </header>

      <div className="statusbar rise rise-1">
        <div className="live">
          <span className="dot" aria-hidden="true" />
          <span>
            live · 10s
            {updatedLabel && <> · {updatedLabel}</>}
          </span>
        </div>
        <button className="btn" onClick={() => refresh(true)} disabled={refreshing}>
          {refreshing ? "Refreshing…" : "Refresh now"}
        </button>
      </div>

      {sortedWorkTypes.length > 0 && (
        <Section
          className="rise rise-2"
          title="Work Type Breakdown"
          subtitle="How you've been spending your AI sessions"
        >
          <div className="card seam" style={{ display: "flex", flexDirection: "column", gap: 12, padding: "18px 18px" }}>
            {sortedWorkTypes.map(([type, strength]) => {
              const meta = WORK_TYPE_META[type] ?? FALLBACK_META;
              const pct = totalWorkSessions > 0
                ? Math.round((strength / totalWorkSessions) * 100)
                : 0;
              return (
                <div key={type} style={{ display: "flex", alignItems: "center", gap: 14 }}>
                  <div
                    aria-hidden="true"
                    style={{
                      width: 30, height: 30, borderRadius: 8, flexShrink: 0,
                      display: "flex", alignItems: "center", justifyContent: "center",
                      fontSize: 13, color: meta.color,
                      background: "color-mix(in srgb, currentColor 9%, transparent)",
                      border: "1px solid color-mix(in srgb, currentColor 25%, transparent)",
                    }}
                  >
                    {meta.glyph}
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 6 }}>
                      <span style={{ fontSize: 13, fontWeight: 600 }}>{meta.label}</span>
                      <span className="mono" style={{ fontSize: 11, color: "var(--ink-faint)" }}>
                        {pct}% · {strength} sessions
                      </span>
                    </div>
                    <div className="meter">
                      <i style={{ width: `${pct}%`, background: meta.color }} />
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </Section>
      )}

      {(data.domains?.length ?? 0) > 0 && (
        <Section
          className="rise rise-3"
          title="Domain Intelligence"
          subtitle="Topics and fields you work across — classified by your AI tool"
        >
          <div className="card-grid">
            {data.domains.map((d) => (
              <DomainCard key={d.tag} domain={d} max={maxDomain} />
            ))}
          </div>
        </Section>
      )}

      {data.skills.length > 0 && (
        <Section
          className="rise rise-4"
          title="Technical Skills"
          subtitle="Keywords extracted from your workflow content"
        >
          <div className="card-grid">
            {data.skills.map((skill) => (
              <SkillCard key={skill.id} skill={skill} max={data.skills[0]?.strength ?? 1} />
            ))}
          </div>
        </Section>
      )}

      {data.skills.length === 0 && (data.domains?.length ?? 0) === 0 && <EmptyState />}
    </div>
  );
}

function Section({
  title,
  subtitle,
  children,
  className,
}: {
  title: string;
  subtitle: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={`section ${className ?? ""}`}>
      <div className="section-head">
        <div>
          <h2 className="h-section">{title}</h2>
          <p className="sub" style={{ marginTop: 3 }}>{subtitle}</p>
        </div>
      </div>
      {children}
    </section>
  );
}

function DomainCard({ domain, max }: { domain: DomainNode; max: number }) {
  const pct = Math.round((domain.strength / max) * 100);
  const label = domain.tag.replace(/_/g, " ");
  return (
    <div className="card">
      <div
        style={{
          fontSize: 13, fontWeight: 600, marginBottom: 10, textTransform: "capitalize",
          whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
        }}
      >
        {label}
      </div>
      <div className="meter" style={{ marginBottom: 8 }}>
        <i style={{ width: `${pct}%`, background: "linear-gradient(90deg, var(--ochre), var(--rust))" }} />
      </div>
      <div className="mono" style={{ fontSize: 10.5, color: "var(--ink-faint)" }}>
        {domain.session_count} session{domain.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}

function SkillCard({ skill, max }: { skill: SkillNode; max: number }) {
  const pct = Math.round((skill.strength / max) * 100);
  const active = (skill.recent_strength ?? 0) > 0.5;
  return (
    <div className="card">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", marginBottom: 10 }}>
        <span style={{ fontWeight: 600, fontSize: 14 }}>{skill.tag}</span>
        {active && (
          <span className="mono" title="Active recently" style={{ fontSize: 10, color: "var(--ochre)" }}>
            ▲
          </span>
        )}
      </div>
      <div className="meter" style={{ marginBottom: 8 }}>
        <i style={{ width: `${pct}%`, background: "var(--slate)" }} />
      </div>
      <div className="mono" style={{ fontSize: 10.5, color: "var(--ink-faint)" }}>
        {skill.session_count} session{skill.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="empty rise rise-2">
      <div className="glyph" aria-hidden="true"><span /><span /><span /></div>
      <div className="title">No sessions logged yet</div>
      <div className="hint">
        Connect Strata to Claude, Cursor, or another MCP-capable tool and your
        skill graph will populate automatically.
      </div>
    </div>
  );
}
