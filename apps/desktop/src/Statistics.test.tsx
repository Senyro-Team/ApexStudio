import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Statistics from "./Statistics";

afterEach(cleanup);
it("shows public totals and post details without links to post pages", async () => {
  const post = {
    bundleId: 123,
    title: "Test livery",
    views: 45,
    likes: 3,
    copies: 4,
    favourites: 2,
    ratingCount: 0,
    ratingAverage: null,
  };
  const stats = {
    profile: "2919579206",
    history: [{ collectedAt: "2026-10-01T12:00:00Z", posts: [post] }],
    error: "",
    busy: false,
    sales: null,
    salesBusy: false,
    saveProfile: vi.fn(),
    refresh: vi.fn(),
    refreshSales: vi.fn(),
  };
  render(<Statistics stats={stats} onSettings={() => {}} />);
  expect(screen.getAllByText("45").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Test livery").length).toBeGreaterThan(0);
  expect(screen.queryAllByRole("link")).toHaveLength(0);
  await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
  expect(stats.refresh).toHaveBeenCalledOnce();
});

it("does not present missing counters as zero", () => {
  const post = {
    bundleId: 1,
    title: "Example",
    views: null,
    likes: 0,
    copies: 0,
    favourites: 0,
    ratingCount: 0,
    ratingAverage: null,
  };
  const stats = {
    profile: "1",
    history: [{ collectedAt: "2026-10-01T00:00:00Z", posts: [post] }],
    error: "",
    busy: false,
    sales: null,
    salesBusy: false,
    saveProfile: vi.fn(),
    refresh: vi.fn(),
    refreshSales: vi.fn(),
  };
  render(<Statistics stats={stats} onSettings={() => {}} />);
  expect(screen.getAllByText("—").length).toBeGreaterThan(0);
});
