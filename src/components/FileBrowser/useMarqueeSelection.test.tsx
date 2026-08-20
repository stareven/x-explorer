import { describe, it, expect } from "vitest";
import { normalizeRect, rectsIntersect, collectBoxSelection, type MarqueeRect } from "./useMarqueeSelection";

function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

describe("normalizeRect", () => {
  it("normalizes a reversed drag into top-left origin", () => {
    expect(normalizeRect(100, 80, 20, 10)).toEqual({ left: 20, top: 10, width: 80, height: 70 });
  });
});

describe("rectsIntersect", () => {
  const marquee = (l: number, t: number, w: number, h: number): MarqueeRect => ({ left: l, top: t, width: w, height: h });

  it("detects overlap", () => {
    expect(rectsIntersect(domRect(10, 10, 20, 20), marquee(20, 20, 20, 20))).toBe(true);
  });

  it("counts touching edges as intersecting", () => {
    expect(rectsIntersect(domRect(0, 0, 20, 20), marquee(20, 20, 10, 10))).toBe(true);
  });

  it("detects disjoint rects", () => {
    expect(rectsIntersect(domRect(0, 0, 10, 10), marquee(50, 50, 10, 10))).toBe(false);
  });

  it("detects containment", () => {
    expect(rectsIntersect(domRect(5, 5, 2, 2), marquee(0, 0, 10, 10))).toBe(true);
  });
});

describe("collectBoxSelection", () => {
  it("returns file names whose entry rects intersect the marquee", () => {
    const container = document.createElement("div");

    const a = document.createElement("div");
    a.setAttribute("data-file-entry", "");
    a.setAttribute("data-file-name", "a.txt");
    a.getBoundingClientRect = () => domRect(0, 0, 100, 20);

    const b = document.createElement("div");
    b.setAttribute("data-file-entry", "");
    b.setAttribute("data-file-name", "b.txt");
    b.getBoundingClientRect = () => domRect(0, 40, 100, 20);

    container.append(a, b);

    expect(collectBoxSelection(container, { left: 0, top: 0, width: 100, height: 25 })).toEqual(["a.txt"]);
  });
});
