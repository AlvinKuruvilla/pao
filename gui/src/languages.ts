// What the viewer does with a file, decided by its extension. The project
// model has already renamed JavaScript modules to `.js` and stripped `.zst`,
// so the extension is reliable for bundled files as well as sources.

import * as monaco from "monaco-editor";

interface Language {
  /** Monaco's language id. */
  monaco: string;
  /** Prettier's parser, or undefined when Prettier cannot format it. */
  prettier?: string;
}

// The languages Prettier formats, plus extensions Monaco's own registry
// lacks or maps to something else. Anything not here falls back to that
// registry, which is where highlighting for Rust, Go, SQL and the rest of
// Monaco's 84 language definitions comes from.
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
  // Prettier has no Python parser, so these are highlighted only. Monaco
  // registers `.py` and `.pyw` but not type stubs.
  py: { monaco: "python" },
  pyw: { monaco: "python" },
  pyi: { monaco: "python" },
  sh: { monaco: "shell" },
  toml: { monaco: "ini" },
};

/** Monaco's registered languages by extension, built on first use. */
let registry: Map<string, string> | undefined;

function registeredLanguage(extension: string): string | undefined {
  if (!registry) {
    registry = new Map();
    for (const language of monaco.languages.getLanguages()) {
      for (const registered of language.extensions ?? []) {
        const key = registered.replace(/^\./, "").toLowerCase();
        // The first language to claim an extension keeps it.
        if (!registry.has(key)) registry.set(key, language.id);
      }
    }
  }
  return registry.get(extension);
}

export function languageOf(path: string): Language {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  const known = byExtension[extension];
  if (known) return known;
  const registered = extension ? registeredLanguage(extension) : undefined;
  return { monaco: registered ?? "plaintext" };
}
