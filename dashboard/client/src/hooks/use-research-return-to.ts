import { useEffect, useMemo } from "react";
import { useLocation } from "react-router-dom";

const STORAGE_PREFIX = "buba.research.return.";

function storageKey(scope: string) {
  return `${STORAGE_PREFIX}${scope}`;
}

function sanitizeReturnTo(value: unknown, fallbackPath: string) {
  if (typeof value !== "string" || !value.startsWith(fallbackPath)) {
    return null;
  }
  const suffix = value.slice(fallbackPath.length);
  if (suffix && !suffix.startsWith("?") && !suffix.startsWith("#")) {
    return null;
  }
  if (value.startsWith("//") || value.includes("://")) {
    return null;
  }
  return value;
}

export function useRememberResearchListReturn(
  scope: string,
  listPath: string,
) {
  const location = useLocation();
  useEffect(() => {
    if (location.pathname !== listPath) return;
    if (typeof window === "undefined") return;
    try {
      window.sessionStorage.setItem(
        storageKey(scope),
        `${location.pathname}${location.search}`,
      );
    } catch {
      return;
    }
  }, [listPath, location.pathname, location.search, scope]);
}

export function useResearchReturnTo(scope: string, fallbackPath: string) {
  const location = useLocation();
  return useMemo(() => {
    const state = location.state as { returnTo?: unknown } | null;
    const stateTarget = sanitizeReturnTo(state?.returnTo, fallbackPath);
    if (stateTarget) return stateTarget;
    if (typeof window === "undefined") return fallbackPath;
    try {
      return (
        sanitizeReturnTo(
          window.sessionStorage.getItem(storageKey(scope)),
          fallbackPath,
        ) ?? fallbackPath
      );
    } catch {
      return fallbackPath;
    }
  }, [fallbackPath, location.state, scope]);
}
