import { useEffect, useRef, useState } from "react";

const HOLD_DURATION_MS = 600;

export function useCmdHoldOverlay(): boolean {
  const [visible, setVisible] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    function clearPendingTimer() {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Meta") {
        if (timerRef.current !== null) return;
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          setVisible(true);
        }, HOLD_DURATION_MS);
        return;
      }

      clearPendingTimer();
      // Defer setVisible(false) to the next macrotask. We're a window-level
      // keydown listener that runs *before* FileBrowser's window-level keydown
      // listener. Calling setState synchronously here makes React 18 flush
      // during the current event's listener pass, which triggers FileBrowser's
      // useEffect to cleanup-and-re-register its handler right between our
      // return and FileBrowser's handler firing — silently swallowing the
      // user's Cmd+U/Cmd+B/etc. Pressing a non-Meta key while the overlay is
      // up must still hide it, so we still schedule the hide, just not inline.
      window.setTimeout(() => setVisible(false), 0);
    }

    function handleKeyUp(event: KeyboardEvent) {
      if (event.key !== "Meta") return;
      clearPendingTimer();
      setVisible(false);
    }

    function handleBlur() {
      clearPendingTimer();
      setVisible(false);
    }

    function handleMouseDown() {
      clearPendingTimer();
      setVisible(false);
    }

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);
    window.addEventListener("mousedown", handleMouseDown);

    return () => {
      clearPendingTimer();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
      window.removeEventListener("mousedown", handleMouseDown);
    };
  }, []);

  return visible;
}
