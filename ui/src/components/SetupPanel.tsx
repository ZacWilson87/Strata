import { useCallback, useEffect, useState } from "react";
import {
  getIntegrations,
  installIntegration,
  runBackfill,
  scanTranscripts,
} from "../ipc";
import type { BackfillReport, IntegrationStatus, ScanReport } from "../types";

/**
 * Setup page — first-minute time-to-value.
 *
 * 1. Import history: scan local Claude Code transcripts (already on this
 *    machine) and run them through the privacy pipeline, populating the
 *    dashboard with months of real data in seconds.
 * 2. Connections: wire Strata into the AI clients' local configs so every
 *    future session is captured.
 */
export default function SetupPanel() {
  const [scan, setScan] = useState<ScanReport | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [report, setReport] = useState<BackfillReport | null>(null);
  const [importError, setImportError] = useState<string | null>(null);

  const [integrations, setIntegrations] = useState<IntegrationStatus[]>([]);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refreshScan = useCallback(() => {
    scanTranscripts()
      .then((r) => {
        setScan(r);
        setScanError(null);
      })
      .catch((e) => setScanError(String(e)));
  }, []);

  useEffect(() => {
    refreshScan();
    getIntegrations()
      .then((r) => setIntegrations(r.integrations))
      .catch((e) => setConnectError(String(e)));
  }, [refreshScan]);

  const handleImport = async () => {
    setImporting(true);
    setImportError(null);
    try {
      setReport(await runBackfill());
      refreshScan();
    } catch (e) {
      setImportError(String(e));
    } finally {
      setImporting(false);
    }
  };

  const handleConnect = async (id: string) => {
    setConnecting(id);
    setConnectError(null);
    try {
      const r = await installIntegration(id);
      setIntegrations(r.integrations);
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setConnecting(null);
    }
  };

  const handleCopy = async (command: string) => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard unavailable — the command is still visible to select manually.
    }
  };

  return (
    <div>
      <div className="page-head rise">
        <div className="kicker">04 · Setup</div>
        <h1 className="h-display">Start with your real history</h1>
        <p className="sub">
          Everything on this page runs locally. Transcripts are read on this
          device, reduced to skill tags in memory, and discarded — raw prompts
          are never stored or sent anywhere.
        </p>
      </div>

      <section className="section rise rise-1">
        <div className="section-head">
          <h2 className="h-section">Import your history</h2>
        </div>
        <div className="card seam import-card">
          {scanError && <p className="setup-error">{scanError}</p>}
          {!scan && !scanError && <p className="sub">Scanning local transcripts…</p>}
          {scan && (
            <>
              <div className="scan-figures">
                <div className="scan-figure">
                  <span className="num-display">{scan.sessions_new}</span>
                  <span className="scan-label">new sessions</span>
                </div>
                <div className="scan-figure">
                  <span className="num-display">{scan.sessions_total}</span>
                  <span className="scan-label">found on disk</span>
                </div>
                <div className="scan-figure">
                  <span className="num-display">{scan.projects}</span>
                  <span className="scan-label">projects</span>
                </div>
              </div>
              {scan.earliest_day && scan.latest_day && (
                <p className="sub mono scan-range">
                  {scan.earliest_day} → {scan.latest_day}
                </p>
              )}
              <div className="import-actions">
                <button
                  className="btn btn-solid primary"
                  onClick={handleImport}
                  disabled={importing || scan.sessions_new === 0}
                >
                  {importing
                    ? "Importing…"
                    : scan.sessions_new === 0
                      ? "Everything imported"
                      : `Import ${scan.sessions_new} sessions`}
                </button>
              </div>
              {importError && <p className="setup-error">{importError}</p>}
              {report && (
                <p className="sub import-result">
                  Imported {report.sessions_ingested} sessions ·{" "}
                  {report.skills_touched} skills touched
                  {report.sessions_self_reported > 0 &&
                    ` · ${report.sessions_self_reported} already self-reported`}
                </p>
              )}
            </>
          )}
        </div>
      </section>

      <section className="section rise rise-2">
        <div className="section-head">
          <h2 className="h-section">Connect your tools</h2>
        </div>
        {connectError && <p className="setup-error">{connectError}</p>}
        <div className="connect-list">
          {integrations.map((integration) => (
            <div className="card connect-row" key={integration.id}>
              <span
                className={`lamp ${integration.installed ? "granted" : "paused"}`}
                aria-hidden="true"
              />
              <div className="connect-name">
                {integration.name}
                {!integration.detected && (
                  <span className="connect-note"> · not detected</span>
                )}
              </div>
              {integration.installed ? (
                <span className="connect-state mono">connected</span>
              ) : integration.auto_installable ? (
                <button
                  className="btn"
                  onClick={() => handleConnect(integration.id)}
                  disabled={connecting === integration.id || !integration.detected}
                >
                  {connecting === integration.id ? "Connecting…" : "Connect"}
                </button>
              ) : integration.manual_command ? (
                <button
                  className="btn"
                  onClick={() => handleCopy(integration.manual_command as string)}
                  title={integration.manual_command}
                >
                  {copied ? "Copied" : "Copy command"}
                </button>
              ) : null}
            </div>
          ))}
        </div>
        <p className="sub connect-foot">
          Strata speaks standard MCP — any MCP-capable client works (Zed,
          Cline, and others: see docs/client-rules.md for copy-paste rules).
          The session capture hook records each Claude Code session as it
          ends. Connections edit local config files only.
        </p>
      </section>
    </div>
  );
}
