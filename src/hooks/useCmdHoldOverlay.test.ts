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

function dispatchMouseDown() {
  window.dispatchEvent(new MouseEvent("mousedown", { button: 0 }));
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

  it("pressing a non-Meta key while overlay is visible hides it eventually (via next macrotask)", async () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(true);

    act(() => {
      dispatchKeyDown("b");
    });

    // setVisible(false) is deferred to the next macrotask to avoid flushing
    // a React commit between this listener and other window-level listeners
    // (see the "defers hiding..." test for the full rationale).
    expect(result.current).toBe(true);

    await act(async () => {
      vi.runAllTimers();
    });

    expect(result.current).toBe(false);
  });

  it("mouse down while waiting cancels the pending timer", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(300);
      dispatchMouseDown();
      vi.advanceTimersByTime(300);
    });

    expect(result.current).toBe(false);
  });

  it("mouse down while overlay is visible hides it immediately", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(true);

    act(() => {
      dispatchMouseDown();
    });

    expect(result.current).toBe(false);
  });

  it("defers hiding the overlay for non-Meta keydown so other window-level listeners see the event first", async () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(600);
    });
    expect(result.current).toBe(true);

    // Pressing a non-Meta key must NOT synchronously flip the overlay off:
    // we're a window-level keydown listener that runs *before* FileBrowser's
    // window-level keydown listener, and a synchronous setVisible(false) here
    // would flush a React 18 commit whose useEffect re-registration would
    // remove FileBrowser's handler before it gets a chance to fire — silently
    // swallowing Cmd+U/Cmd+B/etc.
    act(() => {
      dispatchKeyDown("u");
    });
    expect(result.current).toBe(true);

    await act(async () => {
      vi.runAllTimers();
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
    expect(addedEvents).toEqual(expect.arrayContaining(["keydown", "keyup", "blur", "mousedown"]));

    unmount();
    const removedEvents = removeSpy.mock.calls.map(([type]) => type);
    expect(removedEvents).toEqual(expect.arrayContaining(["keydown", "keyup", "blur", "mousedown"]));

    addSpy.mockRestore();
    removeSpy.mockRestore();
  });
});
