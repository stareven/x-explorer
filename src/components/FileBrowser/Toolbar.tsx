import { useStore } from "../../store";

interface ToolbarProps {
  selectedCount: number;
  onImport: () => void;
  onExport: () => void;
  onDelete: () => void;
}

export function Toolbar({ selectedCount, onImport, onExport, onDelete }: ToolbarProps) {
  const viewMode = useStore((s) => s.viewMode);
  const setViewMode = useStore((s) => s.setViewMode);

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-700">
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
}
