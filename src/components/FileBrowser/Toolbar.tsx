import { forwardRef } from "react";
import { useStore } from "../../store";

interface ToolbarProps {
  selectedCount: number;
  onImport: () => void;
  onExport: () => void;
  onDelete: () => void;
  canGoBack: boolean;
  onBack: () => void;
  canGoForward: boolean;
  onForward: () => void;
  canGoUp: boolean;
  onUp: () => void;
  onRefresh: () => void;
  onBookmark: () => void;
  searchValue: string;
  onSearchChange: (value: string) => void;
}

export const Toolbar = forwardRef<HTMLInputElement, ToolbarProps>(function Toolbar(
  { selectedCount, onImport, onExport, onDelete, canGoBack, onBack, canGoForward, onForward, canGoUp, onUp, onRefresh, onBookmark, searchValue, onSearchChange },
  searchInputRef,
) {
  const viewMode = useStore((s) => s.viewMode);
  const setViewMode = useStore((s) => s.setViewMode);

  const navBtn =
    "px-2 py-1 text-xs rounded text-gray-300 bg-gray-700 hover:bg-gray-600 disabled:opacity-40 disabled:cursor-default";

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-700">
      <button onClick={onBack} disabled={!canGoBack} className={navBtn} aria-label="返回" title="返回">
        ←
      </button>
      <button onClick={onForward} disabled={!canGoForward} className={navBtn} aria-label="前进" title="前进">
        →
      </button>
      <button onClick={onUp} disabled={!canGoUp} className={navBtn} aria-label="回到上级目录" title="回到上级目录">
        ↑
      </button>
      <button onClick={onRefresh} className={navBtn} aria-label="刷新" title="刷新">
        ↻
      </button>
      <button onClick={onBookmark} className={navBtn} aria-label="收藏当前目录" title="收藏当前目录">
        ☆
      </button>
      <input
        ref={searchInputRef}
        value={searchValue}
        onChange={(e) => onSearchChange(e.target.value)}
        placeholder="搜索文件"
        aria-label="搜索文件"
        className="w-48 min-w-0 px-2 py-1 text-xs bg-gray-800 text-gray-200 border border-gray-600 rounded placeholder:text-gray-500 focus:outline-none focus:border-blue-500"
      />
      <button
        onClick={onImport}
        className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700"
      >
        导入
      </button>
      {selectedCount > 0 && (
        <>
          <button
            onClick={onExport}
            className="px-3 py-1 text-xs bg-gray-600 text-white rounded hover:bg-gray-500"
          >
            导出 ({selectedCount})
          </button>
          <button
            onClick={onDelete}
            className="px-3 py-1 text-xs bg-red-700 text-white rounded hover:bg-red-600"
          >
            删除 ({selectedCount})
          </button>
        </>
      )}
      <div className="ml-auto flex gap-1">
        <button
          onClick={() => setViewMode("list")}
          className={`px-2 py-1 text-xs rounded ${viewMode === "list" ? "bg-gray-600 text-white" : "text-gray-400 hover:text-white"}`}
          aria-label="列表视图"
        >
          ☰
        </button>
        <button
          onClick={() => setViewMode("grid")}
          className={`px-2 py-1 text-xs rounded ${viewMode === "grid" ? "bg-gray-600 text-white" : "text-gray-400 hover:text-white"}`}
          aria-label="网格视图"
        >
          ⊞
        </button>
      </div>
    </div>
  );
});
