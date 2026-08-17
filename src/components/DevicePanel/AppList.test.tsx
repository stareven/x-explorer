import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { AppList } from "./AppList";
import { useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

vi.mock("../../hooks/useTauri", () => ({
  tauriApi: {
    listIosApps: vi.fn(),
  },
}));

describe("AppList", () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listIosApps).mockResolvedValue([
      { name: "Documents", bundle_id: "com.example.documents" },
    ]);
    useStore.setState({
      devices: [{ id: "iphone-1", name: "iPhone", platform: "ios", status: "connected" }],
      selectedDeviceId: "iphone-1",
      browseTarget: null,
      favoriteAppIds: [],
    });
  });

  it("shows bundle id for iPhone apps", async () => {
    render(<AppList />);

    expect(await screen.findByText("Documents")).toBeInTheDocument();
    expect(screen.getByText("com.example.documents")).toBeInTheDocument();
  });
});
