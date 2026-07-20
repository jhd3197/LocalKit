import { useEffect, useRef } from "react";

/**
 * Shared dialog behavior (plan 15, phase 4 — Faro's useDialog): Escape to
 * close + outside-click to close, in one place instead of each modal
 * hand-rolling its own listeners. Initial focus stays declarative
 * (`autoFocus` on the panel's first field).
 *
 * Usage:
 *   const { overlayProps, panelProps } = useDialog(onClose);
 *   <div {...overlayProps} className="fixed inset-0 ...">
 *     <div {...panelProps} role="dialog" aria-modal="true">...</div>
 *   </div>
 */

// Open dialogs stack — Escape closes only the topmost one.
const openStack: symbol[] = [];

export function useDialog(onClose: () => void) {
  // Always call the latest onClose without re-binding the listener.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    const id = Symbol("dialog");
    openStack.push(id);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && openStack[openStack.length - 1] === id) {
        closeRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      const i = openStack.indexOf(id);
      if (i !== -1) openStack.splice(i, 1);
    };
  }, []);

  return {
    overlayProps: {
      onClick: () => closeRef.current(),
    },
    panelProps: {
      onClick: (e: React.MouseEvent) => e.stopPropagation(),
    },
  };
}
