import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/utils";

interface PopoverProps {
  anchor: HTMLElement | null;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  align?: "start" | "end";
  width?: number;
  ariaLabel?: string;
  className?: string;
}

interface Position {
  top: number;
  left: number;
  width: number;
}

const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 4;

export function Popover({
  anchor,
  open,
  onClose,
  children,
  align = "start",
  width = 280,
  ariaLabel,
  className,
}: PopoverProps) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState<Position | null>(null);

  useLayoutEffect(() => {
    if (!open || !anchor) return;
    const update = () => {
      const rect = anchor.getBoundingClientRect();
      const effectiveWidth = Math.min(
        width,
        Math.max(160, window.innerWidth - 2 * VIEWPORT_MARGIN),
      );
      const top = rect.bottom + window.scrollY + ANCHOR_GAP;
      const rawLeft =
        align === "end"
          ? rect.right + window.scrollX - effectiveWidth
          : rect.left + window.scrollX;
      const maxLeft =
        window.innerWidth + window.scrollX - effectiveWidth - VIEWPORT_MARGIN;
      const left = Math.max(
        VIEWPORT_MARGIN + window.scrollX,
        Math.min(rawLeft, maxLeft),
      );
      setPosition({ top, left, width: effectiveWidth });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open, anchor, align, width]);

  useEffect(() => {
    if (!open) return;
    const handlePointer = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (panelRef.current?.contains(target)) return;
      if (anchor?.contains(target)) return;
      onClose();
    };
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    document.addEventListener("mousedown", handlePointer);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handlePointer);
      document.removeEventListener("keydown", handleKey);
    };
  }, [open, onClose, anchor]);

  useEffect(() => {
    if (!open || !panelRef.current) return;
    const first = panelRef.current.querySelector<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    first?.focus();
  }, [open]);

  if (!open || !position) return null;

  return createPortal(
    <div
      ref={panelRef}
      role="dialog"
      aria-label={ariaLabel}
      className={cn(
        "absolute z-50 border border-border bg-bg shadow-[3px_3px_0_rgba(0,0,0,0.18)]",
        className,
      )}
      style={{ top: position.top, left: position.left, width: position.width }}
    >
      {children}
    </div>,
    document.body,
  );
}
