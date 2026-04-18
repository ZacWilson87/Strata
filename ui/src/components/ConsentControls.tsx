import { useEffect, useState } from "react";
import { getConsentStatus, pauseConsent, resumeConsent } from "../ipc";

export default function ConsentControls() {
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    getConsentStatus()
      .then(setStatus)
      .catch((e: unknown) => setError(String(e)));

  useEffect(() => { refresh(); }, []);

  const handle = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section style={{ maxWidth: 520 }}>
      <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 8, color: "#f3f4f6" }}>
        Privacy & Consent
      </h2>
      <p style={{ fontSize: 13, color: "#6b7280", marginBottom: 20 }}>
        Strata only collects derived skill signals — never raw prompts or private content.
        You can pause or revoke data collection at any time.
      </p>

      <div
        style={{
          background: "#18181b",
          border: "1px solid #27272a",
          borderRadius: 10,
          padding: 20,
          marginBottom: 16,
        }}
      >
        <div style={{ marginBottom: 16 }}>
          <span style={{ fontSize: 13, color: "#9ca3af" }}>Status: </span>
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: status === "granted" ? "#22c55e" : status === "paused" ? "#f59e0b" : "#ef4444",
            }}
          >
            {status ?? "…"}
          </span>
        </div>

        <div style={{ display: "flex", gap: 10 }}>
          {status === "granted" && (
            <ActionButton
              label="Pause collection"
              onClick={() => handle(pauseConsent)}
              disabled={busy}
              variant="warning"
            />
          )}
          {status === "paused" && (
            <ActionButton
              label="Resume collection"
              onClick={() => handle(resumeConsent)}
              disabled={busy}
              variant="primary"
            />
          )}
        </div>
      </div>

      {error && <p style={{ color: "#ef4444", fontSize: 13 }}>{error}</p>}

      <p style={{ fontSize: 12, color: "#4b5563", marginTop: 20 }}>
        All data is stored locally on your device. No data is sent to any server.
      </p>
    </section>
  );
}

function ActionButton({
  label,
  onClick,
  disabled,
  variant,
}: {
  label: string;
  onClick: () => void;
  disabled: boolean;
  variant: "primary" | "warning" | "danger";
}) {
  const colors = {
    primary: { bg: "#2563eb", hover: "#1d4ed8" },
    warning: { bg: "#d97706", hover: "#b45309" },
    danger: { bg: "#dc2626", hover: "#b91c1c" },
  };
  const c = colors[variant];
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "8px 16px",
        borderRadius: 6,
        border: "none",
        cursor: disabled ? "not-allowed" : "pointer",
        background: disabled ? "#374151" : c.bg,
        color: "#fff",
        fontSize: 13,
        fontWeight: 500,
        opacity: disabled ? 0.6 : 1,
      }}
    >
      {label}
    </button>
  );
}
