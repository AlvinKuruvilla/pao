// Recently opened executables, newest first. Kept in localStorage: a
// per-machine convenience, so losing it (cleared storage, a private window)
// only empties the list.

const KEY = "pao.recent";
/** Arbitrary: enough to cover a working session, short enough to scan. */
const LIMIT = 6;

export function recentPaths(): string[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(stored) ? stored.filter((path) => typeof path === "string") : [];
  } catch {
    return [];
  }
}

export function remember(path: string): void {
  const paths = [path, ...recentPaths().filter((other) => other !== path)].slice(0, LIMIT);
  try {
    localStorage.setItem(KEY, JSON.stringify(paths));
  } catch {
    // Storage unavailable; the list is only a convenience.
  }
}

export function forget(path: string): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(recentPaths().filter((other) => other !== path)));
  } catch {
    // As above.
  }
}
