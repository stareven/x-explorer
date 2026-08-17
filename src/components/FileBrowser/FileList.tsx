import { FileEntry } from "../../store";

function formatSize(bytes: number): string {
  if (bytes === 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function pad(value: number): string {
  return value.toString().padStart(2, "0");
}

function formatModifiedTime(seconds?: number): string {
  if (seconds == null) return "—";
  const date = new Date(seconds * 1000);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

interface FileListProps {
  files: FileEntry[];
  selected: Set<string>;
  onNavigate: (path: string) => void;
  onSelect: (name: string, cmdKey: boolean, shiftKey: boolean) => void;
}

export function FileList({ files, selected, onNavigate, onSelect }: FileListProps) {
  return (
    <table className="w-full text-sm text-gray-300">
      <thead>
        <tr className="text-xs text-gray-500 border-b border-gray-700">
          <th className="text-left px-3 py-1 font-normal">名称</th>
          <th className="text-right px-3 py-1 font-normal w-24">大小</th>
          <th className="text-right px-3 py-1 font-normal w-40">修改时间</th>
        </tr>
      </thead>
      <tbody>
        {files.map((file) => (
          <tr
            key={file.path}
            onClick={(e) => onSelect(file.name, e.metaKey, e.shiftKey)}
            onDoubleClick={() => file.is_dir && onNavigate(file.path)}
            className={`cursor-pointer hover:bg-gray-700 ${
              selected.has(file.name) ? "bg-blue-900" : ""
            }`}
          >
            <td className="px-3 py-1.5 flex items-center gap-2">
              <span>{file.is_dir ? "📁" : "📄"}</span>
              {file.name}
            </td>
            <td className="px-3 py-1.5 text-right text-gray-500 text-xs whitespace-nowrap">
              {file.is_dir ? "—" : formatSize(file.size)}
            </td>
            <td className="px-3 py-1.5 text-right text-gray-500 text-xs whitespace-nowrap">
              {formatModifiedTime(file.modified)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
