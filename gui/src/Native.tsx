// A native addon's headers, in place of the "binary file" placeholder: the
// same facts `pao inspect` prints under a napi row.

import type { ReactNode } from "react";
import type { FileSummary, NativeImage, NativeSummary } from "./api";

export function Native({ file, native }: { file: FileSummary; native: NativeSummary }) {
  return (
    <div className="viewer">
      <div className="viewer-bar">
        <span className="viewer-path">{file.path}</span>
        <span className="muted">
          Native addon, {file.size.toLocaleString()} bytes
        </span>
      </div>
      {native.state === "unreadable" ? (
        <div className="placeholder">Its headers did not read: {native.error}</div>
      ) : (
        <div className="native">
          <table>
            <tbody>
              <tr>
                <th>Format</th>
                <td>{native.format}</td>
              </tr>
              <tr>
                <th>Architectures</th>
                <td>{native.images.map((image) => image.architecture).join(", ")}</td>
              </tr>
              <Field
                label="Name"
                images={native.images}
                data={(image) => image.own_name}
                render={(image) => image.own_name}
              />
              <Field
                label="Exports"
                images={native.images}
                data={(image) => image.node_api_exports}
                render={exports}
              />
              <Field
                label="Links"
                images={native.images}
                data={(image) => image.libraries}
                render={links}
              />
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** One row when every slice agrees on `data`, which is the usual case;
 * otherwise one per slice, labelled with its architecture. Slices of one
 * build often differ only in a target triple inside the build path. */
function Field({
  label,
  images,
  data,
  render,
}: {
  label: string;
  images: NativeImage[];
  /** What slices are compared by: plain data, so `JSON.stringify` is exact. */
  data: (image: NativeImage) => unknown;
  render: (image: NativeImage) => ReactNode;
}) {
  const keys = images.map((image) => JSON.stringify(data(image)));
  if (keys.every((key) => key === keys[0])) {
    const shown = images.length > 0 ? render(images[0]) : null;
    if (shown === null) return null;
    return (
      <tr>
        <th>{label}</th>
        <td>{shown}</td>
      </tr>
    );
  }
  return (
    <>
      {images.map((image, index) => (
        <tr key={image.architecture}>
          <th>{index === 0 ? label : ""}</th>
          <td>
            <span className="muted">{image.architecture}</span> {render(image) ?? "none"}
          </td>
        </tr>
      ))}
    </>
  );
}

function exports(image: NativeImage) {
  if (image.node_api_exports.length === 0) {
    return <span className="native-warning">No Node-API entry points</span>;
  }
  return image.node_api_exports.join(", ");
}

function links(image: NativeImage) {
  if (image.libraries.length === 0) return null;
  return (
    <ul className="native-links">
      {image.libraries.map((library) => (
        <li key={library.name} title={library.name}>
          {library.short_name}
          {library.kind && <span className="muted"> {library.kind}</span>}
        </li>
      ))}
    </ul>
  );
}
