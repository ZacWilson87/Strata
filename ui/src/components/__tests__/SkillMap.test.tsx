import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import SkillMap from "../SkillMap";
import type { SkillsResponse } from "../../types";

// Mock the IPC layer so tests don't need a Tauri backend
vi.mock("../../ipc", () => ({
  getSkills: vi.fn(),
}));

import { getSkills } from "../../ipc";
const mockGetSkills = vi.mocked(getSkills);

const MOCK_DATA: SkillsResponse = {
  summary: "rust, async",
  skills: [
    { id: "1", tag: "rust", strength: 10, last_seen: "2026-04-18T00:00:00Z", session_count: 10 },
    { id: "2", tag: "async", strength: 6, last_seen: "2026-04-18T00:00:00Z", session_count: 6 },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("SkillMap", () => {
  it("renders loading state initially", () => {
    mockGetSkills.mockReturnValue(new Promise(() => {})); // never resolves
    render(<SkillMap />);
    expect(screen.getByText(/loading skills/i)).toBeInTheDocument();
  });

  it("renders skill cards after data loads", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText("rust")).toBeInTheDocument();
      expect(screen.getByText("async")).toBeInTheDocument();
    });
  });

  it("renders summary text", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText("rust, async")).toBeInTheDocument();
    });
  });

  it("renders session counts", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText("10 sessions")).toBeInTheDocument();
      expect(screen.getByText("6 sessions")).toBeInTheDocument();
    });
  });

  it("shows error state when fetch fails", async () => {
    mockGetSkills.mockRejectedValue(new Error("network error"));
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText(/error/i)).toBeInTheDocument();
    });
  });

  it("does not render any raw content", async () => {
    mockGetSkills.mockResolvedValue({
      summary: "rust",
      skills: [{ id: "1", tag: "rust", strength: 1, last_seen: "", session_count: 1 }],
    });
    const { container } = render(<SkillMap />);
    await waitFor(() => {
      expect(container.innerHTML).not.toContain("RawSignal");
      expect(container.innerHTML).not.toContain("prompt");
    });
  });
});
