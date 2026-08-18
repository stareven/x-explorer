import { useEffect, useRef, useState } from "react";

const HOLD_DURATION_MS = 800;

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
      setVisible(false);
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

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleBlur);

    return () => {
      clearPendingTimer();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  return visible;
}
