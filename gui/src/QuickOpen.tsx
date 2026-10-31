// A modal file picker in the style of Neovim's Telescope: a prompt, the
// ranked matches, and a preview of the highlighted match beside them.

import { useEffect, useMemo, useRef, useState } from "react";
import * as monaco from "monaco-editor";
import { readFile, type FileSummary } from "./api";
import { rank } from "./fuzzy";
import { fileIcon } from "./icons";
import { languageOf } from "./languages";

/** Results shown at once. Arbitrary: enough to scan, few enough that one
 * keystroke in a 2,898-file project does not render thousands of rows. */
const MAX_RESULTS = 200;

/** Characters of a file the preview highlights. A minified bundle can be
 * megabytes on one line; the start is enough to tell files apart, and
 * colorizing all of it would stall every arrow-key press. */
const PREVIEW_CHARS = 20_000;

interface Props {
  files: FileSummary[];
  onChoose: (file: FileSummary) => void;
  onClose: () => void;
}

export function QuickOpen({ files, onChoose, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState(0);
  const [preview, setPreview] = useState<string>("");
  const input = useRef<HTMLInputElement>(null);
  const highlightedRow = useRef<HTMLDivElement>(null);

  const matches = useMemo(() => rank(files, query), [files, query]);
  const shown = matches.slice(0, MAX_RESULTS);
  const current = shown[highlighted];

  useEffect(() => input.current?.focus(), []);
  useEffect(() => setHighlighted(0), [query]);
  useEffect(() => highlightedRow.current?.scrollIntoView({ block: "nearest" }), [highlighted]);

  // Preview the highlighted file. A later highlight supersedes an earlier
  // read still in flight, so arrowing quickly never shows a stale file.
  useEffect(() => {
    if (!current) {
      setPreview("");
      return;
    }
    let cancelled = false;
    readFile(current.path).then(async (contents) => {
      if (cancelled) return;
      if (contents.text === null) {
        setPreview(`<span class="muted">Binary file, ${contents.size.toLocaleString()} bytes</span>`);
        return;
      }
      const text = contents.text.slice(0, PREVIEW_CHARS);
      const html = await monaco.editor.colorize(text, languageOf(current.path).monaco, {});
      if (!cancelled) setPreview(html);
    });
    return () => {
      cancelled = true;
    };
  }, [current?.path]);

  const onKey = (event: React.KeyboardEvent) => {
    const move = (step: number) => {
      event.preventDefault();
      if (shown.length > 0) setHighlighted((index) => (index + step + shown.length) % shown.length);
    };
    // Ctrl+N / Ctrl+P as in Telescope; arrows as everywhere else.
    if (event.key === "ArrowDown" || (event.ctrlKey && event.key === "n")) move(1);
    else if (event.key === "ArrowUp" || (event.ctrlKey && event.key === "p")) move(-1);
    else if (event.key === "Enter" && current) onChoose(current);
    else if (event.key === "Escape") onClose();
  };

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="picker" onMouseDown={(event) => event.stopPropagation()} onKeyDown={onKey}>
        <div className="picker-results">
          <div className="picker-title">
            Files <span className="muted">{matches.length.toLocaleString()}</span>
          </div>
          <div className="picker-list">
            {shown.map((file, index) => (
              <div
                key={file.path}
                ref={index === highlighted ? highlightedRow : undefined}
                className={`picker-row${index === highlighted ? " highlighted" : ""}`}
                onMouseMove={() => setHighlighted(index)}
                onClick={() => onChoose(file)}
              >
                <img className="icon" src={fileIcon(file.path)} alt="" draggable={false} />
                {file.path}
              </div>
            ))}
            {matches.length > MAX_RESULTS && (
              <div className="placeholder">
                {(matches.length - MAX_RESULTS).toLocaleString()} more; keep typing to narrow
              </div>
            )}
            {matches.length === 0 && <div className="placeholder">No file matches</div>}
          </div>
          <input
            ref={input}
            className="picker-prompt"
            placeholder="Find file"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            spellCheck={false}
          />
        </div>
        <div className="picker-preview">
          <div className="picker-title">{current?.path ?? "Preview"}</div>
          {/* Monaco's colorizer escapes the text and emits only spans. */}
          <pre className="picker-code" dangerouslySetInnerHTML={{ __html: preview }} />
        </div>
      </div>
    </div>
  );
}
