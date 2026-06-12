import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import SetupPanel from "../SetupPanel";

vi.mock("../../ipc", () => ({
  scanTranscripts: vi.fn(),
  runBackfill: vi.fn(),
  getIntegrations: vi.fn(),
  installIntegration: vi.fn(),
}));

import {
  scanTranscripts,
  runBackfill,
  getIntegrations,
  installIntegration,
} from "../../ipc";
const mockScan = vi.mocked(scanTranscripts);
const mockBackfill = vi.mocked(runBackfill);
const mockIntegrations = vi.mocked(getIntegrations);
const mockInstall = vi.mocked(installIntegration);

const SCAN = {
  projects: 3,
  sessions_total: 12,
  sessions_new: 9,
  earliest_day: "2026-01-02",
  latest_day: "2026-06-10",
};

const INTEGRATIONS = {
  integrations: [
    {
      id: "claude_desktop",
      name: "Claude Desktop",
      detected: true,
      installed: false,
      auto_installable: true,
      manual_command: null,
    },
    {
      id: "claude_code_mcp",
      name: "Claude Code — MCP server",
      detected: true,
      installed: false,
      auto_installable: false,
      manual_command: "claude mcp add --scope user strata -- /bin/strata",
    },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  mockScan.mockResolvedValue(SCAN);
  mockIntegrations.mockResolvedValue(INTEGRATIONS);
});

describe("SetupPanel", () => {
  it("shows scan results with new session count", async () => {
    render(<SetupPanel />);
    await waitFor(() => {
      expect(screen.getByText("9")).toBeInTheDocument();
      expect(screen.getByText(/new sessions/i)).toBeInTheDocument();
      expect(screen.getByText(/import 9 sessions/i)).toBeInTheDocument();
    });
  });

  it("runs the import and shows the report", async () => {
    mockBackfill.mockResolvedValue({
      sessions_ingested: 9,
      sessions_self_reported: 1,
      sessions_duplicate: 2,
      sessions_empty: 0,
      skills_touched: 7,
    });
    render(<SetupPanel />);
    await waitFor(() => screen.getByText(/import 9 sessions/i));

    fireEvent.click(screen.getByText(/import 9 sessions/i));
    await waitFor(() => {
      expect(mockBackfill).toHaveBeenCalledOnce();
      expect(screen.getByText(/imported 9 sessions/i)).toBeInTheDocument();
      expect(screen.getByText(/7 skills touched/i)).toBeInTheDocument();
    });
  });

  it("disables the import button when nothing is new", async () => {
    mockScan.mockResolvedValue({ ...SCAN, sessions_new: 0 });
    render(<SetupPanel />);
    await waitFor(() => {
      const button = screen.getByText(/everything imported/i);
      expect(button).toBeDisabled();
    });
  });

  it("connects an auto-installable integration", async () => {
    mockInstall.mockResolvedValue({
      integrations: [
        { ...INTEGRATIONS.integrations[0], installed: true },
        INTEGRATIONS.integrations[1],
      ],
    });
    render(<SetupPanel />);
    await waitFor(() => screen.getByText("Claude Desktop"));

    fireEvent.click(screen.getByText("Connect"));
    await waitFor(() => {
      expect(mockInstall).toHaveBeenCalledWith("claude_desktop");
      expect(screen.getByText("connected")).toBeInTheDocument();
    });
  });

  it("shows a copy-command button for manual integrations", async () => {
    render(<SetupPanel />);
    await waitFor(() => {
      expect(screen.getByText(/copy command/i)).toBeInTheDocument();
    });
  });

  it("surfaces scan errors instead of breaking", async () => {
    mockScan.mockRejectedValue("consent has been revoked");
    render(<SetupPanel />);
    await waitFor(() => {
      expect(screen.getByText(/consent has been revoked/i)).toBeInTheDocument();
    });
  });
});
