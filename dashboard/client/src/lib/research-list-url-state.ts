export function readEnumParam<T extends string>(
  params: URLSearchParams,
  key: string,
  allowed: readonly T[],
  fallback: T,
): T {
  const value = params.get(key);
  return allowed.includes(value as T) ? (value as T) : fallback;
}

export function readTextParam(params: URLSearchParams, key: string): string {
  return params.get(key) ?? "";
}

export function readEnumListParam<T extends string>(
  params: URLSearchParams,
  key: string,
  allowed: readonly T[],
  fallback: readonly T[],
): T[] {
  const value = params.get(key);
  if (value == null) return [...fallback];
  if (value === "none") return [];
  const allowedSet = new Set(allowed);
  const parsed = value
    .split(",")
    .map((item) => item.trim())
    .filter((item): item is T => allowedSet.has(item as T));
  return parsed.length > 0 ? parsed : [...fallback];
}

export function updateQueryParam(
  params: URLSearchParams,
  setParams: (
    nextInit: URLSearchParams,
    navigateOptions?: { replace?: boolean },
  ) => void,
  key: string,
  value: string,
  fallback: string,
) {
  const next = new URLSearchParams(params);
  setQueryParam(next, key, value, fallback);
  setParams(next, { replace: true });
}

export function setQueryParam(
  params: URLSearchParams,
  key: string,
  value: string,
  fallback: string,
) {
  if (value === fallback || value === "") {
    params.delete(key);
  } else {
    params.set(key, value);
  }
}

export function updateQueryListParam<T extends string>(
  params: URLSearchParams,
  setParams: (
    nextInit: URLSearchParams,
    navigateOptions?: { replace?: boolean },
  ) => void,
  key: string,
  value: readonly T[],
  fallback: readonly T[],
) {
  const next = new URLSearchParams(params);
  setQueryListParam(next, key, value, fallback);
  setParams(next, { replace: true });
}

export function setQueryListParam<T extends string>(
  params: URLSearchParams,
  key: string,
  value: readonly T[],
  fallback: readonly T[],
) {
  if (sameList(value, fallback)) {
    params.delete(key);
  } else {
    params.set(key, value.length === 0 ? "none" : value.join(","));
  }
}

export function sameEnumSet<T extends string>(
  left: readonly T[],
  right: readonly T[],
): boolean {
  return (
    left.length === right.length && right.every((value) => left.includes(value))
  );
}

function sameList<T extends string>(left: readonly T[], right: readonly T[]) {
  if (left.length !== right.length) return false;
  return left.every((value, index) => value === right[index]);
}
