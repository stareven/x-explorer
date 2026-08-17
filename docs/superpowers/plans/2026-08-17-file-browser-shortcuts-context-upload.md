# File Browser Shortcuts and Blank-Area Upload Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add browser-like forward navigation, macOS file browser shortcuts, and a blank-area right-click upload menu to the file browser.

**Architecture:** Keep the change inside the existing `FileBrowser` feature instead of introducing a new global command layer. Store navigation history stays in Zustand, keyboard handling is centralized in one small browser-shortcut helper, and the upload menu is a local overlay that only appears when the file list background is right-clicked. The existing upload, download, delete, bookmark, and navigation handlers are reused so behavior stays consistent with the current toolbar and store.

**Tech Stack:** React 19, TypeScript, Zustand, Vitest, React Testing Library, Tauri dialog API.

---

## File Map

- `src/store/index.ts`: add forward navigation to the existing browser-style history state.
- `src/store/index.test.ts`: cover forward history behavior.
- `src/components/FileBrowser/Toolbar.tsx`: add the forward button beside back/up/refresh/bookmark.
- `src/components/FileBrowser/Toolbar.test.tsx`: verify the new button renders and invokes the callback.
- `src/components/FileBrowser/fileBrowserShortcuts.ts`: centralize keyboard shortcut matching and focus guards.
- `src/components/FileBrowser/fileBrowserShortcuts.test.ts`: cover all shortcut combinations.
- `src/components/FileBrowser/FileBrowserContextMenu.tsx`: render the blank-area upload menu.
- `src/components/FileBrowser/FileBrowserContextMenu.test.tsx`: verify upload menu rendering and click behavior.
- `src/components/FileBrowser/FileBrowser.test.tsx`: verify blank-area right-click opens the upload menu and file rows do not.
- `src/components/FileBrowser/index.tsx`: wire forward navigation, keyboard shortcuts, and blank-area context menu state into the browser shell.

---

### Task 1: Add forward navigation to the store and toolbar

**Files:**
- Modify: `src/store/index.ts`
- Modify: `src/store/index.test.ts`
- Modify: `src/components/FileBrowser/Toolbar.tsx`
- Modify: `src/components/FileBrowser/index.tsx`
- Create: `src/components/FileBrowser/Toolbar.test.tsx`

- [ ] **Step 1: Write the failing store test**

Add this test to `src/store/index.test.ts`:

```ts
it("moves forward after going back and stops at the end of history", () => {
  const { result } = renderHook(() => useStore());

  act(() => {
    result.current.navigate("/Documents");
    result.current.navigate("/Documents/Photos");
    result.current.goBack();
    result.current.goForward();
    result.current.goForward();
  });

  expect(result.current.currentPath).toBe("/Documents/Photos");
  expect(result.current.navIndex).toBe(2);
  expect(result.current.navHistory).toEqual(["/", "/Documents", "/Documents/Photos"]);
});
```

Run:

```bash
npx vitest run src/store/index.test.ts -v
```

Expected: FAIL with `goForward is not a function`.

- [ ] **Step 2: Implement the minimal store change**

Add a `goForward` action next to `goBack` in `src/store/index.ts`:

```ts
goForward: () =>
  set((s) => {
    if (s.navIndex >= s.navHistory.length - 1) return {};
    return { navIndex: s.navIndex + 1, currentPath: s.navHistory[s.navIndex + 1] };
  }),
```

Also update the store shape so `goForward` is part of the public API and the test reset path keeps `navHistory` and `navIndex` aligned with the default root path.

- [ ] **Step 3: Wire the forward button into the toolbar**

Update `ToolbarProps` in `src/components/FileBrowser/Toolbar.tsx` to include `canGoForward: boolean` and `onForward: () => void`, then render a forward button in the same style as the existing arrow buttons:

```tsx
<button onClick={onForward} disabled={!canGoForward} className={navBtn} aria-label="前进" title="前进">
  →
</button>
```

In `src/components/FileBrowser/index.tsx`, read `goForward` from the store, compute `canGoForward` from `navIndex < navHistory.length - 1`, and pass both props into `Toolbar`:

```tsx
<Toolbar
  selectedCount={selected.size}
  onImport={handleImport}
  onExport={handleExport}
  onDelete={handleDelete}
  canGoBack={navIndex > 0}
  onBack={goBack}
  canGoForward={navIndex < navHistory.length - 1}
  onForward={goForward}
  canGoUp={currentPath !== "/"}
  onUp={() => navigate(parentPath(currentPath))}
  onRefresh={handleRefresh}
  onBookmark={handleAddBookmark}
/>
```

- [ ] **Step 4: Add a toolbar regression test**

Create `src/components/FileBrowser/Toolbar.test.tsx` with this coverage:

```ts
it("renders a forward button and calls the handler", () => {
  const onForward = vi.fn();

  render(
    <Toolbar
      selectedCount={0}
      onImport={vi.fn()}
      onExport={vi.fn()}
      onDelete={vi.fn()}
      canGoBack={true}
      onBack={vi.fn()}
      canGoForward={true}
      onForward={onForward}
      canGoUp={false}
      onUp={vi.fn()}
      onRefresh={vi.fn()}
      onBookmark={vi.fn()}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: "前进" }));
  expect(onForward).toHaveBeenCalledTimes(1);
});
```

Run:

```bash
npx vitest run src/store/index.test.ts src/components/FileBrowser/Toolbar.test.tsx -v
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/store/index.ts src/store/index.test.ts src/components/FileBrowser/Toolbar.tsx src/components/FileBrowser/Toolbar.test.tsx src/components/FileBrowser/index.tsx
git commit -m "feat: add forward file browser navigation"
```

---

### Task 2: Centralize browser shortcuts for back, forward, up, bookmark, upload, download, and delete

**Files:**
- Create: `src/components/FileBrowser/fileBrowserShortcuts.ts`
- Create: `src/components/FileBrowser/fileBrowserShortcuts.test.ts`
- Modify: `src/components/FileBrowser/index.tsx`

- [ ] **Step 1: Write the failing shortcut test**

Create `src/components/FileBrowser/fileBrowserShortcuts.test.ts` with these cases:

```ts
const createEvent = (key: string, target: EventTarget = document.body) =>
  ({
    key,
    metaKey: true,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    target,
    preventDefault: vi.fn(),
  }) as unknown as KeyboardEvent;

it("handles back, forward, up, bookmark, upload, download, and delete shortcuts", () => {
  const actions = {
    goBack: vi.fn(),
    goForward: vi.fn(),
    goUp: vi.fn(),
    addBookmark: vi.fn(),
    importFiles: vi.fn(),
    exportFiles: vi.fn(),
    deleteFiles: vi.fn(),
    selectAll: vi.fn(),
  };

  handleFileBrowserKeyDown(createEvent("["), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("]"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("ArrowUp"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("d"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("u"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("s"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });
  handleFileBrowserKeyDown(createEvent("Backspace"), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });

  expect(actions.goBack).toHaveBeenCalledTimes(1);
  expect(actions.goForward).toHaveBeenCalledTimes(1);
  expect(actions.goUp).toHaveBeenCalledTimes(1);
  expect(actions.addBookmark).toHaveBeenCalledTimes(1);
  expect(actions.importFiles).toHaveBeenCalledTimes(1);
  expect(actions.exportFiles).toHaveBeenCalledTimes(1);
  expect(actions.deleteFiles).toHaveBeenCalledTimes(1);
});

it("ignores shortcut keys when focus is in an input-like element", () => {
  const actions = {
    goBack: vi.fn(),
    goForward: vi.fn(),
    goUp: vi.fn(),
    addBookmark: vi.fn(),
    importFiles: vi.fn(),
    exportFiles: vi.fn(),
    deleteFiles: vi.fn(),
    selectAll: vi.fn(),
  };

  const input = document.createElement("input");
  handleFileBrowserKeyDown(createEvent("d", input), actions, { hasSelection: true, canGoBack: true, canGoForward: true, canGoUp: true });

  expect(actions.addBookmark).not.toHaveBeenCalled();
});
```

Run:

```bash
npx vitest run src/components/FileBrowser/fileBrowserShortcuts.test.ts -v
```

Expected: FAIL because `handleFileBrowserKeyDown` does not exist yet.

- [ ] **Step 2: Implement the helper**

Create `src/components/FileBrowser/fileBrowserShortcuts.ts` with a focused API:

```ts
export interface FileBrowserShortcutActions {
  goBack: () => void;
  goForward: () => void;
  goUp: () => void;
  addBookmark: () => void;
  importFiles: () => void;
  exportFiles: () => void;
  deleteFiles: () => void;
  selectAll: () => void;
}

export interface FileBrowserShortcutState {
  hasSelection: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  canGoUp: boolean;
}

export function handleFileBrowserKeyDown(
  event: KeyboardEvent,
  actions: FileBrowserShortcutActions,
  state: FileBrowserShortcutState,
): void {
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT" || target.isContentEditable)) {
    return;
  }

  if (!event.metaKey) return;

  const key = event.key;
  if (key === "a") {
    event.preventDefault();
    actions.selectAll();
    return;
  }
  if (key === "[") {
    event.preventDefault();
    if (state.canGoBack) actions.goBack();
    return;
  }
  if (key === "]") {
    event.preventDefault();
    if (state.canGoForward) actions.goForward();
    return;
  }
  if (key === "ArrowUp") {
    event.preventDefault();
    if (state.canGoUp) actions.goUp();
    return;
  }
  if (key === "d") {
    event.preventDefault();
    actions.addBookmark();
    return;
  }
  if (key === "u") {
    event.preventDefault();
    actions.importFiles();
    return;
  }
  if (key === "s" && state.hasSelection) {
    event.preventDefault();
    actions.exportFiles();
    return;
  }
  if (key === "Backspace" && state.hasSelection) {
    event.preventDefault();
    actions.deleteFiles();
  }
}
```

Update `src/components/FileBrowser/index.tsx` to call this helper from the existing `window.addEventListener("keydown", ...)` effect. Reuse the current handlers for `goBack`, `goForward`, `navigate(parentPath(currentPath))`, `handleAddBookmark`, `handleImport`, `handleExport`, `handleDelete`, and `selectAll`.

Use the current selection state to gate export and delete, so `Cmd+S` and `Cmd+Backspace` only do anything when files are selected.

- [ ] **Step 3: Run the shortcut test again**

Run:

```bash
npx vitest run src/components/FileBrowser/fileBrowserShortcuts.test.ts -v
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/components/FileBrowser/fileBrowserShortcuts.ts src/components/FileBrowser/fileBrowserShortcuts.test.ts src/components/FileBrowser/index.tsx
git commit -m "feat: add file browser keyboard shortcuts"
```

---

### Task 3: Add a blank-area right-click upload menu

**Files:**
- Create: `src/components/FileBrowser/FileBrowserContextMenu.tsx`
- Create: `src/components/FileBrowser/FileBrowserContextMenu.test.tsx`
- Create: `src/components/FileBrowser/FileBrowser.test.tsx`
- Modify: `src/components/FileBrowser/index.tsx`

- [ ] **Step 1: Write the failing menu test**

Create `src/components/FileBrowser/FileBrowserContextMenu.test.tsx` with this assertion:

```ts
it("renders an upload action and calls back on click", () => {
  const onUpload = vi.fn();
  const onClose = vi.fn();

  render(<FileBrowserContextMenu anchor={{ x: 120, y: 180 }} onUpload={onUpload} onClose={onClose} />);

  fireEvent.click(screen.getByRole("button", { name: "上传" }));

  expect(onUpload).toHaveBeenCalledTimes(1);
  expect(onClose).toHaveBeenCalledTimes(1);
});
```

Create `src/components/FileBrowser/FileBrowser.test.tsx` with these integration checks:

```ts
it("opens the upload menu when the file browser background is right-clicked", async () => {
  render(<FileBrowser />);

  fireEvent.contextMenu(screen.getByTestId("file-browser-scroll-area"), { clientX: 120, clientY: 180 });

  expect(await screen.findByRole("menu")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "上传" }));
  expect(open).toHaveBeenCalledTimes(1);
});

it("does not open the upload menu when a file row is right-clicked", async () => {
  render(<FileBrowser />);

  fireEvent.contextMenu(screen.getByText("Documents"));

  expect(screen.queryByRole("menu")).toBeNull();
});
```

Run:

```bash
npx vitest run src/components/FileBrowser/FileBrowserContextMenu.test.tsx src/components/FileBrowser/FileBrowser.test.tsx -v
```

Expected: FAIL because `FileBrowserContextMenu` and the menu wiring do not exist yet.

- [ ] **Step 2: Implement the menu component and browser shell state**

Create `src/components/FileBrowser/FileBrowserContextMenu.tsx` as a tiny overlay component that takes an anchor point and two callbacks:

```tsx
interface FileBrowserContextMenuProps {
  anchor: { x: number; y: number } | null;
  onUpload: () => void;
  onClose: () => void;
}

export function FileBrowserContextMenu({ anchor, onUpload, onClose }: FileBrowserContextMenuProps) {
  if (!anchor) return null;

  return (
    <div
      role="menu"
      className="fixed z-50 min-w-36 rounded bg-gray-800 border border-gray-700 shadow-lg p-1"
      style={{ left: anchor.x, top: anchor.y }}
    >
      <button
        type="button"
        className="w-full text-left px-3 py-2 text-sm text-gray-200 hover:bg-gray-700"
        onClick={() => {
          onUpload();
          onClose();
        }}
      >
        上传
      </button>
    </div>
  );
}
```

In `src/components/FileBrowser/index.tsx`, add local state for the menu anchor, for example:

```ts
const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
```

Attach `onContextMenu` to the scroll container around the file list/grid and only open the menu when the event comes from the blank background, not a row:

```tsx
<div
  data-testid="file-browser-scroll-area"
  className="flex-1 overflow-auto"
  onContextMenu={(e) => {
    if (e.target !== e.currentTarget) return;
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  }}
>
```

Render `FileBrowserContextMenu` next to the list/grid output and wire `onUpload` to the existing `handleImport` function. Close the menu when the user clicks elsewhere or presses `Escape` while the menu is open.

Keep the scope narrow: this menu only appears on blank-space right click and only exposes upload. Existing row clicks, double-click navigation, drag-and-drop, and toolbar actions stay unchanged.

- [ ] **Step 3: Run the menu tests again**

Run:

```bash
npx vitest run src/components/FileBrowser/FileBrowserContextMenu.test.tsx src/components/FileBrowser/FileBrowser.test.tsx -v
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/components/FileBrowser/FileBrowserContextMenu.tsx src/components/FileBrowser/FileBrowserContextMenu.test.tsx src/components/FileBrowser/FileBrowser.test.tsx src/components/FileBrowser/index.tsx
git commit -m "feat: add blank-area upload context menu"
```

---

## Self-Review

- Forward navigation is covered by Task 1 through the store, toolbar, and browser shell.
- Keyboard shortcuts are covered by Task 2, including focus guards and the `Cmd+[ / Cmd+] / Cmd+↑ / Cmd+D / Cmd+U / Cmd+S / Cmd+Backspace` mappings.
- Blank-area upload is covered by Task 3, and the menu is limited to the file list background instead of file rows.
- No new backend commands are needed; the plan only reuses existing upload/download/delete/bookmark/navigation actions.
- The plan stays inside the existing file browser subsystem, so it is still one implementation plan rather than multiple independent projects.
