/**
 * Injectable clock for backtesting. In production, clock.now() === Date.now().
 * The backtester calls setClock() to control time during tick replay.
 */

let _now: () => number = Date.now;

export function now(): number {
  return _now();
}

export function setClock(fn: () => number): void {
  _now = fn;
}

export function resetClock(): void {
  _now = Date.now;
}
