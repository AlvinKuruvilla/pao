// File and folder icons from the Material Icon Theme (MIT), resolved the way
// VS Code resolves an icon theme: an exact file name first, then the longest
// matching extension, so `foo.d.ts` gets the declaration icon rather than
// TypeScript's, then the Monaco language, then the default.

import tables from "virtual:material-icons";
import { languageOf } from "./languages";

// Every SVG becomes a bundled asset URL. Eager only builds the URL table; a
// browser fetches an SVG from disk the first time an <img> shows it.
const svgs = import.meta.glob<string>("/node_modules/material-icon-theme/icons/*.svg", {
  query: "?url",
  import: "default",
  eager: true,
});
const urlOf = (icon: string): string | undefined =>
  svgs[`/node_modules/material-icon-theme/icons/${icon}.svg`];

export function fileIcon(path: string): string | undefined {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
  const byName = tables.fileNames[name];
  if (byName) return urlOf(byName);
  // `a.b.c` tries `b.c`, then `c`.
  for (let dot = name.indexOf("."); dot >= 0; dot = name.indexOf(".", dot + 1)) {
    const icon = tables.fileExtensions[name.slice(dot + 1)];
    if (icon) return urlOf(icon);
  }
  const byLanguage = tables.languageIds[languageOf(path).monaco];
  return urlOf(byLanguage ?? tables.file);
}

export function folderIcon(name: string, expanded: boolean): string | undefined {
  const lower = name.toLowerCase();
  const icon = expanded
    ? (tables.folderNamesExpanded[lower] ?? tables.folderExpanded)
    : (tables.folderNames[lower] ?? tables.folder);
  return urlOf(icon);
}
