// What `pao inspect` says about the module a file came from.

import type { FileSummary, OpenedBinary } from "./api";

export function Details({ binary, file }: { binary: OpenedBinary; file: FileSummary | null }) {
  const module = file ? binary.modules[file.module] : null;
  const rows: [string, string][] = [
    ["Bun", binary.bun_version],
    ["Layout", binary.layout],
    ["Modules", binary.modules.length.toLocaleString()],
    ["Entry point", binary.modules[binary.entry_point]?.name ?? "?"],
  ];
  if (file && module) {
    rows.push(
      ["", ""],
      [file.tree === "sources" ? "Listed by" : "Module", module.name],
      ["Loader", module.loader],
    );
    if (module.module_format) rows.push(["Format", module.module_format]);
    if (module.encoding) rows.push(["Encoding", module.encoding]);
    if (module.side) rows.push(["Side", module.side]);
    rows.push(
      ["Offset", `0x${module.contents_offset.toString(16)}`],
      ["Stored size", `${module.contents_size.toLocaleString()} bytes`],
    );
    if (module.sourcemap_size > 0) {
      rows.push(["Source map", `${module.sourcemap_size.toLocaleString()} bytes`]);
    }
    if (module.bytecode) rows.push(["Bytecode", module.bytecode]);
  }
  return (
    <div className="details">
      <table>
        <tbody>
          {rows.map(([label, value], index) => (
            <tr key={index}>
              <th>{label}</th>
              <td>{value}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {binary.warnings.length > 0 && (
        <div className="warnings">
          <h4>Kept as stored</h4>
          {binary.warnings.map((warning) => (
            <div key={warning}>{warning}</div>
          ))}
        </div>
      )}
    </div>
  );
}
