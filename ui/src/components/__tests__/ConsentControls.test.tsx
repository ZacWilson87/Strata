import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import ConsentControls from "../ConsentControls";

vi.mock("../../ipc", () => ({
  getConsentStatus: vi.fn(),
  pauseConsent: vi.fn(),
  resumeConsent: vi.fn(),
  revokeConsent: vi.fn(),
  getAuditLog: vi.fn(),
}));

import { getConsentStatus, pauseConsent, resumeConsent, revokeConsent, getAuditLog } from "../../ipc";
const mockStatus = vi.mocked(getConsentStatus);
const mockPause = vi.mocked(pauseConsent);
const mockResume = vi.mocked(resumeConsent);
const mockRevoke = vi.mocked(revokeConsent);
const mockAuditLog = vi.mocked(getAuditLog);

beforeEach(() => {
  vi.clearAllMocks();
  mockAuditLog.mockResolvedValue({ entries: [] });
  // Stub window.confirm so revoke tests don't hang
  vi.stubGlobal("confirm", () => true);
});

describe("ConsentControls", () => {
  it("shows granted status", async () => {
    mockStatus.mockResolvedValue("granted");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText("granted")).toBeInTheDocument();
    });
  });

  it("shows pause button when granted", async () => {
    mockStatus.mockResolvedValue("granted");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText(/pause collection/i)).toBeInTheDocument();
    });
  });

  it("calls pauseConsent when pause button clicked", async () => {
    mockStatus.mockResolvedValue("granted");
    mockPause.mockResolvedValue(undefined);
    mockStatus.mockResolvedValueOnce("granted").mockResolvedValueOnce("paused");

    render(<ConsentControls />);
    await waitFor(() => screen.getByText(/pause collection/i));
    fireEvent.click(screen.getByText(/pause collection/i));

    await waitFor(() => {
      expect(mockPause).toHaveBeenCalledOnce();
    });
  });

  it("shows resume button when paused", async () => {
    mockStatus.mockResolvedValue("paused");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText(/resume collection/i)).toBeInTheDocument();
    });
  });

  it("calls resumeConsent when resume button clicked", async () => {
    mockStatus.mockResolvedValue("paused");
    mockResume.mockResolvedValue(undefined);
    mockStatus.mockResolvedValueOnce("paused").mockResolvedValueOnce("granted");

    render(<ConsentControls />);
    await waitFor(() => screen.getByText(/resume collection/i));
    fireEvent.click(screen.getByText(/resume collection/i));

    await waitFor(() => {
      expect(mockResume).toHaveBeenCalledOnce();
    });
  });

  it("shows revoke button when granted or paused", async () => {
    mockStatus.mockResolvedValue("granted");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /revoke/i })).toBeInTheDocument();
    });
  });

  it("calls revokeConsent after confirmation", async () => {
    mockStatus.mockResolvedValue("granted");
    mockRevoke.mockResolvedValue(undefined);
    mockStatus.mockResolvedValueOnce("granted").mockResolvedValueOnce("revoked");

    render(<ConsentControls />);
    await waitFor(() => screen.getByRole("button", { name: /revoke/i }));
    fireEvent.click(screen.getByRole("button", { name: /revoke/i }));

    await waitFor(() => {
      expect(mockRevoke).toHaveBeenCalledOnce();
    });
  });

  it("shows revoked message when status is revoked", async () => {
    mockStatus.mockResolvedValue("revoked");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText(/consent revoked/i)).toBeInTheDocument();
    });
  });

  it("renders audit log entries", async () => {
    mockStatus.mockResolvedValue("granted");
    mockAuditLog.mockResolvedValue({
      entries: [
        { event: "skill_ingested", detail: "count=3", occurred_at: new Date().toISOString() },
        { event: "consent_granted", detail: null, occurred_at: new Date().toISOString() },
      ],
    });
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText("skill_ingested")).toBeInTheDocument();
      expect(screen.getByText("consent_granted")).toBeInTheDocument();
    });
  });

  it("displays local-only privacy note", async () => {
    mockStatus.mockResolvedValue("granted");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText(/stored locally on your device/i)).toBeInTheDocument();
    });
  });
});
