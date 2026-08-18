import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GoToPathDialog } from "./GoToPathDialog";

describe("GoToPathDialog", () => {
  it("prefills the input with the current path and submits the trimmed value", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    render(<GoToPathDialog initialValue="/Documents" onSubmit={onSubmit} onClose={onClose} />);

    const input = screen.getByPlaceholderText("/Documents/example") as HTMLInputElement;
    expect(input.value).toBe("/Documents");

    fireEvent.change(input, { target: { value: "  /Documents/Photos  " } });
    fireEvent.click(screen.getByRole("button", { name: "跳转" }));

    expect(onSubmit).toHaveBeenCalledWith("/Documents/Photos");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not submit an empty path", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    render(<GoToPathDialog initialValue="/Documents" onSubmit={onSubmit} onClose={onClose} />);

    const input = screen.getByPlaceholderText("/Documents/example");
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.click(screen.getByRole("button", { name: "跳转" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes without submitting from the cancel button", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    render(<GoToPathDialog initialValue="/Documents" onSubmit={onSubmit} onClose={onClose} />);

    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(onSubmit).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes from Escape", () => {
    const onClose = vi.fn();

    render(<GoToPathDialog initialValue="/Documents" onSubmit={vi.fn()} onClose={onClose} />);

    fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
