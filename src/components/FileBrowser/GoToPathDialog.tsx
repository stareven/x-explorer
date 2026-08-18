import { useEffect, useRef, useState, type FormEvent } from "react";

type GoToPathDialogProps = {
  initialValue: string;
  onSubmit: (path: string) => void;
  onClose: () => void;
};

export function GoToPathDialog({ initialValue, onSubmit, onClose }: GoToPathDialogProps) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
    onClose();
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onClose}>
      <form
        role="dialog"
        aria-label="跳转到目录"
        className="w-96 rounded border border-gray-700 bg-gray-900 p-4 shadow-lg"
        onClick={(e) => e.stopPropagation()}
        onSubmit={handleSubmit}
      >
        <label htmlFor="goto-path-input" className="block text-xs font-semibold text-gray-400 mb-2">
          跳转到目录
        </label>
        <input
          id="goto-path-input"
          ref={inputRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="/Documents/example"
          className="w-full px-2 py-1.5 text-sm bg-gray-800 text-gray-200 border border-gray-600 rounded placeholder:text-gray-500 focus:outline-none focus:border-blue-500"
        />
        <div className="mt-3 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="px-3 py-1 text-xs bg-gray-700 text-gray-200 rounded hover:bg-gray-600"
          >
            取消
          </button>
          <button
            type="submit"
            className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700"
          >
            跳转
          </button>
        </div>
      </form>
    </div>
  );
}
