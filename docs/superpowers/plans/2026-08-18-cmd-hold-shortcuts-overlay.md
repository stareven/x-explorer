# Cmd Hold Shortcuts Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a global "hold Cmd for 800ms" overlay that displays all FileBrowser keyboard shortcuts in a centered, semi-transparent card, and hides immediately when Cmd is released.

**Architecture:** A new `FILE_BROWSER_SHORTCUTS` data table in `shortcuts.ts` becomes the single source of truth for shortcut display strings/descriptions. A new `useCmdHoldOverlay` hook (window-level `keydown`/`keyup`/`blur` listeners + a timer) tracks whether the overlay should be visible, independent of any focus/editable-target checks. A new `ShortcutsOverlay` presentational component renders the overlay when told to. `App.tsx` wires the hook's boolean into the overlay component at the top level so it works globally, not just inside `FileBrowser`.

**Tech Stack:** React 19, TypeScript, Vitest, React Testing Library, `@testing-library/react`'s `renderHook`, Vitest fake timers.

---

## File Map

- `src/components/FileBrowser/shortcuts.ts`: add `FILE_BROWSER_SHORTCUTS` data table (no behavior change to existing exports).
- `src/hooks/useCmdHoldOverlay.ts` (new): hook that returns `boolean` — whether the overlay should be visible.
- `src/hooks/useCmdHoldOverlay.test.ts` (new): covers timer/keyup/blur/interrupt behavior.
- `src/components/ShortcutsOverlay.tsx` (new): presentational overlay component.
- `src/components/ShortcutsOverlay.test.tsx` (new): covers rendering when visible/hidden.
- `src/App.tsx`: wire the hook and render the overlay at the top level.

---

### Task 1: Add the shortcut display data table

**Files:**
- Modify: `src/components/FileBrowser/shortcuts.ts`

- [ ] **Step 1: Add the `FILE_BROWSER_SHORTCUTS` constant**

Append this to the end of `src/components/FileBrowser/shortcuts.ts` (after the existing `getFileBrowserShortcutAction` function, keeping all existing code unchanged):

```ts
export interface FileBrowserShortcutInfo {
  action: FileBrowserShortcutAction;
  keys: string;
  description: string;
}

export const FILE_BROWSER_SHORTCUTS: FileBrowserShortcutInfo[] = [
  { action: "back", keys: "⌘[", description: "后退" },
  { action: "forward", keys: "⌘]", description: "前进" },
  { action: "up", keys: "⌘↑", description: "上级目录" },
  { action: "bookmark", keys: "⌘B", description: "收藏 / 取消收藏" },
  { action: "upload", keys: "⌘U", description: "上传" },
  { action: "download", keys: "⌘S", description: "下载" },
  { action: "delete", keys: "⌘⌫", description: "删除" },
  { action: "select-all", keys: "⌘A", description: "全选" },
  { action: "goto", keys: "⌘G", description: "跳转到目录" },
];
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npx tsc --noEmit -p /Users/hongqize/Workspace/x-explorer`

Expected: no errors related to `shortcuts.ts`.

- [ ] **Step 3: Commit**

```bash
cd /Users/hongqize/Workspace/x-explorer
git add src/components/FileBrowser/shortcuts.ts
git commit -m "feat: add shortcut display data table for overlay"
```

---

### Task 2: Implement `useCmdHoldOverlay` hook (TDD)

**Files:**
- Create: `src/hooks/useCmdHoldOverlay.test.ts`
- Create: `src/hooks/useCmdHoldOverlay.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/hooks/useCmdHoldOverlay.test.ts`:

```ts
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

  it("becomes visible after holding Meta for 800ms", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    expect(result.current).toBe(false);

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(800);
    });

    expect(result.current).toBe(true);
  });

  it("does not become visible before 800ms elapses", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(799);
    });

    expect(result.current).toBe(false);
  });

  it("releasing Meta before 800ms cancels the overlay", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(400);
      dispatchKeyUp("Meta");
      vi.advanceTimersByTime(400);
    });

    expect(result.current).toBe(false);
  });

  it("releasing Meta after the overlay is visible hides it immediately", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(800);
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
      vi.advanceTimersByTime(400);
      dispatchKeyDown("b");
      vi.advanceTimersByTime(400);
    });

    expect(result.current).toBe(false);
  });

  it("pressing a non-Meta key while overlay is visible hides it immediately", () => {
    const { result } = renderHook(() => useCmdHoldOverlay());

    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(800);
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
      vi.advanceTimersByTime(400);
      dispatchBlur();
      vi.advanceTimersByTime(400);
    });
    expect(pendingResult.current).toBe(false);

    const { result: visibleResult } = renderHook(() => useCmdHoldOverlay());
    act(() => {
      dispatchKeyDown("Meta");
      vi.advanceTimersByTime(800);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx vitest run src/hooks/useCmdHoldOverlay.test.ts`

Expected: FAIL — `Cannot find module './useCmdHoldOverlay'` (file doesn't exist yet).

- [ ] **Step 3: Implement the hook**

Create `src/hooks/useCmdHoldOverlay.ts`:

```ts
import { useEffect, useRef, useState } from "react";

const HOLD_DURATION_MS = 800;

export function useCmdHoldOverlay(): boolean {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    function clearPendingTimer() {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Meta") {
        if (timerRef.current !== null) return;
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          setVisible(true);
        }, HOLD_DURATION_MS);
        return;
      }

      clearPendingTimer();
      setVisible(false);
    }

    function handleKeyUp(event: KeyboardEvent) {
      if (event.key !== "Meta") return;
      clearPendingTimer();
      setVisible(false);
    }

    function handleBlur() {
      clearPendingTimer();
      setVisible(false);
    }

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);

    return () => {
      clearPendingTimer();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  return visible;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx vitest run src/hooks/useCmdHoldOverlay.test.ts`

Expected: PASS — all 8 tests green.

- [ ] **Step 5: Commit**

```bash
cd /Users/hongqize/Workspace/x-explorer
git add src/hooks/useCmdHoldOverlay.ts src/hooks/useCmdHoldOverlay.test.ts
git commit -m "feat: add useCmdHoldOverlay hook for long-press Cmd detection"
```

---

### Task 3: Implement `ShortcutsOverlay` component (TDD)

**Files:**
- Create: `src/components/ShortcutsOverlay.test.tsx`
- Create: `src/components/ShortcutsOverlay.tsx`

- [ ] **Step 1: Write the failing tests**

Create `src/components/ShortcutsOverlay.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ShortcutsOverlay } from "./ShortcutsOverlay";
import { FILE_BROWSER_SHORTCUTS } from "./FileBrowser/shortcuts";

describe("ShortcutsOverlay", () => {
  it("renders nothing when not visible", () => {
    render(<ShortcutsOverlay visible={false} />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders all shortcuts when visible", () => {
    render(<ShortcutsOverlay visible={true} />);

    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeTruthy();

    for (const shortcut of FILE_BROWSER_SHORTCUTS) {
      expect(screen.getByText(shortcut.keys)).toBeTruthy();
      expect(screen.getByText(shortcut.description)).toBeTruthy();
    }
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx vitest run src/components/ShortcutsOverlay.test.tsx`

Expected: FAIL — `Cannot find module './ShortcutsOverlay'`.

- [ ] **Step 3: Implement the component**

Create `src/components/ShortcutsOverlay.tsx`:

```tsx
import { FILE_BROWSER_SHORTCUTS } from "./FileBrowser/shortcuts";

type ShortcutsOverlayProps = {
  visible: boolean;
};

export function ShortcutsOverlay({ visible }: ShortcutsOverlayProps) {
  if (!visible) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-label="快捷键说明"
        className="w-80 rounded border border-gray-700 bg-gray-900 p-4 shadow-lg"
      >
        <h2 className="text-xs font-semibold text-gray-400 mb-3">快捷键</h2>
        <ul className="space-y-2">
          {FILE_BROWSER_SHORTCUTS.map((shortcut) => (
            <li key={shortcut.action} className="flex items-center justify-between gap-3">
              <span className="rounded border border-gray-600 bg-gray-800 px-1.5 py-0.5 text-xs font-mono text-gray-200">
                {shortcut.keys}
              </span>
              <span className="text-sm text-gray-300">{shortcut.description}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx vitest run src/components/ShortcutsOverlay.test.tsx`

Expected: PASS — both tests green.

- [ ] **Step 5: Commit**

```bash
cd /Users/hongqize/Workspace/x-explorer
git add src/components/ShortcutsOverlay.tsx src/components/ShortcutsOverlay.test.tsx
git commit -m "feat: add ShortcutsOverlay component"
```

---

### Task 4: Wire the overlay into `App.tsx`

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Add the hook import and overlay import**

In `src/App.tsx`, update the top imports (currently lines 1-7) to add two new imports. The full new import block:

```ts
import { useEffect, useState } from "react";
import { DevicePanel } from "./components/DevicePanel";
import { FileBrowser } from "./components/FileBrowser";
import { TransferPanel } from "./components/TransferPanel";
import { ShortcutsOverlay } from "./components/ShortcutsOverlay";
import { useDeviceListener } from "./hooks/useTauri";
import { tauriApi } from "./hooks/useTauri";
import { useStore } from "./store";
import { useCmdHoldOverlay } from "./hooks/useCmdHoldOverlay";
```

- [ ] **Step 2: Call the hook and render the overlay**

Inside the `App` function body, add the hook call right after `useDeviceListener();` (currently line 10):

```ts
export default function App() {
  useDeviceListener();
  const showShortcutsOverlay = useCmdHoldOverlay();
```

Then update the returned JSX (currently lines 35-51) to render `ShortcutsOverlay` as a sibling of the root `<div>`'s children, right after the closing `</div>` of the `flex flex-1` row and before `<TransferPanel />`, so the final return block looks like:

```tsx
  return (
    <div className="flex flex-col h-screen bg-gray-900 text-white overflow-hidden">
      {loadError && (
        <div className="px-3 py-2 bg-red-900 text-red-200 text-xs flex items-center justify-between">
          <span>{loadError}</span>
          <button onClick={() => setLoadError(null)} className="text-red-300 hover:text-white">
            ✕
          </button>
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <DevicePanel />
        <FileBrowser />
      </div>
      <TransferPanel />
      <ShortcutsOverlay visible={showShortcutsOverlay} />
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx tsc --noEmit`

Expected: no errors.

- [ ] **Step 4: Run the full frontend test suite**

Run: `cd /Users/hongqize/Workspace/x-explorer && npx vitest run`

Expected: all tests pass, including the new `useCmdHoldOverlay.test.ts` and `ShortcutsOverlay.test.tsx`.

- [ ] **Step 5: Commit**

```bash
cd /Users/hongqize/Workspace/x-explorer
git add src/App.tsx
git commit -m "feat: wire Cmd-hold shortcuts overlay into App"
```

---

## Final Verification

- [ ] Run `cd /Users/hongqize/Workspace/x-explorer && npx vitest run` — all tests pass.
- [ ] Run `cd /Users/hongqize/Workspace/x-explorer && npx tsc --noEmit` — no type errors.
- [ ] Manually verify with `npm run tauri dev`: hold Cmd for ~800ms with no other key pressed → overlay appears listing 9 shortcuts; release Cmd → overlay disappears immediately; hold Cmd then press another key (e.g. `b`) → overlay does not appear (or disappears if already visible).
