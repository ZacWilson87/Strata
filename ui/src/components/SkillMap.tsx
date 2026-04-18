import { useEffect, useState } from "react";
import { getSkills } from "../ipc";
import type { SkillNode, SkillsResponse } from "../types";

export default function SkillMap() {
  const [data, setData] = useState<SkillsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSkills()
      .then(setData)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  if (error) return <p style={{ color: "#ef4444" }}>Error: {error}</p>;
  if (!data) return <p style={{ color: "#9ca3af" }}>Loading skills…</p>;

  return (
    <section>
      <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 16, color: "#f3f4f6" }}>
        Skill Map
      </h2>
      <p style={{ color: "#6b7280", fontSize: 13, marginBottom: 20 }}>{data.summary}</p>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
          gap: 12,
        }}
      >
        {data.skills.map((skill) => (
          <SkillCard key={skill.id} skill={skill} max={data.skills[0]?.strength ?? 1} />
        ))}
      </div>
    </section>
  );
}

function SkillCard({ skill, max }: { skill: SkillNode; max: number }) {
  const pct = Math.round((skill.strength / max) * 100);
  return (
    <div
      style={{
        background: "#18181b",
        borderRadius: 10,
        padding: "14px 16px",
        border: "1px solid #27272a",
      }}
    >
      <div style={{ fontWeight: 600, fontSize: 15, marginBottom: 8 }}>{skill.tag}</div>
      <div
        style={{
          height: 4,
          background: "#27272a",
          borderRadius: 2,
          marginBottom: 8,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            height: "100%",
            width: `${pct}%`,
            background: "#2563eb",
            borderRadius: 2,
            transition: "width 0.4s ease",
          }}
        />
      </div>
      <div style={{ fontSize: 12, color: "#6b7280" }}>
        {skill.session_count} session{skill.session_count !== 1 ? "s" : ""}
      </div>
    </div>
  );
}
