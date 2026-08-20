# 文件区鼠标框选 设计文档

**日期：** 2026-08-20
**状态：** 已确认

---

## 概述

在 FileBrowser 的文件浏览区域，支持按住鼠标左键拖出一个矩形框，松手后框选矩形相交的所有文件条目作为新的选中集（替换旧选中）。列表视图与网格视图均支持。普通点击（未拖动）行为完全不变。

## 背景

当前文件区只有「点选 / Cmd 点选 / Shift 点选」，没有真正的框选能力。用户尝试拖拽时，浏览器会触发原生文本选中（文件名上的文字高亮），叠加在条目的 `bg-blue-900` 选中高亮上，观感不自然。已通过给浏览区域加 `select-none` 关闭文本选中，本特性补齐真正的框选交互。

## 交互行为

- 在文件区任意位置（包括按在某个条目上）按住左键拖动，**移动超过 4px** 进入框选，出现蓝色半透明矩形框。
- 松手时，所有与矩形相交的条目成为新选中（**替换**之前的选中，框选期间忽略 Cmd/Shift）。
- 矩形覆盖不到任何条目 → 清空选中。
- 未拖动（<4px）的普通点击：仍是现有点选 / Cmd 点选 / Shift 点选 / 双击打开，完全不变。
- 列表视图：矩形碰到某行任意部分即选中整行。
- 拖到区域外或窗口外松手：通过 window 级监听保证也能正常结束。
- 仅左键（`button === 0`）触发，右键 / 中键不参与。

## 架构与文件

### `src/components/FileBrowser/useMarqueeSelection.ts`（新建）

自定义 hook，接管框选的鼠标事件与矩形状态。返回：

- `marquee: { left: number; top: number; width: number; height: number } | null` —— 当前框选矩形（视口坐标，已归一化），用于渲染浮层。
- 绑定到滚动容器的 `onMouseDown` 处理函数。

内部逻辑：

- `onMouseDown`（仅 `button === 0`）：记录起点 `startX/startY`，置 `tracking = true`，并在 `window` 上挂 `mousemove` / `mouseup` 监听（挂 window 以支持拖到区域外松手）。
- `mousemove`：若 `tracking`，计算 `dx/dy`，当 `Math.max(|dx|, |dy|) > 4` 时置 `dragging = true`；`dragging` 期间每次更新 `marquee` 状态。
- `mouseup`：若 `dragging`，归一化矩形，调用 `onBoxSelect(collectBoxSelection(container, rect))`；无论是否 `dragging`，清理状态与监听。
- 卸载时清理 window 监听。

导出两个纯函数（便于单测）：

```ts
function rectsIntersect(a: DOMRect, b: Rect): boolean;
function collectBoxSelection(container: HTMLElement, rect: Rect): string[];
```

`collectBoxSelection` 内部 `container.querySelectorAll<HTMLElement>("[data-file-entry]")`，对每个元素取 `getBoundingClientRect()` 与矩形做相交判断，命中则收集其 `dataset.fileName`。

### `src/components/FileBrowser/useSelection.ts`（修改）

新增 `selectMany(names: string[])`：

```ts
function selectMany(names: string[]) {
  setSelected(new Set(names));
  setLastClicked(null);
}
```

### `src/components/FileBrowser/FileList.tsx` / `FileGrid.tsx`（修改）

每个条目根元素新增 `data-file-name={file.name}`（与现有 `data-file-entry` 并列），用于 DOM 矩形 → 文件名映射。

**不改动 onClick。** 框选是「替换」语义，拖后触发的 `click` 要么落在与 mousedown 相同的条目上（结果与框选一致）、要么落在非交互的公共祖先上，不会覆盖框选结果。

### `src/components/FileBrowser/index.tsx`（修改）

- 调用 `useMarqueeSelection`，把返回的 `onMouseDown` 绑到滚动容器（`flex-1 overflow-auto select-none`）。
- 把 `selectMany`（来自 `useSelection`）作为 `onBoxSelect` 传给 hook。
- 在组件根部渲染矩形浮层（`marquee` 非空时）：

```tsx
{marquee && (
  <div
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

## 数据流

```
mousedown(容器) → 记录起点
mousemove(window) → 超过阈值 → 更新 marquee 矩形 → 渲染浮层
mouseup(window) → dragging ? collectBoxSelection(容器, 矩形) → onBoxSelect → selectMany → store.selected 更新
               → 无拖动 → 不干预，走原有 click/onSelect
```

## 边界与安全

- 仅左键触发；右键/中键不参与。
- 浮层 `pointer-events: none`，不拦截后续鼠标事件。
- 与现有「拖入文件上传」的 `onDrop` / `onDragOver` 不冲突（那是原生 HTML5 drag 事件，与 mousedown/mousemove/mouseup 独立）。
- 与文本选中不冲突：浏览区域已 `select-none`，拖拽不再产生文字高亮。
- 不涉及设备端路径拼接，无安全面变化。

## 测试

- `rectsIntersect`：相交 / 相切 / 不相交 / 包含。
- `collectBoxSelection`：jsdom 中造带 `data-file-entry` / `data-file-name` 的元素，mock `getBoundingClientRect`，断言命中集合。
- `useMarqueeSelection` hook（@testing-library + fireEvent）：
  - mousedown + 超过阈值 mousemove + mouseup → `onBoxSelect` 收到正确条目。
  - 未超过阈值（干净点击）→ 不触发 `onBoxSelect`。
- `useSelection.selectMany`：将选中集替换为给定集合。
- FileList / FileGrid：断言条目渲染出 `data-file-name`。

## 范围之外

- 框选期间不支持 Cmd/Shift 组合（本特性语义为「替换」）。
- 不做拖到视口边缘时的自动滚动（后续按需补充）。
- 不做「套索」自由形状选择，仅矩形框选。
