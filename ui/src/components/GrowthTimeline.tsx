import { useEffect, useState } from "react";
import { getSkillHistory } from "../ipc";
import type { WeeklySnapshot } from "../types";

export default function GrowthTimeline() {
  const [weeks, setWeeks] = useState<WeeklySnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSkillHistory()
      .then((r) => setWeeks(r.weeks))
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <p style={{ color: "#6b7280", fontSize: 14 }}>Loading…</p>;
  if (error) return <p style={{ color: "#ef4444", fontSize: 14 }}>Error: {error}</p>;
  if (weeks.length === 0) {
    return (
      <div style={{ textAlign: "center", padding: "60px 20px", color: "#4b5563" }}>
        <p style={{ fontSize: 15, marginBottom: 8 }}>No growth data yet.</p>
        <p style={{ fontSize: 13 }}>Activity will appear here after your first sessions are logged.</p>
      </div>
    );
  }

  const maxSessions = Math.max(...weeks.map((w) => w.total_sessions), 1);

  return (
    <section style={{ maxWidth: 700 }}>
      <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 4, color: "#f3f4f6" }}>
        Growth Timeline
      </h2>
      <p style={{ fontSize: 13, color: "#6b7280", marginBottom: 24 }}>
        Skill activity over the last 8 weeks.
      </p>

      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        {weeks.map((snap) => (
          <WeekRow key={snap.week} snap={snap} maxSessions={maxSessions} />
        ))}
      </div>
    </section>
  );
}

function WeekRow({ snap, maxSessions }: { snap: WeeklySnapshot; maxSessions: number }) {
  const [hovered, setHovered] = useState(false);
  const pct = Math.max((snap.total_sessions / maxSessions) * 100, 4);

  return (
    <div
      style={{ display: "flex", alignItems: "center", gap: 14, position: "relative" }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* Week label */}
      <span
        style={{
          fontSize: 12,
          color: "#6b7280",
          fontFamily: "monospace",
          minWidth: 80,
          flexShrink: 0,
        }}
      >
        {snap.week}
      </span>

      {/* Bar */}
      <div style={{ flex: 1, background: "#27272a", borderRadius: 4, height: 24, overflow: "hidden" }}>
        <div
          style={{
            width: `${pct}%`,
            height: "100%",
            background: "linear-gradient(90deg, #2563eb, #7c3aed)",
            borderRadius: 4,
            transition: "width 0.3s ease",
          }}
        />
      </div>

      {/* Session count */}
      <span style={{ fontSize: 12, color: "#9ca3af", minWidth: 36, textAlign: "right", flexShrink: 0 }}>
        {snap.total_sessions}
      </span>

      {/* Tag tooltip on hover */}
      {hovered && snap.top_tags.length > 0 && (
        <div
          style={{
            position: "absolute",
            left: 94,
            top: -36,
            background: "#1f2937",
            border: "1px solid #374151",
            borderRadius: 6,
            padding: "5px 10px",
            fontSize: 12,
            color: "#e5e7eb",
            whiteSpace: "nowrap",
            zIndex: 10,
            pointerEvents: "none",
          }}
        >
          {snap.top_tags.join(", ")}
        </div>
      )}
    </div>
  );
}
