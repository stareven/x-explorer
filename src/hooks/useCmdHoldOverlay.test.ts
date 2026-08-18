import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useCmdHoldOverlay } from "./useCmdHoldOverlay";

function dispatchKeyDown(key: string) {
  window.dispatchEvent(new KeyboardEvent("keydown", { key }));
}

function dispatchKeyUp(key: string) {
  window.dispatchEvent(new KeyboardEvent("keyup", { key }));
}

function dispatchBlur() {
  window.dispatchEvent(new Event("blur"));
}

describe("useCmdHoldOverlay", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("becomes visible after holding Meta for 600ms", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    expect(result.current).toBe(false);

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });

    expect(result.current).toBe(true);
  });

  it("does not become visible before 600ms elapses", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(599);
    });

    expect(result.current).toBe(false);
  });

  it("releasing Meta before 600ms cancels the overlay", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(300);
      dispatchKeyUp("Meta");
      vi.advanceTimersByTime(300);
    });

    expect(result.current).toBe(false);
  });

  it("releasing Meta after the overlay is visible hides it immediately", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(true);

    act(() => {
      dispatchKeyUp("Meta");
    });

    expect(result.current).toBe(false);
  });

  it("pressing a non-Meta key while waiting cancels the pending timer", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(300);
      dispatchKeyDown("b");
      vi.advanceTimersByTime(300);
    });

    expect(result.current).toBe(false);
  });

  it("pressing a non-Meta key while overlay is visible hides it immediately", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(true);

    act(() => {
      dispatchKeyDown("b");
    });

    expect(result.current).toBe(false);
  });

  it("window blur resets pending timer and visible state", () => {
    const { result: pendingResult } = renderHook(() => useCmdHoldOverlay());
    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(300);
      dispatchBlur();
      vi.advanceTimersByTime(300);
    });
    expect(pendingResult.current).toBe(false);

    const { result: visibleResult } = renderHook(() => useCmdHoldOverlay());
    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(visibleResult.current).toBe(true);

    act(() => {
      dispatchBlur();
    });
    expect(visibleResult.current).toBe(false);
  });

  it("cleans up listeners on unmount", () => {
    const addSpy = vi.spyOn(window, "addEventListener");
    const removeSpy = vi.spyOn(window, "removeEventListener");

    const { unmount } = renderHook(() => useCmdHoldOverlay());
    const addedEvents = addSpy.mock.calls.map(([type]) => type);
    expect(addedEvents).toEqual(expect.arrayContaining(["keydown", "keyup", "blur"]));

    unmount();
    const removedEvents = removeSpy.mock.calls.map(([type]) => type);
    expect(removedEvents).toEqual(expect.arrayContaining(["keydown", "keyup", "blur"]));

    addSpy.mockRestore();
    removeSpy.mockRestore();
  });
});
