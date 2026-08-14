import { useState } from "react";

export function useSelection(items: string[]) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastClicked, setLastClicked] = useState<string | null>(null);

  function handleClick(name: string, cmdKey: boolean, shiftKey: boolean) {
    if (cmdKey) {
      setSelected((prev) => {
        const next = new Set(prev);
        if (next.has(name)) next.delete(name);
        else next.add(name);
        return next;
      });
    } else if (shiftKey && lastClicked) {
      const fromIdx = items.indexOf(lastClicked);
      const toIdx = items.indexOf(name);
      const [start, end] = fromIdx < toIdx ? [fromIdx, toIdx] : [toIdx, fromIdx];
      setSelected(new Set(items.slice(start, end + 1)));
    } else {
      setSelected(new Set([name]));
    }
    setLastClicked(name);
  }

  function selectAll() {
    setSelected(new Set(items));
  }

  function clearSelection() {
    setSelected(new Set());
  }

  return { selected, handleClick, selectAll, clearSelection };
}
