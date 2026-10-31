import { createRequire } from "node:module";
import { defineConfig, searchForWorkspaceRoot, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// The Material Icon Theme's lookup tables, as the VS Code extension builds
// them: file name, extension and folder name to icon name. The package only
// generates them in Node, so they are computed here at build time and served
// to the app as `virtual:material-icons`. The SVGs themselves are bundled by
// `src/icons.ts`.
function materialIcons(): Plugin {
  const id = "virtual:material-icons";
  const resolved = "\0" + id;
  return {
    name: "material-icons",
    resolveId: (source) => (source === id ? resolved : undefined),
    load(source) {
      if (source !== resolved) return;
      const { generateManifest } = createRequire(import.meta.url)("material-icon-theme");
      const manifest = generateManifest();
      const tables = {
        file: manifest.file,
        folder: manifest.folder,
        folderExpanded: manifest.folderExpanded,
        fileNames: manifest.fileNames,
        fileExtensions: manifest.fileExtensions,
        folderNames: manifest.folderNames,
        folderNamesExpanded: manifest.folderNamesExpanded,
        languageIds: manifest.languageIds,
      };
      return `export default ${JSON.stringify(tables)};`;
    },
  };
}

// Tauri loads the dev server at a fixed URL (`devUrl` in
// crates/pao-gui/tauri.conf.json), so the port must not drift to the next
// free one.
export default defineConfig({
  plugins: [react(), materialIcons()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // The welcome screen shows the app icon's source, which lives with the
    // Tauri crate so `cargo tauri icon` and the frontend share one file.
    fs: { allow: [searchForWorkspaceRoot(process.cwd()), "../crates/pao-gui/icons"] },
  },
  // Monaco alone is about 4 MB. The bundle is read from disk by the desktop
  // app, never downloaded, so Vite's 500 kB warning does not apply.
  build: {
    outDir: "dist",
    target: "es2022",
    chunkSizeWarningLimit: 6000,
    // Most icons are under Vite's 4 kB inlining limit. Inlined, all 1,256
    // would add 1.5 MB to the main bundle, parsed at startup; as separate
    // files, only the ones on screen are ever read.
    assetsInlineLimit: (file) => (file.includes("material-icon-theme") ? false : undefined),
  },
  worker: { format: "es" },
});
