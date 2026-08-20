import { MouseEvent as ReactMouseEvent } from "react";
import { FileEntry } from "../../store";

interface FileGridProps {
  files: FileEntry[];
  selected: Set<string>;
  onNavigate: (path: string) => void;
  onSelect: (name: string, cmdKey: boolean, shiftKey: boolean) => void;
  onContextMenu: (name: string, event: ReactMouseEvent<HTMLDivElement>) => void;
}

export function FileGrid({ files, selected, onNavigate, onSelect, onContextMenu }: FileGridProps) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(80px,1fr))] gap-3 p-4">
      {files.map((file) => (
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
          <span className="text-3xl">{file.is_dir ? "📁" : "📄"}</span>
          <span className="text-xs text-gray-300 truncate w-full">{file.name}</span>
        </div>
      ))}
    </div>
  );
}
