# The Bun standalone executable format

A Bun single-file executable is a copy of the Bun runtime with a serialized
module graph attached. The graph is a blob of strings (module paths, source,
source maps, bytecode) followed by a table of fixed-size module entries that
point into the blob, an `Offsets` record that locates the table, and a 16-byte
trailer.

Everything below was read from Bun's `StandaloneModuleGraph` source at the
releases listed under [Sources](#sources), then checked against 74 binaries
built with Bun 1.0.0 through 1.4.4 for macOS arm64, Linux x64 and Windows x64
(`tools/bun-matrix`), plus one large production binary. Where the
source and the bytes disagreed, the bytes won; the one case is called out below.

## Layout

All integers are little-endian. A `StringPointer` is two `u32`s, `offset` then
`length`, with `offset` measured from the start of the payload (byte 0 of the
diagram below), not from the start of the file.

```text
            payload start                                        trailer end
            v                                                              v
  ... runtime | strings blob ...... | module table | extra | Offsets | trailer | [u64]
              0                     ^              ^       ^         ^
                                    |              |       |         "\n---- Bun! ----\n"
                                    |              |       24 or 32 bytes
                                    |              Bun >= 1.4 only, flag-gated
                                    modules_ptr.offset .. + modules_ptr.length

  byte_count (in Offsets) = everything from payload start up to Offsets
```

- **strings blob**: every module's name, contents, source map and bytecode,
  addressed by `StringPointer`s. Order is not guaranteed; Bun 1.4 has a flag
  saying the contents are contiguous, which implies earlier versions made no
  such promise.
- **module table**: `count = modules_ptr.length / entry_size` entries. The
  entry size is not recorded anywhere, which is the whole version-detection
  problem (see below).
- **extra**: Bun 1.4 appends optional tables after the module table (source
  hashes, builtin bytecode, a bytecode string table), each gated by a bit in
  `Offsets.flags`. A reader that only wants modules can ignore them, because
  `modules_ptr` gives the table's exact extent.
- **Offsets**: immediately before the trailer. See below.
- **trailer**: the 16 bytes `"\n---- Bun! ----\n"`.
- **`[u64]`**: when the payload is appended to the end of the file (see below),
  the file ends with a `u64` equal to the total file length, after the
  trailer. The parser below does not need it.

### Where the payload lives in the file

Each platform moved the payload from "appended to the file" into a section of
the executable at a different release. The detection method below does not
depend on this, so it is recorded for the GUI's binary map. Measured on the
test binaries; the exact release of each move lies in the bracketed range,
between the last version tested appended and the first tested in a section:

| Platform | Appended, then `u64` file length | In a section |
|---|---|---|
| macOS (Mach-O) | <= 1.2.0 | `__BUN,__bun` from 1.2.12 [1.2.1 – 1.2.12] |
| Windows (PE) | <= 1.2.12 | from 1.2.23 [1.2.13 – 1.2.23] |
| Linux (ELF) | <= 1.3.7 | from 1.3.14 [1.3.8 – 1.3.14] |

Inside a section, the payload is preceded by a `u64` holding
`byte_count + sizeof(Offsets) + 16` from Bun 1.3.7 on macOS and Windows and
from 1.3.14 on Linux; the earlier sectioned builds (macOS 1.2.12 – 1.3.0,
Windows 1.2.23 – 1.3.0) do not have it.

The trailer string also appears once inside the runtime itself as a literal,
so every binary measured contains it twice. Only one occurrence decodes.

## Offsets

| Field | Type | Present |
|---|---|---|
| `byte_count` | `u64` | always |
| `modules_ptr` | `StringPointer` | always |
| `entry_point_id` | `u32` | always; index into the module table |
| `compile_exec_argv_ptr` | `StringPointer` | Bun >= 1.2.23 (the exact release it appeared in is somewhere in 1.2.13 to 1.2.23) |
| `flags` | `u32` | Bun >= 1.3.7; before that these 4 bytes are padding |

Size is 24 bytes without `compile_exec_argv_ptr` (20 bytes of fields, padded
to 8-byte alignment) and 32 bytes with it. The arrival of `flags` did not
change the size because it took over the padding.

## Module entries

Four layouts, identified here by size:

| Size | Bun | Fields in memory order |
|---|---|---|
| 32 | 1.0.0 – 1.1.20 | `name`, `contents`, `sourcemap`, `loader: u8`, 7 bytes padding |
| 36 | 1.1.30 – 1.2.12 | `name`, `contents`, `sourcemap`, `bytecode`, `encoding: u8`, `loader: u8`, `module_format: u8`, 1 byte padding |
| 36 | 1.2.23 – 1.3.7 | as above, with `side: u8` in the padding byte |
| 52 | 1.3.14 – 1.4.4 | `name`, `contents`, `sourcemap`, `bytecode`, `module_info`, `bytecode_origin_path`, `encoding`, `loader`, `module_format`, `side` |

The 32-byte layout is the one place the source misleads. Bun 1.0–1.1 declares
`loader` second, but the struct is a plain Zig `struct`, whose field order the
compiler may change, and the bytes show `loader` last. Every pointer field
measured sits in declared order; for Bun < 1.4 that is an observation about the
Zig compiler of the day, not a guarantee. Bun 1.4's Rust port is `#[repr(C)]`,
so its order is fixed.

The two 36-byte variants decode identically if the fourth `u8` is read as
`side` and absent means 0 (`server`).

Name prefixes seen: `compiled://root/` (Bun 1.0.0), `/$bunfs/root/` (later,
Unix), `B:/~BUN/root/` (Windows).

## Version detection

There is no version field. The layout has to be inferred, and it can be,
structurally:

1. Find every occurrence of the trailer.
2. For each, and for each `Offsets` size (32, 24), read `Offsets` just before
   the trailer. Reject if `payload_start = offsets_pos - byte_count` is
   negative, if `modules_ptr` falls outside `[0, byte_count)`, or if
   `entry_point_id >= count`.
3. For each entry size (52, 36, 32) that divides `modules_ptr.length`, decode
   every entry. Reject if any `StringPointer` falls outside `[0, byte_count)` or
   any name is empty, over 4,096 bytes, not UTF-8, or contains a control
   character. No name prefix is required, so a release that changes the prefix
   still decodes.
4. Exactly one surviving `(offsets size, entry size)` is the answer. Zero means
   not a Bun executable, or a layout this table does not know. More than one is
   an ambiguity to report, never to guess through.

Over the 75 binaries above, step 4 found exactly one candidate every time.

`tools/probe.py` is the reference implementation of this procedure, written
to answer whether it works before any Rust was written; it checks names as
printable ASCII. `crates/pao-core/src/locate.rs` is the Rust one.

## Trade-offs

- **Structural inference instead of a version string.** The runtime does carry
  its version, and mapping version to layout would be explicit. It was rejected
  because it needs a table entry for every release, breaks on the first
  release not in the table, and cannot read a binary whose version string has
  been stripped or moved. Inference keeps working on a new Bun release as long
  as it reuses a known entry size. The cost is a theoretical ambiguity: a
  module table whose length is a multiple of both 32 and 36 (any multiple of
  288 bytes) could in principle decode both ways. Validating every entry makes
  that unlikely, but the test binaries hold only one or two modules each in
  the 32- and 36-byte layouts, so this has not been exercised. Only the
  production binary, in the 52-byte layout, tests a large table.
- **Trailer scan instead of parsing Mach-O, PE and ELF.** A scan over a
  file of a few hundred megabytes is one `memmem` pass and needs no per-platform code. Parsing the
  container would find the payload directly, and the GUI will want it anyway
  for the binary map, but it would add three parsers to the path that matters,
  each with its own history of when the payload moved (see the table above).
  That history is what broke the existing extractors: unbuned looks only in
  the `__BUN` section, so it fails on macOS builds from 1.2.0 and earlier.

## Open questions

- Enum values. `loader` maps to a different numbering in Bun 1.4 than the one
  other extractors hard-code: a text file embedded by 1.4.4 has loader `0x0d`,
  where those tools' tables say 13 is `sqlite`. Needs a per-version loader
  table read from the source.
- The production binary sets `Offsets.flags` bits above the ones defined at
  1.4.4. Unexplained.
- Bytecode. The pointer is decoded but the format is JavaScriptCore's, tied to
  the exact Bun version, and out of scope for now.

## Sources

`src/StandaloneModuleGraph.zig` (or `src/standalone_bun.zig` in 1.0.0,
`src/standalone_graph/StandaloneModuleGraph.zig` in 1.3.14,
`src/standalone_graph/StandaloneModuleGraph.rs` in 1.4.x) at:

| Release | Commit |
|---|---|
| 1.0.0 | `822a00c4d508` |
| 1.1.20 | `ae19489250ab` |
| 1.1.30 | `7996d06b8f58` |
| 1.2.12 | `0785effaab48` |
| 1.2.23 | `cf1367137da5` |
| 1.3.7 | `ba426210c28a` |
| 1.3.14 | `0d9b296af33f` |
| 1.4.4 | `38afee6c84fc` |
