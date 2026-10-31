import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { exportTo, openBinary, type FileSummary, type OpenedBinary } from "./api";
import { FileTree, type FileTreeHandle } from "./FileTree";
import { Viewer } from "./Viewer";
import { Details } from "./Details";
import { QuickOpen } from "./QuickOpen";
import { Welcome } from "./Welcome";
import { remember } from "./recent";

const isMac = navigator.platform.startsWith("Mac");

/** Whether the module details panel is shown, remembered across launches.
 * A convenience, so unavailable storage just means it starts shown. */
const DETAILS_KEY = "pao.showDetails";
const storedShowDetails = () => {
  try {
    return localStorage.getItem(DETAILS_KEY) !== "false";
  } catch {
    return true;
  }
};

export function App() {
  const [binary, setBinary] = useState<OpenedBinary | null>(null);
  const [selected, setSelected] = useState<FileSummary | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [showDetails, setShowDetails] = useState(storedShowDetails);
  const tree = useRef<FileTreeHandle>(null);

  const toggleDetails = () =>
    setShowDetails((shown) => {
      try {
        localStorage.setItem(DETAILS_KEY, String(!shown));
      } catch {
        // Storage unavailable; the panel still toggles for this session.
      }
      return !shown;
    });

  const openPath = async (path: string) => {
    setBusy(true);
    setStatus(`Opening ${path}…`);
    try {
      const opened = await openBinary(path);
      remember(path);
      setBinary(opened);
      setPicking(false);
      // Start on the entry point's original source if there is one, else
      // its bundled code: the file a reader most likely wants first.
      const entry = opened.files.filter((file) => file.module === opened.entry_point);
      const first = entry.find((file) => file.tree === "sources") ?? entry[0] ?? null;
      setSelected(first);
      // The tree for this binary mounts on this render, so reveal after it.
      if (first) requestAnimationFrame(() => tree.current?.reveal(first));
      setStatus(null);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  };

  const browse = async () => {
    const path = await open({ multiple: false, directory: false, title: "Open a Bun executable" });
    if (typeof path === "string") await openPath(path);
  };

  // Shortcuts, caught in the capture phase so they work while Monaco has
  // focus. Cmd+P (Ctrl+P off macOS) toggles the file picker; inside the
  // picker Ctrl+P moves the highlight instead, Telescope's binding, so on
  // macOS only Cmd+P closes it. Cmd+O opens an executable. Cmd+Option+B
  // toggles the details panel, VS Code's key for its secondary side bar.
  // That one is matched on `code`, the physical key, because with Option
  // held macOS reports the character Option produces (`∫` for B); the
  // others match the typed letter, so they follow the keyboard layout.
  const browseRef = useRef(browse);
  browseRef.current = browse;
  const toggleDetailsRef = useRef(toggleDetails);
  toggleDetailsRef.current = toggleDetails;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(isMac ? event.metaKey : event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      if (event.altKey && event.code === "KeyB") toggleDetailsRef.current();
      else if (event.altKey) return;
      else if (key === "p") setPicking((current) => !current);
      else if (key === "o") void browseRef.current();
      else return;
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  // A file dropped anywhere on the window opens. The webview reports the
  // drop with real paths, which a browser drop event would not give.
  const openPathRef = useRef(openPath);
  openPathRef.current = openPath;
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") setDragging(true);
      else setDragging(false);
      if (payload.type === "drop" && payload.paths.length > 0) void openPathRef.current(payload.paths[0]);
    });
    return () => void unlisten.then((stop) => stop());
  }, []);

  const show = (file: FileSummary) => {
    tree.current?.reveal(file);
    setSelected(file);
  };

  const chooseExport = async () => {
    const directory = await open({ directory: true, title: "Export into an empty folder" });
    if (typeof directory !== "string") return;
    setBusy(true);
    try {
      const written = await exportTo(directory);
      setStatus(`Wrote ${written.toLocaleString()} files to ${directory}`);
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  };

  if (!binary) {
    return (
      <div className="app">
        <Welcome onOpen={openPath} onBrowse={browse} dragging={dragging} busy={busy} />
        {status && <div className="welcome-status">{status}</div>}
      </div>
    );
  }

  return (
    <div className="app">
      <header className="toolbar">
        <button onClick={browse} disabled={busy} title="Open an executable (⌘O)">
          Open…
        </button>
        <button onClick={chooseExport} disabled={busy}>
          Export…
        </button>
        <button onClick={() => setPicking(true)} title="Find a file (⌘P)">
          Find file…
        </button>
        <span className="title">
          {binary.file_name} <span className="muted">Bun {binary.bun_version}</span>
        </span>
        {status && <span className="status">{status}</span>}
        <button
          className={`toggle${showDetails ? " on" : ""}`}
          onClick={toggleDetails}
          aria-pressed={showDetails}
          title={`${showDetails ? "Hide" : "Show"} module details (⌥⌘B)`}
        >
          Details
        </button>
      </header>
      <main
        className={`panes${dragging ? " dragging" : ""}${showDetails ? "" : " no-details"}`}
      >
        <aside className="sidebar">
          <FileTree
            ref={tree}
            // A new executable starts with a fresh tree: open folders from
            // the last one mean nothing here.
            key={binary.path}
            files={binary.files}
            selected={selected?.path ?? null}
            onSelect={setSelected}
          />
        </aside>
        <section className="content">
          {selected ? <Viewer file={selected} /> : <div className="placeholder">Select a file</div>}
        </section>
        {showDetails && (
          <aside className="inspector">
            <Details binary={binary} file={selected} />
          </aside>
        )}
      </main>
      {picking && (
        <QuickOpen
          files={binary.files}
          onChoose={(file) => {
            setPicking(false);
            show(file);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </div>
  );
}
