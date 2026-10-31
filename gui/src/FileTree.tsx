// The `bundled/` and `sources/` trees as collapsible folders. Paths arrive
// flat and sorted from the backend; the nesting is rebuilt here. Finding a
// file by name is the picker's job (`QuickOpen.tsx`); the tree only reveals
// what the picker chose.

import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import type { FileSummary } from "./api";
import { fileIcon, folderIcon } from "./icons";

interface Folder {
  name: string;
  path: string;
  folders: Folder[];
  files: FileSummary[];
  /** Files at any depth below, shown beside the folder name. */
  count: number;
}

function buildTree(files: FileSummary[]): Folder {
  const root: Folder = { name: "", path: "", folders: [], files: [], count: 0 };
  for (const file of files) {
    const segments = file.path.split("/");
    let folder = root;
    folder.count++;
    for (const segment of segments.slice(0, -1)) {
      const path = folder.path ? `${folder.path}/${segment}` : segment;
      let next = folder.folders.find((child) => child.name === segment);
      if (!next) {
        next = { name: segment, path, folders: [], files: [], count: 0 };
        folder.folders.push(next);
      }
      folder = next;
      folder.count++;
    }
    folder.files.push(file);
  }
  return root;
}

const Icon = ({ src }: { src: string | undefined }) =>
  src ? <img className="icon" src={src} alt="" draggable={false} /> : <span className="icon" />;

export interface FileTreeHandle {
  /** Opens every folder above `file` and scrolls it into view. */
  reveal: (file: FileSummary) => void;
}

interface Props {
  files: FileSummary[];
  selected: string | null;
  onSelect: (file: FileSummary) => void;
}

export const FileTree = forwardRef<FileTreeHandle, Props>(function FileTree(
  { files, selected, onSelect },
  ref,
) {
  const root = useMemo(() => buildTree(files), [files]);
  // `sources/` is the readable tree, so it starts open; `bundled/` can hold
  // thousands of chunks and starts closed when sources exist.
  const hasSources = files.some((file) => file.tree === "sources");
  const [open, setOpen] = useState<Set<string>>(
    () => new Set(hasSources ? ["sources"] : ["bundled"]),
  );
  const selectedRow = useRef<HTMLDivElement>(null);

  useImperativeHandle(ref, () => ({
    reveal: (file) => {
      const folders = file.path.split("/").slice(0, -1);
      setOpen((current) => {
        const next = new Set(current);
        folders.forEach((_, index) => next.add(folders.slice(0, index + 1).join("/")));
        return next;
      });
    },
  }));

  // Runs after a reveal has opened the folders, so the row exists to scroll to.
  useEffect(() => selectedRow.current?.scrollIntoView({ block: "nearest" }), [selected, open]);

  const toggle = (path: string) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });

  const renderFolder = (folder: Folder, depth: number) => (
    <>
      {folder.folders.map((child) => (
        <div key={child.path}>
          <div
            className="tree-row folder"
            style={{ paddingLeft: depth * 12 + 8 }}
            onClick={() => toggle(child.path)}
          >
            <span className="twisty">{open.has(child.path) ? "▾" : "▸"}</span>
            <Icon src={folderIcon(child.name, open.has(child.path))} />
            {child.name}
            <span className="count">{child.count.toLocaleString()}</span>
          </div>
          {open.has(child.path) && renderFolder(child, depth + 1)}
        </div>
      ))}
      {folder.files.map((file) => {
        const isSelected = file.path === selected;
        return (
          <div
            key={file.path}
            ref={isSelected ? selectedRow : undefined}
            className={`tree-row file${isSelected ? " selected" : ""}`}
            style={{ paddingLeft: depth * 12 + 22 }}
            onClick={() => onSelect(file)}
            title={file.path}
          >
            <Icon src={fileIcon(file.path)} />
            {file.path.slice(file.path.lastIndexOf("/") + 1)}
          </div>
        );
      })}
    </>
  );

  return (
    <div className="tree-pane">
      <div className="tree-tools">
        <span className="muted">Files</span>
        <button title="Collapse all folders" onClick={() => setOpen(new Set())}>
          Collapse all
        </button>
      </div>
      <div className="tree">{renderFolder(root, 0)}</div>
    </div>
  );
});
