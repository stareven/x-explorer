import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSelection } from "./useSelection";

describe("useSelection", () => {
  const items = ["a.txt", "b.txt", "c.txt", "d.txt"];

  it("toggles single item on click", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    expect(result.current.selected).toEqual(new Set(["a.txt"]));
  });

  it("adds item on cmd+click", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    act(() => result.current.handleClick("c.txt", true, false));
    expect(result.current.selected).toEqual(new Set(["a.txt", "c.txt"]));
  });

  it("replaces selection with a single item", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    act(() => result.current.selectOnly("c.txt"));
    expect(result.current.selected).toEqual(new Set(["c.txt"]));
  });

  it("selects all with selectAll()", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.selectAll());
    expect(result.current.selected.size).toBe(4);
  });

  it("clears selection", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.selectAll());
    act(() => result.current.clearSelection());
    expect(result.current.selected.size).toBe(0);
  });

  it("replaces selection with a set via selectMany()", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    act(() => result.current.selectMany(["b.txt", "c.txt"]));
    expect(result.current.selected).toEqual(new Set(["b.txt", "c.txt"]));
  });
});
