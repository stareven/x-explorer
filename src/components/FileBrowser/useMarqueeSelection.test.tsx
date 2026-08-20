import { describe, it, expect, vi } from "vitest";
import { normalizeRect, rectsIntersect, collectBoxSelection, useMarqueeSelection, type MarqueeRect } from "./useMarqueeSelection";
import { fireEvent, render, screen } from "@testing-library/react";

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

function Harness({ onBoxSelect }: { onBoxSelect: (names: string[]) => void }) {
  const { marquee, onMouseDown } = useMarqueeSelection(onBoxSelect);
  return (
    <div data-testid="container" onMouseDown={onMouseDown}>
      <div data-file-entry data-file-name="a.txt" />
      <div data-file-entry data-file-name="b.txt" />
      {marquee && <div data-testid="marquee-overlay" />}
    </div>
  );
}

describe("useMarqueeSelection", () => {
  it("selects entries intersecting the dragged rectangle", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const entries = document.querySelectorAll<HTMLElement>("[data-file-entry]");
    entries[0].getBoundingClientRect = () => domRect(0, 0, 100, 20);
    entries[1].getBoundingClientRect = () => domRect(0, 40, 100, 20);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 0, clientY: 0 });
    fireEvent.mouseMove(window, { clientX: 100, clientY: 25 });
    expect(screen.getByTestId("marquee-overlay")).toBeInTheDocument();

    fireEvent.mouseUp(window, { clientX: 100, clientY: 25 });
    expect(onBoxSelect).toHaveBeenCalledWith(["a.txt"]);
    expect(screen.queryByTestId("marquee-overlay")).not.toBeInTheDocument();
  });

  it("does not trigger selection on a clean click", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.mouseUp(window, { clientX: 10, clientY: 10 });

    expect(onBoxSelect).not.toHaveBeenCalled();
  });

  it("ignores sub-threshold movement", () => {
    const onBoxSelect = vi.fn();
    render(<Harness onBoxSelect={onBoxSelect} />);

    const container = screen.getByTestId("container");
    fireEvent.mouseDown(container, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.mouseMove(window, { clientX: 12, clientY: 10 });
    fireEvent.mouseUp(window, { clientX: 12, clientY: 10 });

    expect(onBoxSelect).not.toHaveBeenCalled();
  });
});
