import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import ConsentControls from "../ConsentControls";

vi.mock("../../ipc", () => ({
  getConsentStatus: vi.fn(),
  pauseConsent: vi.fn(),
  resumeConsent: vi.fn(),
}));

import { getConsentStatus, pauseConsent, resumeConsent } from "../../ipc";
const mockStatus = vi.mocked(getConsentStatus);
const mockPause = vi.mocked(pauseConsent);
const mockResume = vi.mocked(resumeConsent);

beforeEach(() => {
  vi.clearAllMocks();
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

  it("displays local-only privacy note", async () => {
    mockStatus.mockResolvedValue("granted");
    render(<ConsentControls />);
    await waitFor(() => {
      expect(screen.getByText(/stored locally on your device/i)).toBeInTheDocument();
    });
  });
});
