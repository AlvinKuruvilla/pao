// The Tauri commands in crates/pao-gui/src/main.rs, typed. The shapes mirror
// the Rust structs there field for field; serde writes them in snake_case.

import { invoke } from "@tauri-apps/api/core";

export type Tree = "bundled" | "sources";

export interface FileSummary {
  /** `/`-separated, starting `bundled/` or `sources/`. */
  path: string;
  tree: Tree;
  /** Index into `OpenedBinary.modules`. */
  module: number;
  size: number;
  /** Valid UTF-8 with no NUL bytes, so the viewer can show it. */
  text: boolean;
}

export interface ModuleSummary {
  name: string;
  /** Named for the release that built the binary, or `"value N"` when that
   * release does not define the value. */
  loader: string;
  module_format: string | null;
  encoding: string | null;
  side: string | null;
  /** File offset of the module's contents, for a hex editor. */
  contents_offset: number;
  contents_size: number;
  sourcemap_size: number;
  /** `"688 bytes"`, `"entry at 0x…"` for linked bytecode, or null. */
  bytecode: string | null;
}

export interface OpenedBinary {
  path: string;
  file_name: string;
  bun_version: string;
  layout: string;
  entry_point: number;
  modules: ModuleSummary[];
  files: FileSummary[];
  warnings: string[];
}

export interface FileContents {
  /** Null for binary files. */
  text: string | null;
  size: number;
}

export const openBinary = (path: string) => invoke<OpenedBinary>("open_binary", { path });

export const readFile = (path: string) => invoke<FileContents>("read_file", { path });

/** Writes every file under `directory`, which must be empty or absent.
 * Resolves to the number of files written. */
export const exportTo = (directory: string) => invoke<number>("export_to", { directory });
