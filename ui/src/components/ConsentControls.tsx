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

  const lamp = status ?? "granted";

  return (
    <section style={{ maxWidth: 620 }}>
      <header className="page-head rise">
        <div className="kicker">Your data, your rules</div>
        <h1 className="h-display">Privacy &amp; Consent</h1>
        <p className="sub" style={{ marginTop: 8, maxWidth: 480 }}>
          Strata only collects derived skill signals — never raw prompts or private content.
          You can pause or permanently revoke data collection at any time.
        </p>
      </header>

      {/* Status & controls */}
      <div className="card seam rise rise-1" style={{ padding: 20, marginBottom: 14 }}>
        <div className="consent-status">
          <span className={`lamp ${lamp}`} aria-hidden="true" />
          <span className="sub">Status:</span>
          <span className={`word ${lamp}`}>{status ?? "…"}</span>
        </div>

        <div style={{ display: "flex", gap: 10, flexWrap: "wrap" }}>
          {status === "granted" && (
            <button className="btn-solid btn warning" onClick={() => handle(pauseConsent)} disabled={busy}>
              Pause collection
            </button>
          )}
          {status === "paused" && (
            <button className="btn-solid btn primary" onClick={() => handle(resumeConsent)} disabled={busy}>
              Resume collection
            </button>
          )}
          {status !== "revoked" && (
            <button className="btn-solid btn danger" onClick={handleRevoke} disabled={busy}>
              Revoke &amp; delete all data
            </button>
          )}
          {status === "revoked" && (
            <p className="sub" style={{ color: "var(--rust)", margin: 0 }}>
              Consent revoked. All skill data has been deleted.
            </p>
          )}
        </div>
      </div>

      {error && <p className="sub" style={{ color: "var(--rust)", marginBottom: 12 }}>{error}</p>}

      <p className="mono rise rise-2" style={{ fontSize: 10.5, color: "var(--ink-faint)", marginBottom: 28, letterSpacing: "0.05em" }}>
        ⌂ All data is stored locally on your device. No data is sent to any server.
      </p>

      {/* Audit log */}
      <div className="rise rise-3">
        <div className="section-head">
          <div>
            <h2 className="h-section">Collection Log</h2>
            <p className="sub" style={{ marginTop: 3 }}>Every operation Strata performs, on the record</p>
          </div>
        </div>
        {auditLog.length === 0 ? (
          <p className="sub">No activity recorded yet.</p>
        ) : (
          <div className="audit-table">
            {auditLog.slice(0, 20).map((entry, i) => (
              <div key={i} className="audit-row">
                <div style={{ minWidth: 0 }}>
                  <span className="evt">{entry.event}</span>
                  {entry.detail && <span className="det">{entry.detail}</span>}
                </div>
                <span className="when">{new Date(entry.occurred_at).toLocaleString()}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
