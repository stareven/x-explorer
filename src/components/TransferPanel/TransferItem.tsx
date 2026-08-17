import { TransferTask } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function TransferItem({ task }: { task: TransferTask }) {
  const pct =
    task.total_files > 0
      ? Math.round((task.completed_files / task.total_files) * 100)
      : 0;

  const canCancel = task.status === "pending" || task.status === "running";

  return (
    <div className="px-3 py-2 border-b border-gray-700 text-xs">
      <div className="flex items-center justify-between mb-1">
        <span className="truncate text-gray-300 max-w-[180px]">
          {task.src.split("/").pop()}
        </span>
        <div className="flex items-center gap-2">
          <span className="tabular-nums text-gray-400">
            {task.completed_files}/{task.total_files}
          </span>
          <span className={`text-xs ${task.status === "error" ? "text-red-400" : "text-gray-500"}`}>
            {task.status === "error" ? task.error ?? "error" : task.status}
          </span>
          {canCancel && (
            <button
              onClick={() => tauriApi.cancelTransfer(task.id)}
              className="text-gray-500 hover:text-red-400"
            >
              ✕
            </button>
          )}
        </div>
      </div>
      <div className="h-1 bg-gray-700 rounded overflow-hidden">
        <div
          className={`h-full rounded transition-all ${
            task.status === "error" ? "bg-red-500" : "bg-blue-500"
          }`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
