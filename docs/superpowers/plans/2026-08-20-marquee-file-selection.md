# Marquee File Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mouse marquee (rubber-band rectangle) selection to the FileBrowser, replacing the current selection with the entries intersecting the dragged rectangle.

**Architecture:** A new `useMarqueeSelection` hook tracks `mousedown` on the scroll container plus window-level `mousemove`/`mouseup`, exposes the current rectangle for rendering an overlay, and on release collects intersecting `[data-file-entry]` elements via `getBoundingClientRect()` and replaces the selection through a new `selectMany` helper on the existing `useSelection` hook.

**Tech Stack:** React + TypeScript + Zustand, Vitest + @testing-library/react (jsdom), Tailwind CSS.

---

## File Structure

- **Create:** `src/components/FileBrowser/useMarqueeSelection.ts` — pure geometry helpers (`normalizeRect`, `rectsIntersect`, `collectBoxSelection`) + the `useMarqueeSelection` hook.
- **Create:** `src/components/FileBrowser/useMarqueeSelection.test.tsx` — tests for the pure helpers and the hook.
- **Modify:** `src/components/FileBrowser/useSelection.ts` — add `selectMany(names: string[])`.
- **Modify:** `src/components/FileBrowser/useSelection.test.ts` — test `selectMany`.
- **Modify:** `src/components/FileBrowser/FileList.tsx` — add `data-file-name` to each row.
- **Modify:** `src/components/FileBrowser/FileGrid.tsx` — add `data-file-name` to each item.
- **Modify:** `src/components/FileBrowser/FileViews.test.tsx` — test `data-file-name` on list + grid.
- **Modify:** `src/components/FileBrowser/index.tsx` — wire the hook, render the overlay.
- **Modify:** `src/components/FileBrowser/FileBrowser.test.tsx` — integration test.

---

### Task 1: Pure geometry/collection helpers

**Files:**
- Create: `src/components/FileBrowser/useMarqueeSelection.ts`
- Create: `src/components/FileBrowser/useMarqueeSelection.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `src/components/FileBrowser/useMarqueeSelection.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { normalizeRect, rectsIntersect, collectBoxSelection, type MarqueeRect } from "./useMarqueeSelection";

function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

describe("normalizeRect", () => {
  it("normalizes a reversed drag into top-left origin", () => {
    expect(normalizeRect(100, 80, 20, 10)).toEqual({ left: 20, top: 10, width: 80, height: 70 });
  });
});

describe("rectsIntersect", () => {
  const marquee = (l: number, t: number, w: number, h: number): MarqueeRect => ({ left: l, top: t, width: w, height: h });

  it("detects overlap", () => {
    expect(rectsIntersect(domRect(10, 10, 20, 20), marquee(20, 20, 20, 20))).toBe(true);
  });

  it("counts touching edges as intersecting", () => {
    expect(rectsIntersect(domRect(0, 0, 20, 20), marquee(20, 20, 10, 10))).toBe(true);
  });

  it("detects disjoint rects", () => {
    expect(rectsIntersect(domRect(0, 0, 10, 10), marquee(50, 50, 10, 10))).toBe(false);
  });

  it("detects containment", () => {
    expect(rectsIntersect(domRect(5, 5, 2, 2), marquee(0, 0, 10, 10))).toBe(true);
  });
});

describe("collectBoxSelection", () => {
  it("returns file names whose entry rects intersect the marquee", () => {
    const container = document.createElement("div");

    const a = document.createElement("div");
    a.setAttribute("data-file-entry", "");
    a.setAttribute("data-file-name", "a.txt");
    a.getBoundingClientRect = () => domRect(0, 0, 100, 20);

    const b = document.createElement("div");
    b.setAttribute("data-file-entry", "");
    b.setAttribute("data-file-name", "b.txt");
    b.getBoundingClientRect = () => domRect(0, 40, 100, 20);

    container.append(a, b);

    expect(collectBoxSelection(container, { left: 0, top: 0, width: 100, height: 25 })).toEqual(["a.txt"]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/FileBrowser/useMarqueeSelection.test.tsx`
Expected: FAIL — `Cannot find module './useMarqueeSelection'` (file does not exist yet).

- [ ] **Step 3: Write minimal implementation**

Create `src/components/FileBrowser/useMarqueeSelection.ts` with just the pure helpers (the hook is added in Task 4):

```ts
export interface MarqueeRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function normalizeRect(x1: number, y1: number, x2: number, y2: number): MarqueeRect {
  return {
    left: Math.min(x1, x2),
    top: Math.min(y1, y2),
    width: Math.abs(x2 - x1),
    height: Math.abs(y2 - y1),
  };
}

export function rectsIntersect(a: DOMRect, b: MarqueeRect): boolean {
  return (
    a.left <= b.left + b.width &&
    a.left + a.width >= b.left &&
    a.top <= b.top + b.height &&
    a.top + a.height >= b.top
  );
}

export function collectBoxSelection(container: HTMLElement, rect: MarqueeRect): string[] {
  const names: string[] = [];
  container.querySelectorAll<HTMLElement>("[data-file-entry]").forEach((el) => {
    const name = el.dataset.fileName;
    if (name != null && rectsIntersect(el.getBoundingClientRect(), rect)) {
      names.push(name);
    }
  });
  return names;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/FileBrowser/useMarqueeSelection.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/components/FileBrowser/useMarqueeSelection.ts src/components/FileBrowser/useMarqueeSelection.test.tsx
git commit -m "feat(marquee): add rect intersection and box-selection helpers"
```

---

### Task 2: `selectMany` on `useSelection`

**Files:**
- Modify: `src/components/FileBrowser/useSelection.ts`
- Test: `src/components/FileBrowser/useSelection.test.ts`

- [ ] **Step 1: Write the failing test**

Append to `src/components/FileBrowser/useSelection.test.ts` (inside the existing `describe("useSelection", ...)`):

```ts
it("replaces selection with a set via selectMany()", () => {
  const { result } = renderHook(() => useSelection(items));
  act(() => result.current.handleClick("a.txt", false, false));
  act(() => result.current.selectMany(["b.txt", "c.txt"]));
  expect(result.current.selected).toEqual(new Set(["b.txt", "c.txt"]));
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/FileBrowser/useSelection.test.ts`
Expected: FAIL — TypeScript/type error: `selectMany` does not exist on the returned object.

- [ ] **Step 3: Write minimal implementation**

In `src/components/FileBrowser/useSelection.ts`, add the function (e.g. after `selectAll`) and include it in the return object:

```ts
function selectMany(names: string[]) {
  setSelected(new Set(names));
  setLastClicked(null);
}
```

Change the return statement to:

```ts
return { selected, handleClick, selectOnly, selectAll, clearSelection, selectMany };
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/FileBrowser/useSelection.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/components/FileBrowser/useSelection.ts src/components/FileBrowser/useSelection.test.ts
git commit -m "feat(marquee): add selectMany to useSelection"
```

---

### Task 3: `data-file-name` on list + grid entries

**Files:**
- Modify: `src/components/FileBrowser/FileList.tsx`
- Modify: `src/components/FileBrowser/FileGrid.tsx`
- Test: `src/components/FileBrowser/FileViews.test.tsx`

- [ ] **Step 1: Write the failing test**

In `src/components/FileBrowser/FileViews.test.tsx`, add `FileGrid` to the imports:

```tsx
import { FileGrid } from "./FileGrid";
```

Append these two tests (each in the appropriate `describe` block — add a new `describe("FileGrid", ...)` block for the second):

```tsx
it("renders data-file-name on each row", () => {
  render(
    <FileList
      files={mockFiles}
      selected={new Set()}
      onNavigate={vi.fn()}
      onSelect={vi.fn()}
      onContextMenu={vi.fn()}
    />
  );
  const rows = document.querySelectorAll("[data-file-entry]");
  expect(rows).toHaveLength(2);
  expect(rows[0]).toHaveAttribute("data-file-name", "Documents");
  expect(rows[1]).toHaveAttribute("data-file-name", "config.json");
});

describe("FileGrid", () => {
  it("renders data-file-name on each item", () => {
    render(
      <FileGrid
        files={mockFiles}
        selected={new Set()}
        onNavigate={vi.fn()}
        onSelect={vi.fn()}
        onContextMenu={vi.fn()}
      />
    );
    const items = document.querySelectorAll("[data-file-entry]");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveAttribute("data-file-name", "Documents");
    expect(items[1]).toHaveAttribute("data-file-name", "config.json");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/FileBrowser/FileViews.test.tsx`
Expected: FAIL — the two `toHaveAttribute("data-file-name", ...)` assertions fail (attribute missing).

- [ ] **Step 3: Write minimal implementation**

In `src/components/FileBrowser/FileList.tsx`, add the attribute to the `<tr>`:

```tsx
<tr
  key={file.path}
  data-file-entry
  data-file-name={file.name}
  onContextMenu={(e) => onContextMenu(file.name, e)}
  onClick={(e) => onSelect(file.name, e.metaKey, e.shiftKey)}
  onDoubleClick={() => file.is_dir && onNavigate(file.path)}
  className={`cursor-pointer hover:bg-gray-700 ${
    selected.has(file.name) ? "bg-blue-900" : ""
  }`}
>
```

In `src/components/FileBrowser/FileGrid.tsx`, add the attribute to the `<div>`:

```tsx
<div
  key={file.path}
  data-file-entry
  data-file-name={file.name}
  onContextMenu={(e) => onContextMenu(file.name, e)}
  onClick={(e) => onSelect(file.name, e.metaKey, e.shiftKey)}
  onDoubleClick={() => file.is_dir && onNavigate(file.path)}
  className={`flex flex-col items-center gap-1 p-2 rounded cursor-pointer text-center hover:bg-gray-700 ${
    selected.has(file.name) ? "bg-blue-900" : ""
  }`}
>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/FileBrowser/FileViews.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/FileBrowser/FileList.tsx src/components/FileBrowser/FileGrid.tsx src/components/FileBrowser/FileViews.test.tsx
git commit -m "feat(marquee): tag file entries with data-file-name"
```

---

### Task 4: `useMarqueeSelection` hook

**Files:**
- Modify: `src/components/FileBrowser/useMarqueeSelection.ts`
- Test: `src/components/FileBrowser/useMarqueeSelection.test.tsx`

- [ ] **Step 1: Write the failing test**

Add to the top of `src/components/FileBrowser/useMarqueeSelection.test.tsx`:

```tsx
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useMarqueeSelection } from "./useMarqueeSelection";
```

Append the hook tests (plus a harness) to the same file:

```tsx
function Harness({ onBoxSelect }: { onBoxSelect: (names: string[]) => void }) {
  const { marquee, onMouseDown } = useMarqueeSelection(onBoxSelect);
  return (
    <div data-testid="container" onMouseDown={onMouseDown}>
      <div data-file-entry data-file-name="a.txt" />
      <div data-file-entry data-file-name="b.txt" />
      {marquee && <div data-testid="marquee-overlay" />}
    </div>
  );
}

describe("useMarqueeSelection", () => {
  it("selects entries intersecting the dragged rectangle", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const entries = document.querySelectorAll<HTMLElement>("[data-file-entry]");
    entries[0].getBoundingClientRect = () => domRect(0, 0, 100, 20);
    entries[1].getBoundingClientRect = () => domRect(0, 40, 100, 20);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 0, clientY: 0 });
    fireEvent.mouseMove(window, { clientX: 100, clientY: 25 });
    expect(screen.getByTestId("marquee-overlay")).toBeInTheDocument();

    fireEvent.mouseUp(window, { clientX: 100, clientY: 25 });
    expect(onBoxSelect).toHaveBeenCalledWith(["a.txt"]);
    expect(screen.queryByTestId("marquee-overlay")).not.toBeInTheDocument();
  });

  it("does not trigger selection on a clean click", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.mouseUp(window, { clientX: 10, clientY: 10 });

    expect(onBoxSelect).not.toHaveBeenCalled();
  });

  it("ignores sub-threshold movement", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.mouseMove(window, { clientX: 12, clientY: 10 });
    fireEvent.mouseUp(window, { clientX: 12, clientY: 10 });

    expect(onBoxSelect).not.toHaveBeenCalled();
  });
});
```

Also add `vi` to the vitest import at the top of the file:

```tsx
import { describe, it, expect, vi } from "vitest";
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/FileBrowser/useMarqueeSelection.test.tsx`
Expected: FAIL — `useMarqueeSelection` is not exported.

- [ ] **Step 3: Write minimal implementation**

Append to `src/components/FileBrowser/useMarqueeSelection.ts`:

```ts
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";

const DRAG_THRESHOLD_PX = 4;

export function useMarqueeSelection(onBoxSelect: (names: string[]) => void) {
  const containerRef = useRef<HTMLElement | null>(null);
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const draggingRef = useRef(false);
  const [marquee, setMarquee] = useState<MarqueeRect | null>(null);
  const onBoxSelectRef = useRef(onBoxSelect);
  onBoxSelectRef.current = onBoxSelect;

  function stopTracking() {
    window.removeEventListener("mousemove", handleMouseMove);
    window.removeEventListener("mouseup", handleMouseUp);
    startRef.current = null;
    draggingRef.current = false;
  }

  function handleMouseMove(e: MouseEvent) {
    const start = startRef.current;
    if (!start) return;
    const dx = e.clientX - start.x;
    const dy = e.clientY - start.y;
    if (!draggingRef.current && Math.max(Math.abs(dx), Math.abs(dy)) <= DRAG_THRESHOLD_PX) return;
    draggingRef.current = true;
    setMarquee(normalizeRect(start.x, start.y, e.clientX, e.clientY));
  }

  function handleMouseUp(e: MouseEvent) {
    const start = startRef.current;
    if (draggingRef.current && start && containerRef.current) {
      onBoxSelectRef.current(
        collectBoxSelection(containerRef.current, normalizeRect(start.x, start.y, e.clientX, e.clientY)),
      );
    }
    stopTracking();
    setMarquee(null);
  }

  function onMouseDown(e: ReactMouseEvent<HTMLElement>) {
    if (e.button !== 0) return;
    containerRef.current = e.currentTarget;
    startRef.current = { x: e.clientX, y: e.clientY };
    draggingRef.current = false;
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  }

  // The handlers only read refs (never props/state), so the initial render's
  // closures stay valid for the hook's lifetime — empty deps are intentional.
  useEffect(() => {
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  return { marquee, onMouseDown };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/components/FileBrowser/useMarqueeSelection.test.tsx`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add src/components/FileBrowser/useMarqueeSelection.ts src/components/FileBrowser/useMarqueeSelection.test.tsx
git commit -m "feat(marquee): add useMarqueeSelection hook"
```

---

### Task 5: Wire into FileBrowser + integration test

**Files:**
- Modify: `src/components/FileBrowser/index.tsx`
- Test: `src/components/FileBrowser/FileBrowser.test.tsx`

- [ ] **Step 1: Write the failing test**

In `src/components/FileBrowser/FileBrowser.test.tsx`, add a `domRect` helper (near the top, after the `app` const):

```ts
function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}
```

Append a new test inside the `describe("FileBrowser search", ...)` block (or a new `describe("FileBrowser marquee", ...)` block):

```tsx
it("marquee selects entries intersecting the dragged rectangle", async () => {
  render(<FileBrowser />);
  await waitFor(() => expect(screen.getByText("Alpha.txt")).toBeInTheDocument());

  const entries = document.querySelectorAll<HTMLElement>("[data-file-entry]");
  entries.forEach((el, i) => {
    el.getBoundingClientRect = () => domRect(0, i * 30, 100, 20);
  });

  const container = screen.getByLabelText("文件浏览区域");
  fireEvent.mouseDown(container, { button: 0, clientX: 0, clientY: 0 });
  fireEvent.mouseMove(window, { clientX: 100, clientY: 55 });
  expect(screen.getByTestId("marquee-overlay")).toBeInTheDocument();
  fireEvent.mouseUp(window, { clientX: 100, clientY: 55 });

  // Rows 0 (0-20) and 1 (30-50) intersect; rows 2 (60-80) and 3 (90-110) do not.
  await waitFor(() => expect(screen.getByText("导出 (2)")).toBeInTheDocument());
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/components/FileBrowser/FileBrowser.test.tsx`
Expected: FAIL — the `marquee-overlay` test id is never rendered (overlay not wired yet).

- [ ] **Step 3: Write minimal implementation**

In `src/components/FileBrowser/index.tsx`:

1. Add the import (with the other FileBrowser imports):

```ts
import { useMarqueeSelection } from "./useMarqueeSelection";
```

2. Change the `useSelection` destructuring (currently `const { selected, handleClick, selectOnly, selectAll, clearSelection } = useSelection(visibleFileNames);`) to include `selectMany`, and call the marquee hook right after:

```ts
const { selected, handleClick, selectOnly, selectAll, clearSelection, selectMany } = useSelection(visibleFileNames);
const { marquee, onMouseDown: onMarqueeMouseDown } = useMarqueeSelection(selectMany);
```

3. Bind `onMouseDown` to the scroll container (the div with `aria-label="文件浏览区域"`):

```tsx
<div
  className="flex-1 overflow-auto select-none"
  aria-label="文件浏览区域"
  onMouseDown={onMarqueeMouseDown}
  onContextMenu={handleBackgroundContextMenu}
>
```

4. Render the overlay at the end of the returned tree, just before the final closing `</div>` (after the `{showGoToPath && ...}` block):

```tsx
{marquee && (
  <div
    data-testid="marquee-overlay"
    style={{
      position: "fixed",
      left: marquee.left,
      top: marquee.top,
      width: marquee.width,
      height: marquee.height,
      pointerEvents: "none",
      zIndex: 50,
      border: "1px solid rgb(59 130 246)",
      background: "rgba(59 130 246, 0.15)",
    }}
  />
)}
```

- [ ] **Step 4: Run tests and type check**

Run: `npx vitest run src/components/FileBrowser`
Expected: all FileBrowser tests PASS.

Run: `npm run build`
Expected: succeeds with no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/FileBrowser/index.tsx src/components/FileBrowser/FileBrowser.test.tsx
git commit -m "feat(marquee): wire marquee selection into FileBrowser"
```

---

## Self-Review Notes

- **Spec coverage:** interaction behavior → Task 4/5; replace-selection semantics → Task 2 (`selectMany`) + Task 5 wiring; 4px threshold → Task 4 (`DRAG_THRESHOLD_PX`); list + grid → Task 3; empty-box clears selection → `collectBoxSelection` returns `[]` → `selectMany([])`; left-button only → Task 4 (`e.button !== 0` guard); overlay styling → Task 5; window-level release → Task 4 (window `mouseup`). All spec items map to a task.
- **Type consistency:** `MarqueeRect`, `normalizeRect`, `rectsIntersect`, `collectBoxSelection`, `useMarqueeSelection`, `selectMany` names are used consistently across tasks and match the spec.
- **Placeholders:** none — every code step includes full source.
