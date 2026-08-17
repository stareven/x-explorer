import { useEffect, useRef } from "react";

type FileBrowserContextMenuAction = {
  label: string;
  onAction: () => void;
};

type FileBrowserContextMenuProps = {
  x: number;
  y: number;
  actions: FileBrowserContextMenuAction[];
  onClose: () => void;
};

export function FileBrowserContextMenu({ x, y, actions, onClose }: FileBrowserContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    function handlePointerDown(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handlePointerDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handlePointerDown);
    };
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 pointer-events-none">
      <div
        ref={menuRef}
        role="menu"
        className="absolute pointer-events-auto min-w-24 rounded border border-gray-700 bg-gray-900 py-1 shadow-lg"
        style={{ left: x, top: y }}
      >
        {actions.map((action) => (
          <button
            key={action.label}
            type="button"
            role="menuitem"
            className="block w-full px-3 py-1.5 text-left text-sm text-gray-100 hover:bg-gray-700"
            onClick={() => {
              action.onAction();
              onClose();
            }}
          >
            {action.label}
          </button>
        ))}
      </div>
    </div>
  );
}
