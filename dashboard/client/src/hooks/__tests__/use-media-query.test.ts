import { renderHook, act } from "@testing-library/react";
import { vi } from "vitest";
import { useMediaQuery } from "../use-media-query";

function mockMatchMedia(matches: boolean) {
  const listeners: Array<(e: MediaQueryListEvent) => void> = [];
  const mql = {
    matches,
    media: "",
    onchange: null,
    addEventListener: vi.fn((_: string, fn: (e: MediaQueryListEvent) => void) => {
      listeners.push(fn);
    }),
    removeEventListener: vi.fn((_: string, fn: (e: MediaQueryListEvent) => void) => {
      const idx = listeners.indexOf(fn);
      if (idx >= 0) listeners.splice(idx, 1);
    }),
    dispatchEvent: () => false,
    addListener: vi.fn(),
    removeListener: vi.fn(),
  };
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: vi.fn().mockReturnValue(mql),
  });
  return { mql, listeners };
}

test("returns initial match state", () => {
  mockMatchMedia(true);
  const { result } = renderHook(() => useMediaQuery("(min-width: 768px)"));
  expect(result.current).toBe(true);
});

test("returns false when query does not match", () => {
  mockMatchMedia(false);
  const { result } = renderHook(() => useMediaQuery("(min-width: 768px)"));
  expect(result.current).toBe(false);
});

test("updates when media query changes", () => {
  const { mql, listeners } = mockMatchMedia(false);
  const { result } = renderHook(() => useMediaQuery("(min-width: 768px)"));
  expect(result.current).toBe(false);

  act(() => {
    mql.matches = true;
    for (const fn of listeners) fn({ matches: true } as MediaQueryListEvent);
  });
  expect(result.current).toBe(true);
});

test("cleans up listener on unmount", () => {
  const { mql } = mockMatchMedia(true);
  const { unmount } = renderHook(() => useMediaQuery("(min-width: 768px)"));
  unmount();
  expect(mql.removeEventListener).toHaveBeenCalled();
});
