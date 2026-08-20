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
