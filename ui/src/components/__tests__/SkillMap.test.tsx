import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import SkillMap from "../SkillMap";
import type { SkillsResponse } from "../../types";

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
  work_types: { creation: 8, debugging: 4 },
  domains: [
    { tag: "rust", strength: 10, session_count: 10 },
    { tag: "mcp_protocol", strength: 4, session_count: 4 },
  ],
  tool_usage: { "claude-code": 10, cursor: 4 },
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("SkillMap", () => {
  it("renders loading state initially", () => {
    mockGetSkills.mockReturnValue(new Promise(() => {}));
    render(<SkillMap />);
    expect(screen.getByText(/loading/i)).toBeInTheDocument();
  });

  it("renders skill cards after data loads", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      // "rust" appears in both Skills and Domains; getAllByText handles multiple
      expect(screen.getAllByText("rust").length).toBeGreaterThan(0);
      expect(screen.getByText("async")).toBeInTheDocument();
    });
  });

  it("renders work type breakdown", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText("Creation")).toBeInTheDocument();
      expect(screen.getByText("Debugging")).toBeInTheDocument();
    });
  });

  it("renders domain intelligence section", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText("Domain Intelligence")).toBeInTheDocument();
      expect(screen.getByText("mcp protocol")).toBeInTheDocument();
    });
  });

  it("renders session counts", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    render(<SkillMap />);
    await waitFor(() => {
      const tens = screen.getAllByText("10 sessions");
      expect(tens.length).toBeGreaterThan(0);
    });
  });

  it("shows error state when fetch fails", async () => {
    mockGetSkills.mockRejectedValue(new Error("network error"));
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText(/error/i)).toBeInTheDocument();
    });
  });

  it("shows empty state when no skills or domains", async () => {
    mockGetSkills.mockResolvedValue({
      summary: "",
      skills: [],
      work_types: {},
      domains: [],
      tool_usage: {},
    });
    render(<SkillMap />);
    await waitFor(() => {
      expect(screen.getByText(/no sessions logged/i)).toBeInTheDocument();
    });
  });

  it("does not render any raw content", async () => {
    mockGetSkills.mockResolvedValue(MOCK_DATA);
    const { container } = render(<SkillMap />);
    await waitFor(() => {
      expect(container.innerHTML).not.toContain("RawSignal");
      expect(container.innerHTML).not.toContain("prompt");
    });
  });
});
