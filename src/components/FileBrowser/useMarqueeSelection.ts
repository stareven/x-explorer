import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";

export interface MarqueeRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function normalizeRect(x1: number, y1: number, x2: number, y2: number): MarqueeRect {
  return {
    left: Math.min(x1, x2),
    top: Math.min(y1, y2),
    width: Math.abs(x2 - x1),
    height: Math.abs(y2 - y1),
  };
}

export function rectsIntersect(a: DOMRect, b: MarqueeRect): boolean {
  return (
    a.left <= b.left + b.width &&
    a.left + a.width >= b.left &&
    a.top <= b.top + b.height &&
    a.top + a.height >= b.top
  );
}

export function collectBoxSelection(container: HTMLElement, rect: MarqueeRect): string[] {
  const names: string[] = [];
  container.querySelectorAll<HTMLElement>("[data-file-entry]").forEach((el) => {
    const name = el.dataset.fileName;
    if (name != null && rectsIntersect(el.getBoundingClientRect(), rect)) {
      names.push(name);
    }
  });
  return names;
}

const DRAG_THRESHOLD_PX = 4;

export function useMarqueeSelection(onBoxSelect: (names: string[]) => void) {
  const containerRef = useRef<HTMLElement | null>(null);
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const draggingRef = useRef(false);
  const [marquee, setMarquee] = useState<MarqueeRect | null>(null);
  const onBoxSelectRef = useRef(onBoxSelect);
  onBoxSelectRef.current = onBoxSelect;

  const handleMouseMove = useCallback((e: MouseEvent) => {
    const start = startRef.current;
    if (!start) return;
    const dx = e.clientX - start.x;
    const dy = e.clientY - start.y;
    if (!draggingRef.current && Math.max(Math.abs(dx), Math.abs(dy)) <= DRAG_THRESHOLD_PX) return;
    draggingRef.current = true;
    setMarquee(normalizeRect(start.x, start.y, e.clientX, e.clientY));
  }, []);

  const handleMouseUp = useCallback((e: MouseEvent) => {
    const start = startRef.current;
    if (draggingRef.current && start && containerRef.current) {
      onBoxSelectRef.current(
        collectBoxSelection(containerRef.current, normalizeRect(start.x, start.y, e.clientX, e.clientY)),
      );
    }
    window.removeEventListener("mousemove", handleMouseMove);
    window.removeEventListener("mouseup", handleMouseUp);
    startRef.current = null;
    draggingRef.current = false;
    setMarquee(null);
  }, [handleMouseMove]);

  function onMouseDown(e: ReactMouseEvent<HTMLElement>) {
    if (e.button !== 0) return;
    containerRef.current = e.currentTarget;
    startRef.current = { x: e.clientX, y: e.clientY };
    draggingRef.current = false;
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
  }

  useEffect(() => {
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [handleMouseMove, handleMouseUp]);

  return { marquee, onMouseDown };
}
