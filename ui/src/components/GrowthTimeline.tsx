import { useEffect, useMemo, useState } from "react";
import { dismissInsight, getGrowth, getInsights, getSkillHistory, getTopicSummaries } from "../ipc";
import type {
  GrowthResponse,
  Insight,
  SkillWithVelocity,
  TopicSummaryEntry,
  VelocityDirection,
  WeeklySnapshot,
} from "../types";

const BAND_COLORS = ["var(--ochre)", "var(--rust)", "var(--moss)", "var(--slate)", "var(--violet)"];

const DIRECTION_META: Record<VelocityDirection, { label: string; arrow: string; blurb: string }> = {
  accelerating: { label: "Accelerating", arrow: "↗", blurb: "More sessions this week than last" },
  new:          { label: "Emerging",     arrow: "✧", blurb: "First appeared in the last 7 days" },
  declining:    { label: "Cooling",      arrow: "↘", blurb: "Less activity than the prior week" },
  stable:       { label: "Steady",       arrow: "→", blurb: "Holding pace week over week" },
};

interface DisplaySkill extends SkillWithVelocity {
  displayTag: string;
  isDomain: boolean;
  recentStrength: number;
}

export default function GrowthTimeline() {
  const [growth, setGrowth] = useState<GrowthResponse | null>(null);
  const [weeks, setWeeks] = useState<WeeklySnapshot[]>([]);
  const [journal, setJournal] = useState<TopicSummaryEntry[]>([]);
  const [insights, setInsights] = useState<Insight[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.allSettled([getGrowth(), getSkillHistory(), getTopicSummaries(), getInsights()]).then(
      ([g, h, t, ins]) => {
        if (g.status === "fulfilled") setGrowth(g.value);
        if (h.status === "fulfilled") setWeeks(h.value.weeks);
        if (t.status === "fulfilled") setJournal(t.value.summaries);
        if (ins.status === "fulfilled") setInsights(ins.value.insights);
        if (g.status === "rejected") setError(String(g.reason));
        setLoading(false);
      }
    );
  }, []);

  const handleDismiss = async (id: string) => {
    setInsights((prev) => prev.filter((i) => i.id !== id));
    try {
      await dismissInsight(id);
    } catch {
      /* dismissal is best-effort; card is already hidden locally */
    }
  };

  const skills: DisplaySkill[] = useMemo(() => {
    if (!growth) return [];
    return growth.skills
      .filter((s) => !s.tag.startsWith("wt:") && !s.tag.startsWith("tool:"))
      .map((s) => ({
        ...s,
        isDomain: s.tag.startsWith("dt:"),
        displayTag: s.tag.replace(/^dt:/, "").replace(/_/g, " "),
        recentStrength: growth.recent_strengths[s.tag] ?? 0,
      }));
  }, [growth]);

  const byDirection = useMemo(() => {
    const groups: Record<VelocityDirection, DisplaySkill[]> = {
      accelerating: [], new: [], declining: [], stable: [],
    };
    for (const s of skills) groups[s.velocity.direction].push(s);
    groups.accelerating.sort((a, b) => b.velocity.delta - a.velocity.delta);
    groups.declining.sort((a, b) => a.velocity.delta - b.velocity.delta);
    groups.new.sort((a, b) => b.velocity.recent_sessions - a.velocity.recent_sessions);
    return groups;
  }, [skills]);

  const activeNow = useMemo(
    () =>
      [...skills]
        .filter((s) => s.recentStrength > 0)
        .sort((a, b) => b.recentStrength - a.recentStrength)
        .slice(0, 8),
    [skills]
  );

  if (loading) return <p className="sub">Loading…</p>;
  if (error) return <p className="sub" style={{ color: "var(--rust)" }}>Error: {error}</p>;

  const hasAnything = skills.length > 0 || weeks.length > 0 || journal.length > 0;
  if (!hasAnything) {
    return (
      <div>
        <PageHead />
        <div className="empty rise rise-1">
          <div className="glyph" aria-hidden="true"><span /><span /><span /></div>
          <div className="title">No growth data yet</div>
          <div className="hint">
            Activity will appear here after your first sessions are logged.
            Each week of work lays down a new layer.
          </div>
        </div>
      </div>
    );
  }

  const movers = ([...byDirection.accelerating, ...byDirection.new, ...byDirection.declining]);

  return (
    <div>
      <PageHead />

      {/* ── Insights (craft) ── */}
      {insights.length > 0 && (
        <section className="section rise rise-1">
          <div className="section-head">
            <div>
              <h2 className="h-section">Insights</h2>
              <p className="sub" style={{ marginTop: 3 }}>
                Patterns in how you work with AI tools — and what they suggest. Dismissals are permanent.
              </p>
            </div>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            {insights.map((insight) => (
              <InsightCard key={insight.id} insight={insight} onDismiss={handleDismiss} />
            ))}
          </div>
        </section>
      )}

      {/* ── Momentum ── */}
      {movers.length > 0 && (
        <section className="section rise rise-1">
          <div className="section-head">
            <div>
              <h2 className="h-section">Momentum</h2>
              <p className="sub" style={{ marginTop: 3 }}>
                Seven-day velocity against the week before
                {byDirection.stable.length > 0 && (
                  <span className="mono" style={{ fontSize: 10.5 }}>
                    {" "}· {byDirection.stable.length} steady
                  </span>
                )}
              </p>
            </div>
          </div>
          <div className="momentum-grid">
            {movers.slice(0, 9).map((s) => (
              <VelocityCard key={s.id} skill={s} />
            ))}
          </div>
        </section>
      )}

      {/* ── Active now ── */}
      {activeNow.length > 0 && (
        <section className="section rise rise-2">
          <div className="section-head">
            <div>
              <h2 className="h-section">Active Now</h2>
              <p className="sub" style={{ marginTop: 3 }}>
                Recency-weighted strength — unused skills fade with a 30-day half-life
              </p>
            </div>
          </div>
          <div className="card seam" style={{ display: "flex", flexDirection: "column", gap: 11, padding: "18px 18px" }}>
            {activeNow.map((s) => {
              const max = activeNow[0]?.recentStrength || 1;
              const pct = Math.max(Math.round((s.recentStrength / max) * 100), 3);
              return (
                <div key={s.id} style={{ display: "flex", alignItems: "center", gap: 12 }}>
                  <span
                    style={{
                      fontSize: 12.5, fontWeight: 600, minWidth: 130, textTransform: "capitalize",
                      whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
                    }}
                  >
                    {s.displayTag}
                    {s.isDomain && <span className="mono" style={{ fontSize: 9, color: "var(--ink-faint)", marginLeft: 6 }}>domain</span>}
                  </span>
                  <div className="meter" style={{ flex: 1 }}>
                    <i style={{ width: `${pct}%`, background: "linear-gradient(90deg, var(--ochre), var(--sand))" }} />
                  </div>
                  <span className="mono" style={{ fontSize: 10.5, color: "var(--ink-faint)", minWidth: 64, textAlign: "right" }}>
                    {s.recentStrength.toFixed(1)} / {s.strength.toFixed(0)} all-time
                  </span>
                </div>
              );
            })}
          </div>
        </section>
      )}

      {/* ── Weekly strata ── */}
      {weeks.length > 0 && (
        <section className="section rise rise-3">
          <div className="section-head">
            <div>
              <h2 className="h-section">Weekly Strata</h2>
              <p className="sub" style={{ marginTop: 3 }}>
                Each column is a week; each band a skill laid down in it
              </p>
            </div>
          </div>
          <div className="card" style={{ padding: "20px 18px 14px" }}>
            <StrataChart weeks={weeks} />
          </div>
        </section>
      )}

      {/* ── Work journal ── */}
      {journal.length > 0 && (
        <section className="section rise rise-4">
          <div className="section-head">
            <div>
              <h2 className="h-section">Work Journal</h2>
              <p className="sub" style={{ marginTop: 3 }}>
                One-sentence summaries your AI tools logged — the full extent of what Strata stores
              </p>
            </div>
          </div>
          <div className="journal">
            {journal.slice(0, 12).map((entry) => (
              <div key={`${entry.timestamp_ms}-${entry.conversation_id ?? ""}`} className="journal-entry">
                <div className="journal-date">{formatStamp(entry.timestamp_ms)}</div>
                <div className="journal-text">{entry.summary}</div>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function InsightCard({
  insight,
  onDismiss,
}: {
  insight: Insight;
  onDismiss: (id: string) => void;
}) {
  return (
    <div className="card seam insight-card">
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="insight-title">{insight.title}</div>
        <p className="insight-body">{insight.body}</p>
        <div className="insight-evidence mono">{insight.evidence}</div>
      </div>
      <button
        className="btn insight-dismiss"
        title="Dismiss permanently"
        aria-label={`Dismiss insight: ${insight.title}`}
        onClick={() => onDismiss(insight.id)}
      >
        ✕
      </button>
    </div>
  );
}

function PageHead() {
  return (
    <header className="page-head rise">
      <div className="kicker">Momentum &amp; trajectory</div>
      <h1 className="h-display">Growth</h1>
    </header>
  );
}

function VelocityCard({ skill }: { skill: DisplaySkill }) {
  const meta = DIRECTION_META[skill.velocity.direction];
  const dirClass = `dir-${skill.velocity.direction}`;
  const delta = skill.velocity.delta;
  const deltaLabel =
    skill.velocity.direction === "new"
      ? `${skill.velocity.recent_sessions}`
      : `${delta > 0 ? "+" : ""}${delta}`;
  return (
    <div className="card velo-card" title={meta.blurb}>
      <div className="velo-head">
        <span className="velo-tag">{skill.displayTag}</span>
        {skill.isDomain && <span className="pill">domain</span>}
      </div>
      <div className="velo-delta">
        <span className={`arrow ${dirClass}`} aria-hidden="true">{meta.arrow}</span>
        <span className={`num-display ${dirClass}`}>{deltaLabel}</span>
        <span className="mono" style={{ fontSize: 10, color: "var(--ink-faint)" }}>
          {skill.velocity.direction === "new" ? "sessions" : "vs prior wk"}
        </span>
      </div>
      <div className={`velo-meta`}>
        {meta.label} · {skill.velocity.recent_sessions} this week
      </div>
    </div>
  );
}

function StrataChart({ weeks }: { weeks: WeeklySnapshot[] }) {
  const [hovered, setHovered] = useState<string | null>(null);
  const max = Math.max(...weeks.map((w) => w.total_sessions), 1);

  return (
    <div>
      <div className="strata-chart">
        {weeks.map((w) => {
          const heightPct = Math.max((w.total_sessions / max) * 100, 5);
          const bands = w.top_tags.length || 1;
          return (
            <div
              key={w.week}
              className="strata-col"
              style={{ height: `${heightPct}%` }}
              onMouseEnter={() => setHovered(w.week)}
              onMouseLeave={() => setHovered(null)}
            >
              {(w.top_tags.length > 0 ? w.top_tags : ["—"]).map((tag, i) => (
                <div
                  key={tag}
                  className="band"
                  style={{
                    flex: 1,
                    background: BAND_COLORS[i % BAND_COLORS.length],
                    opacity: 0.55 + (0.45 * (bands - i)) / bands,
                  }}
                />
              ))}
              {hovered === w.week && (
                <div className="strata-tip">
                  <div style={{ fontWeight: 600, marginBottom: 2 }}>
                    {w.total_sessions} session{w.total_sessions !== 1 ? "s" : ""}
                  </div>
                  <div className="mono">{w.top_tags.join(" · ") || "no tagged skills"}</div>
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="strata-axis">
        {weeks.map((w) => (
          <span key={w.week}>{w.week.replace(/^\d{4}-/, "")}</span>
        ))}
      </div>
    </div>
  );
}

function formatStamp(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleDateString([], { month: "short", day: "numeric" }) +
    " · " +
    d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
