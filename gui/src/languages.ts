// What the viewer does with a file, decided by its extension. The project
// model has already renamed JavaScript modules to `.js` and stripped `.zst`,
// so the extension is reliable for bundled files as well as sources.

interface Language {
  /** Monaco's language id. */
  monaco: string;
  /** Prettier's parser, or undefined when Prettier cannot format it. */
  prettier?: string;
}

const byExtension: Record<string, Language> = {
  js: { monaco: "javascript", prettier: "babel" },
  mjs: { monaco: "javascript", prettier: "babel" },
  cjs: { monaco: "javascript", prettier: "babel" },
  jsx: { monaco: "javascript", prettier: "babel" },
  ts: { monaco: "typescript", prettier: "typescript" },
  mts: { monaco: "typescript", prettier: "typescript" },
  cts: { monaco: "typescript", prettier: "typescript" },
  tsx: { monaco: "typescript", prettier: "typescript" },
  json: { monaco: "json", prettier: "json" },
  css: { monaco: "css", prettier: "css" },
  html: { monaco: "html", prettier: "html" },
  md: { monaco: "markdown", prettier: "markdown" },
  sh: { monaco: "shell" },
  toml: { monaco: "ini" },
  yaml: { monaco: "yaml", prettier: undefined },
  yml: { monaco: "yaml", prettier: undefined },
};

export function languageOf(path: string): Language {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  return byExtension[extension] ?? { monaco: "plaintext" };
}
