import { FILE_BROWSER_SHORTCUTS } from "./FileBrowser/shortcuts";

type ShortcutsOverlayProps = {
  visible: boolean;
};

export function ShortcutsOverlay({ visible }: ShortcutsOverlayProps) {
  if (!visible) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/40">
      <div
        role="dialog"
        aria-label="快捷键说明"
        className="w-80 rounded border border-gray-700 bg-gray-900 p-4 shadow-lg"
      >
        <h2 className="text-xs font-semibold text-gray-400 mb-3">快捷键</h2>
        <ul className="space-y-2">
          {FILE_BROWSER_SHORTCUTS.map((shortcut) => (
            <li key={shortcut.action} className="flex items-center justify-between gap-3">
              <span className="rounded border border-gray-600 bg-gray-800 px-1.5 py-0.5 text-xs font-mono text-gray-200">
                {shortcut.keys}
              </span>
              <span className="text-sm text-gray-300">{shortcut.description}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
