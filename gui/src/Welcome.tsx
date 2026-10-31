// The screen before an executable is open: what pao is, a drop zone, and
// the executables opened recently.

import logo from "../../crates/pao-gui/icons/logo.svg";
import { forget, recentPaths } from "./recent";
import { useState } from "react";

interface Props {
  onOpen: (path: string) => void;
  onBrowse: () => void;
  /** A file is being dragged over the window. */
  dragging: boolean;
  busy: boolean;
}

const splitPath = (path: string) => {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return { name: path.slice(cut + 1), directory: path.slice(0, cut) };
};

export function Welcome({ onOpen, onBrowse, dragging, busy }: Props) {
  const [recent, setRecent] = useState(recentPaths);

  return (
    <div className="welcome">
      <div className="welcome-card">
        <img className="welcome-logo" src={logo} alt="" draggable={false} />
        <h1>Pao</h1>
        <p className="welcome-tagline">Read the code inside Bun executables.</p>

        <button
          className={`dropzone${dragging ? " active" : ""}`}
          onClick={onBrowse}
          disabled={busy}
        >
          <span className="dropzone-title">
            {dragging ? "Drop to open" : "Drop an executable here"}
          </span>
          <span className="muted">
            or click to choose one <kbd>⌘O</kbd>
          </span>
        </button>

        {recent.length > 0 && (
          <div className="recent">
            <div className="recent-title">Recent</div>
            {recent.map((path) => {
              const { name, directory } = splitPath(path);
              return (
                <div key={path} className="recent-row" title={path}>
                  <button className="recent-open" onClick={() => onOpen(path)} disabled={busy}>
                    <span className="recent-name">{name}</span>
                    <span className="recent-dir">{directory}</span>
                  </button>
                  <button
                    className="recent-forget"
                    title="Remove from recent"
                    onClick={() => {
                      forget(path);
                      setRecent(recentPaths());
                    }}
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
        )}

        <p className="welcome-footer">
          Any <code>bun build --compile</code> executable from Bun 1.0 to 1.4, built for macOS,
          Linux or Windows. Original sources appear when it was built with{" "}
          <code>--sourcemap</code>.
        </p>
      </div>
    </div>
  );
}
