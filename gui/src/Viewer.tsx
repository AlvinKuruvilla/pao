// One file, read-only in Monaco, formatted with Prettier when that is quick.

import { useEffect, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import { readFile, type FileContents, type FileSummary } from "./api";
import { languageOf } from "./languages";
import type { FormatRequest, FormatResponse } from "./format.worker";
import FormatWorker from "./format.worker?worker";

/** Files up to this size are formatted as soon as they open. Measured: about
 * a second at 1 MB, so larger files wait for the button. Arbitrary within
 * that: lower means more clicks, higher means a longer wait on open. */
const AUTO_FORMAT_LIMIT = 1_000_000;

type Formatting =
  | { state: "none" }
  | { state: "running" }
  | { state: "done"; text: string }
  | { state: "failed"; error: string };

export function Viewer({ file }: { file: FileSummary }) {
  const [contents, setContents] = useState<FileContents | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [formatting, setFormatting] = useState<Formatting>({ state: "none" });
  const [showFormatted, setShowFormatted] = useState(true);
  const worker = useRef<Worker | null>(null);
  const requestId = useRef(0);

  const language = languageOf(file.path);

  const format = (text: string) => {
    if (!language.prettier) return;
    worker.current ??= new FormatWorker();
    // A response for a file that is no longer open is dropped by id.
    const id = ++requestId.current;
    setFormatting({ state: "running" });
    worker.current.onmessage = (event: MessageEvent<FormatResponse>) => {
      const response = event.data;
      if (response.id !== requestId.current) return;
      setFormatting(
        response.ok
          ? { state: "done", text: response.text }
          : { state: "failed", error: response.error },
      );
    };
    const request: FormatRequest = { id, text, parser: language.prettier };
    worker.current.postMessage(request);
  };

  useEffect(() => {
    let cancelled = false;
    requestId.current++;
    setContents(null);
    setLoadError(null);
    setFormatting({ state: "none" });
    setShowFormatted(true);
    readFile(file.path)
      .then((loaded) => {
        if (cancelled) return;
        setContents(loaded);
        if (loaded.text !== null && loaded.size <= AUTO_FORMAT_LIMIT) format(loaded.text);
      })
      .catch((error) => !cancelled && setLoadError(String(error)));
    return () => {
      cancelled = true;
    };
    // `format` is left out of the dependencies: it changes every render, and
    // everything it reads is derived from `file.path`.
  }, [file.path]);

  // A worker stuck on a 5 MB file would keep formatting after the viewer
  // goes away.
  useEffect(() => () => worker.current?.terminate(), []);

  if (loadError) return <div className="placeholder">Could not read {file.path}: {loadError}</div>;
  if (!contents) return <div className="placeholder">Loading…</div>;
  if (contents.text === null) {
    return (
      <div className="placeholder">
        Binary file, {contents.size.toLocaleString()} bytes. Export to open it in another tool.
      </div>
    );
  }

  const shown =
    showFormatted && formatting.state === "done" ? formatting.text : contents.text;

  return (
    <div className="viewer">
      <div className="viewer-bar">
        <span className="viewer-path">{file.path}</span>
        {language.prettier && formatting.state === "none" && (
          <button onClick={() => format(contents.text!)}>
            Format ({(contents.size / 1e6).toFixed(1)} MB, may take a few seconds)
          </button>
        )}
        {formatting.state === "running" && <span className="muted">Formatting…</span>}
        {formatting.state === "failed" && (
          <span className="muted" title={formatting.error}>
            Prettier could not parse this file; showing it as stored
          </span>
        )}
        {formatting.state === "done" && (
          <button onClick={() => setShowFormatted((value) => !value)}>
            {showFormatted ? "Show original" : "Show formatted"}
          </button>
        )}
      </div>
      <Editor
        height="100%"
        theme="vs-dark"
        path={file.path}
        language={language.monaco}
        value={shown}
        options={{
          readOnly: true,
          domReadOnly: true,
          minimap: { enabled: true },
          wordWrap: "off",
          scrollBeyondLastLine: false,
          // Minified lines run to megabytes; Monaco stops tokenizing a line
          // past this length and renders it plain, which keeps it responsive.
          maxTokenizationLineLength: 20_000,
        }}
      />
    </div>
  );
}
