// Ranking for the file picker.

import type { FileSummary } from "./api";

const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/** Lower is better; null means no match. A file name starting with the query
 * beats one containing it, which beats a path containing it, which beats the
 * query's letters appearing in order anywhere in the path ("clijs" finds
 * `cli.js`). */
function score(path: string, query: string): number | null {
  const lowerPath = path.toLowerCase();
  const name = baseName(lowerPath);
  if (name.startsWith(query)) return 0;
  if (name.includes(query)) return 1;
  if (lowerPath.includes(query)) return 2;
  let at = 0;
  for (const char of query) {
    at = lowerPath.indexOf(char, at);
    if (at < 0) return null;
    at++;
  }
  return 3;
}

/** `files` matching `query`, best first; ties go to the shorter path. With
 * no query, every file, original sources before bundled code. */
export function rank(files: FileSummary[], query: string): FileSummary[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return [...files].sort((a, b) =>
      a.tree === b.tree ? a.path.localeCompare(b.path) : a.tree === "sources" ? -1 : 1,
    );
  }
  return files
    .map((file) => ({ file, score: score(file.path, needle) }))
    .filter((match): match is { file: FileSummary; score: number } => match.score !== null)
    .sort((a, b) => a.score - b.score || a.file.path.length - b.file.path.length)
    .map((match) => match.file);
}
