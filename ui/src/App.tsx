import { useEffect, useState } from "react";
import SkillMap from "./components/SkillMap";
import ConsentControls from "./components/ConsentControls";
import GrowthTimeline from "./components/GrowthTimeline";
import SetupPanel from "./components/SetupPanel";
import { getSkills } from "./ipc";
import type { Tab } from "./types";

const TABS: { id: Tab; label: string; idx: string }[] = [
  { id: "skills", label: "Skill Map", idx: "01" },
  { id: "growth", label: "Growth", idx: "02" },
  { id: "consent", label: "Privacy & Consent", idx: "03" },
  { id: "setup", label: "Setup", idx: "04" },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("skills");

  // First run: an empty graph means there's nothing to look at yet — land on
  // Setup, where one click imports the user's existing local history.
  useEffect(() => {
    let cancelled = false;
    getSkills()
      .then((r) => {
        if (!cancelled && r.skills.length === 0 && r.domains.length === 0) {
          setActiveTab("setup");
        }
      })
      .catch(() => {
        // Consent paused/revoked or backend unavailable — stay on Skill Map.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="shell">
      <aside className="rail">
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            <span /><span /><span /><span />
          </div>
          <div className="brand-name">Strata</div>
        </div>

        <nav aria-label="Sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setActiveTab(t.id)}
              className={`rail-link${activeTab === t.id ? " active" : ""}`}
            >
              <span className="idx">{t.idx}</span>
              {t.label}
            </button>
          ))}
        </nav>

        <div className="rail-foot">
          <span className="dot" aria-hidden="true" />
          local-first · no cloud
        </div>
      </aside>

      <main className="stage">
        <div className="stage-inner" key={activeTab}>
          {activeTab === "skills" && <SkillMap />}
          {activeTab === "growth" && <GrowthTimeline />}
          {activeTab === "consent" && <ConsentControls />}
          {activeTab === "setup" && <SetupPanel />}
        </div>
      </main>
    </div>
  );
}
