import { useState } from "react";
import SkillMap from "./components/SkillMap";
import ConsentControls from "./components/ConsentControls";
import GrowthTimeline from "./components/GrowthTimeline";
import type { Tab } from "./types";

const TABS: { id: Tab; label: string }[] = [
  { id: "skills", label: "Skill Map" },
  { id: "growth", label: "Growth" },
  { id: "consent", label: "Privacy & Consent" },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("skills");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh" }}>
      <header
        style={{
          padding: "12px 20px",
          borderBottom: "1px solid #222",
          display: "flex",
          alignItems: "center",
          gap: 24,
        }}
      >
        <span style={{ fontWeight: 700, fontSize: 18, letterSpacing: "-0.5px" }}>
          Strata
        </span>
        <nav style={{ display: "flex", gap: 8 }}>
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setActiveTab(t.id)}
              style={{
                padding: "6px 14px",
                borderRadius: 6,
                border: "none",
                cursor: "pointer",
                background: activeTab === t.id ? "#2563eb" : "transparent",
                color: activeTab === t.id ? "#fff" : "#9ca3af",
                fontWeight: 500,
                fontSize: 14,
              }}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      <main style={{ flex: 1, overflow: "auto", padding: 20 }}>
        {activeTab === "skills" && <SkillMap />}
        {activeTab === "growth" && <GrowthTimeline />}
        {activeTab === "consent" && <ConsentControls />}
      </main>
    </div>
  );
}
