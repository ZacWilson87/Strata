import { useEffect, useState } from "react";
import { getConsentStatus, pauseConsent, resumeConsent, revokeConsent, getAuditLog } from "../ipc";
import type { AuditEntry } from "../types";

export default function ConsentControls() {
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [auditLog, setAuditLog] = useState<AuditEntry[]>([]);

  const refresh = async () => {
    getConsentStatus()
      .then(setStatus)
      .catch((e: unknown) => setError(String(e)));
    getAuditLog()
      .then((r) => setAuditLog(r.entries))
      .catch(() => {/* audit log is best-effort */});
  };

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

  const handleRevoke = () => {
    if (!window.confirm(
      "Permanently revoke consent? This will delete all collected skill data and cannot be undone."
    )) return;
    handle(revokeConsent);
  };

  return (
    <section style={{ maxWidth: 560 }}>
      <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 8, color: "#f3f4f6" }}>
        Privacy & Consent
      </h2>
      <p style={{ fontSize: 13, color: "#6b7280", marginBottom: 20 }}>
        Strata only collects derived skill signals — never raw prompts or private content.
        You can pause or permanently revoke data collection at any time.
      </p>

      {/* Status & controls */}
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
              color:
                status === "granted"
                  ? "#22c55e"
                  : status === "paused"
                  ? "#f59e0b"
                  : "#ef4444",
            }}
          >
            {status ?? "…"}
          </span>
        </div>

        <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
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
          {status !== "revoked" && (
            <ActionButton
              label="Revoke & delete all data"
              onClick={handleRevoke}
              disabled={busy}
              variant="danger"
            />
          )}
          {status === "revoked" && (
            <p style={{ fontSize: 13, color: "#ef4444", margin: 0 }}>
              Consent revoked. All skill data has been deleted.
            </p>
          )}
        </div>
      </div>

      {error && <p style={{ color: "#ef4444", fontSize: 13, marginBottom: 12 }}>{error}</p>}

      <p style={{ fontSize: 12, color: "#4b5563", marginBottom: 24 }}>
        All data is stored locally on your device. No data is sent to any server.
      </p>

      {/* Audit log */}
      <div>
        <h3 style={{ fontSize: 14, fontWeight: 600, color: "#9ca3af", marginBottom: 10 }}>
          Collection Log
        </h3>
        {auditLog.length === 0 ? (
          <p style={{ fontSize: 13, color: "#4b5563" }}>No activity recorded yet.</p>
        ) : (
          <div
            style={{
              background: "#18181b",
              border: "1px solid #27272a",
              borderRadius: 8,
              overflow: "hidden",
            }}
          >
            {auditLog.slice(0, 20).map((entry, i) => (
              <div
                key={i}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "8px 14px",
                  borderBottom: i < Math.min(auditLog.length, 20) - 1 ? "1px solid #27272a" : "none",
                }}
              >
                <div>
                  <span style={{ fontSize: 12, color: "#e5e7eb", fontFamily: "monospace" }}>
                    {entry.event}
                  </span>
                  {entry.detail && (
                    <span style={{ fontSize: 11, color: "#6b7280", marginLeft: 8 }}>
                      {entry.detail}
                    </span>
                  )}
                </div>
                <span style={{ fontSize: 11, color: "#4b5563", whiteSpace: "nowrap", marginLeft: 12 }}>
                  {new Date(entry.occurred_at).toLocaleString()}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
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
    primary: { bg: "#2563eb" },
    warning: { bg: "#d97706" },
    danger: { bg: "#dc2626" },
  };
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "8px 16px",
        borderRadius: 6,
        border: "none",
        cursor: disabled ? "not-allowed" : "pointer",
        background: disabled ? "#374151" : colors[variant].bg,
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
