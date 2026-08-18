# 长按 Cmd 显示快捷键蒙层 设计文档

**日期：** 2026-08-18
**状态：** 已确认

---

## 概述

在整个应用中，长按 Cmd 键 800ms 后，弹出一个半透明全屏蒙层，居中展示一张卡片，列出当前 FileBrowser 支持的全部快捷键（按键组合 + 中文说明）。松开 Cmd 键后蒙层立即消失。该行为全局生效，不受输入框聚焦、当前界面等条件限制。

## 背景

FileBrowser 已经实现了 9 个 macOS 风格快捷键（`src/components/FileBrowser/shortcuts.ts` 中的 `getFileBrowserShortcutAction`），但用户没有直观的方式发现这些快捷键。参考 macOS 系统级"按住修饰键查看提示"的交互习惯，添加一个长按 Cmd 弹出的说明蒙层。

## 触发规则

- 单独按下并持续按住 `Meta`（Cmd）键，不释放、且期间没有按下任何其他键，达到 **800ms** 后显示蒙层。
- 蒙层显示期间，若用户松开 Cmd 键，立即隐藏蒙层（无过渡动画要求，也不需要额外的关闭方式）。
- 在等待 800ms 计时期间，如果用户按下了除 `Meta` 以外的任意键（意味着正在触发某个 Cmd+X 快捷键组合），取消计时器；如果蒙层已经显示，则立即关闭。
- 窗口失去焦点（`blur`）时重置内部状态（清除计时器、隐藏蒙层），避免因为 `keyup` 事件丢失（例如切换到其他应用时松开 Cmd）导致蒙层卡在显示状态。
- 该行为不检查 `isEditableTarget`（不同于现有快捷键的输入框保护逻辑）——无论焦点在哪里，长按 Cmd 都会弹出蒙层。这是与 `getFileBrowserShortcutAction` 唯一的行为差异点。

## 架构与文件

### `src/hooks/useCmdHoldOverlay.ts`（新建）

自定义 hook，无参数，返回 `boolean`（是否应显示蒙层）。内部：

- 在 `window` 上监听 `keydown`、`keyup`、`blur`。
- `keydown`：
  - 若 `event.key === "Meta"` 且当前没有计时器在跑、且没有"已按下非 Meta 键"标记 → 启动 `setTimeout(800ms)`，到时置 `isVisible = true`。
  - 若 `event.key !== "Meta"` → 清除计时器（如果存在），若 `isVisible === true` 则置为 `false`。
- `keyup`：
  - 若 `event.key === "Meta"` → 清除计时器，置 `isVisible = false`。
- `blur`：清除计时器，置 `isVisible = false`。
- 组件卸载时清理所有监听器和计时器。

### `src/components/FileBrowser/shortcuts.ts`（修改）

新增导出常量：

```ts
export const FILE_BROWSER_SHORTCUTS: Array<{
  action: FileBrowserShortcutAction;
  keys: string;
  description: string;
}> = [
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

这份列表是蒙层展示内容的唯一数据源，键位文案与 `getFileBrowserShortcutAction` 中的判断逻辑保持同源（共用 `FileBrowserShortcutAction` 类型），后续若增删快捷键，只需要同步维护这一处。

### `src/components/ShortcutsOverlay.tsx`（新建）

纯展示组件：

```ts
type ShortcutsOverlayProps = { visible: boolean };
export function ShortcutsOverlay({ visible }: ShortcutsOverlayProps) { ... }
```

- `visible === false` 时不渲染任何 DOM（`return null`）。
- `visible === true` 时渲染：
  - 最外层：`fixed inset-0 z-[100] flex items-center justify-center bg-black/40`，不绑定任何点击/键盘事件（纯展示，不可交互关闭）。
  - 居中卡片：`rounded border border-gray-700 bg-gray-900 p-4 shadow-lg`，视觉风格与现有 `GoToPathDialog` 一致。
  - 卡片标题："快捷键"。
  - 列表：遍历 `FILE_BROWSER_SHORTCUTS`，每行左侧是 kbd 样式徽章（`bg-gray-800 border border-gray-600 rounded px-1.5 py-0.5 text-xs font-mono`）展示 `keys`，右侧是 `text-sm text-gray-300` 展示 `description`。

### `src/App.tsx`（修改）

在组件顶层调用：

```ts
const showShortcuts = useCmdHoldOverlay();
```

并在根 `<div>` 内、与 `DevicePanel`/`FileBrowser`/`TransferPanel` 平级的位置渲染 `<ShortcutsOverlay visible={showShortcuts} />`，保证全局生效且不受某个子组件挂载/卸载影响。

## 测试

- `src/hooks/useCmdHoldOverlay.test.ts`：使用 `vitest` 的 fake timer 验证：
  - 按住 `Meta` 800ms 后返回 `true`。
  - 未满 800ms 松开 `Meta` 不会变为 `true`。
  - 显示后松开 `Meta` 立即变回 `false`。
  - 等待期间按下非 `Meta` 键会取消计时（之后即使凑够 800ms 也不显示）。
  - 已显示时按下非 `Meta` 键会立即隐藏。
  - 触发 `window` 的 `blur` 事件会重置为 `false` 并清除挂起的计时器。
- `src/components/ShortcutsOverlay.test.tsx`：
  - `visible={false}` 时容器不存在于 DOM 中。
  - `visible={true}` 时渲染出全部 9 条快捷键的按键文本与说明文本。

## 范围之外

- 不做动画过渡效果。
- 不支持鼠标点击蒙层关闭（不需要，因为这是"按住查看"交互，松开即消失）。
- 不涉及 Windows/Linux 平台的 Ctrl 键等价映射（本项目是 macOS 专属应用）。
