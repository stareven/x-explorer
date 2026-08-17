import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { Bookmarks } from "./Bookmarks";
import { useStore } from "../../store";

const removeBookmark = vi.fn();
const openBookmark = vi.fn();

beforeEach(() => {
  removeBookmark.mockClear();
  openBookmark.mockClear();
  useStore.setState({
    bookmarks: [
      {
        platform: "ios",
        app: { name: "Documents", bundle_id: "com.example.documents" },
        path: "/Downloads",
      },
    ],
    devices: [],
    selectedDeviceId: null,
    removeBookmark,
    openBookmark,
  });
});

describe("Bookmarks", () => {
  it("shows the full path in the hover title", () => {
    render(<Bookmarks />);

    expect(screen.getByText("Documents")).toBeInTheDocument();
    expect(screen.getByText(/com\.example\.documents/)).toBeInTheDocument();
    expect(screen.getByTitle("com.example.documents/Downloads")).toBeInTheDocument();
  });
});
