# Bun standalone executable format

This document specifies the data that `bun build --compile` embeds in a
single-file executable, for Bun 1.0.0 through 1.4.4, and a procedure for
reading it without knowing which Bun release produced the file.

Bun publishes no specification of this format. Everything here was derived
from Bun's source at the commits listed in [References](#14-references) and
then checked against executables built by those releases ([Evidence](#11-evidence)).
Where the two disagreed, the bytes are what is specified, and the
disagreement is noted.

## Contents

1. [Conventions](#1-conventions)
2. [File structure](#2-file-structure)
3. [Payload placement](#3-payload-placement)
4. [Trailer](#4-trailer)
5. [Offsets record](#5-offsets-record)
6. [Module table](#6-module-table)
7. [Field values](#7-field-values)
8. [Bun 1.4 extensions](#8-bun-14-extensions)
9. [Reading procedure](#9-reading-procedure)
10. [Identifying the Bun release](#10-identifying-the-bun-release)
11. [Evidence](#11-evidence)
12. [Rationale](#12-rationale)
13. [Open questions](#13-open-questions)
14. [References](#14-references)

## 1. Conventions

- All integers are unsigned and little-endian: `u8`, `u32`, `u64`.
- A **`StringPointer`** is 8 bytes, `offset: u32` then `length: u32`, naming
  the bytes `offset .. offset + length` of the payload. Offsets in a
  `StringPointer` count from **payload byte 0**, never from the start of the
  file. A `StringPointer` with `length` 0 names nothing.
- **File offset** means a position in the executable file. **Payload
  offset** means a position counted from payload byte 0. The two differ by
  `base`, the file offset of payload byte 0 (section 9).
- Release ranges such as "1.2.23 – 1.3.7" name the earliest and latest
  *tested* releases with a property. An untested release between two ranges
  could behave like either neighbour.
- Byte diagrams put offsets on a ruler above each record. Offsets are
  relative to the start of the record and given in decimal.

## 2. File structure

An executable is a copy of the Bun runtime with a payload attached. The
payload is a single blob holding every module's bytes, a table describing
the modules, and, from Bun 1.4, further flag-gated records. It ends with an
`Offsets` record and a fixed 16-byte trailer, in that order:

```text
 payload offset 0                                              byte_count
 v                                                             v
 +--------------------+-------------+-------------------------+---------+---------+
 | data               | module      | extension records       | Offsets | trailer |
 | (names, contents,  | table       | (Bun >= 1.4 only,       | 24 or   | 16      |
 |  source maps,      |             |  gated by flags)        | 32      | bytes   |
 |  bytecode)         |             |                         | bytes   |         |
 +--------------------+-------------+-------------------------+---------+---------+
                      ^             ^
                      |             modules_ptr.offset + modules_ptr.length
                      modules_ptr.offset
```

- **data**: the bytes every `StringPointer` refers to. No order is promised.
  Bun 1.4 has a flag, `SOURCE_TEXT_CONTIGUOUS`, that promises all module
  contents form one run, which implies earlier releases did not.
- **module table**: one fixed-size entry per module (section 6). Its extent
  is exactly `modules_ptr`, so a reader that only wants modules can ignore
  everything between the table and `Offsets`.
- **extension records**: Bun 1.4 only. Present or absent according to bits
  in `Offsets.flags` (section 8).
- **Offsets**: locates the module table and gives the payload's length,
  `byte_count`, which counts everything from payload byte 0 up to the start
  of `Offsets` (section 5).
- **trailer**: identifies the end of the payload (section 4).

## 3. Payload placement

Where the payload sits in the file depends on the platform and the release.
Each platform began by appending it to the end of the file and later moved it
into the executable image:

```text
Appended

 0                                                                    EOF
 +--------------------------+---------+---------+---------+-----------+
 | runtime image            | payload | Offsets | trailer | file size |
 | (Mach-O, PE or ELF)      |         |         |         | u64       |
 +--------------------------+---------+---------+---------+-----------+
                                                          ^
                                         equals the total file length

Inside the executable image

 0                                                                    EOF
 +--------------+-----------+---------+---------+---------+-----------+
 | runtime      | length    | payload | Offsets | trailer | runtime   |
 | image ...    | u64       |         |         |         | image ... |
 +--------------+-----------+---------+---------+---------+-----------+
                ^
                byte_count + size of Offsets + 16; present only from the
                releases in the last column below
```

| Platform | Appended | Inside the image | Length header from |
|---|---|---|---|
| macOS (Mach-O) | ≤ 1.2.0 | `__BUN,__bun` section, ≥ 1.2.12 | 1.3.7 |
| Windows (PE) | ≤ 1.2.12 | `.bun` section, ≥ 1.2.23 | 1.3.7 |
| Linux (ELF) | ≤ 1.3.7 | `.bun` section, ≥ 1.3.14 | 1.3.14 |

The move happened at an untested release inside each gap: macOS 1.2.1 –
1.2.12, Windows 1.2.13 – 1.2.23, Linux 1.3.8 – 1.3.14. Sectioned builds
before the "length header" release (macOS 1.2.12 – 1.3.0, Windows 1.2.23 –
1.3.0) have no length header.

The reading procedure in section 9 does not depend on placement. It is
specified here for tools that map the file, and because it explains why
readers that look only in one place fail on other releases.

## 4. Trailer

The payload ends with these 16 bytes:

```text
 0                                                              16
 +----+----+----+----+----+----+----+----+----+----+----+----+----+----+----+----+
 | \n |  - |  - |  - |  - |    |  B |  u |  n |  ! |    |  - |  - |  - |  - | \n |
 +----+----+----+----+----+----+----+----+----+----+----+----+----+----+----+----+
```

The string `"\n---- Bun! ----\n"` also occurs as a literal inside the
runtime image, so every executable contains it at least twice. Only one
occurrence ends a payload; a reader must not assume the first or the last
one found is it (section 9).

## 5. Offsets record

The `Offsets` record immediately precedes the trailer. It has two sizes:

```text
24 bytes, Bun 1.0.0 – 1.2.12

 0                       8           12          16          20          24
 +-----------------------+-----------+-----------+-----------+-----------+
 | byte_count            | modules_ptr           | entry_    | padding   |
 | u64                   | offset    | length    | point_id  |           |
 +-----------------------+-----------+-----------+-----------+-----------+

32 bytes, Bun 1.2.23 – 1.4.4

 0                       8           12          16          20          24          28          32
 +-----------------------+-----------+-----------+-----------+-----------+-----------+-----------+
 | byte_count            | modules_ptr           | entry_    | compile_exec_argv_ptr | flags     |
 | u64                   | offset    | length    | point_id  | offset    | length    | u32       |
 +-----------------------+-----------+-----------+-----------+-----------+-----------+-----------+
```

| Field | Type | Meaning |
|---|---|---|
| `byte_count` | `u64` | Length of the payload: everything from payload byte 0 up to, not including, `Offsets`. |
| `modules_ptr` | `StringPointer` | The module table. |
| `entry_point_id` | `u32` | Index into the module table of the module run at startup. |
| `compile_exec_argv_ptr` | `StringPointer` | Arguments given to `--compile-exec-argv`. Added in an untested release between 1.2.13 and 1.2.23. |
| `flags` | `u32` | Bit set from Bun 1.3.3; padding before, zero in every build tested. Section 8.1 lists the bits and the release that defined each. |

The first 20 bytes are identical in both sizes. The 32-byte record arrived
with `compile_exec_argv_ptr`; `flags` later took over what had been padding,
so the size did not change again.

## 6. Module table

The module table is `count = modules_ptr.length / entry size` entries, with
no header. The entry size is recorded nowhere in the file, which is why the
layout has to be inferred (section 9). There are four layouts:

| Layout | Bun | Offsets | Entry |
|---|---|---|---|
| V1_0 | 1.0.0 – 1.1.20 | 24 | 32 |
| V1_1_30 | 1.1.30 – 1.2.12 | 24 | 36 |
| V1_2_23 | 1.2.23 – 1.3.7 | 32 | 36 |
| V1_3_14 | 1.3.14 – 1.4.4 | 32 | 52 |

Each `StringPointer` in an entry occupies 8 bytes; single-byte fields follow
the last pointer.

```text
V1_0, 32 bytes

 0           8           16          24 25                       32
 +-----------+-----------+-----------+--+------------------------+
 | name      | contents  | sourcemap |L | padding (7)            |
 +-----------+-----------+-----------+--+------------------------+

V1_1_30 and V1_2_23, 36 bytes

 0           8           16          24          32 33 34 35 36
 +-----------+-----------+-----------+-----------+--+--+--+--+
 | name      | contents  | sourcemap | bytecode  |E |L |F |S |
 +-----------+-----------+-----------+-----------+--+--+--+--+

V1_3_14, 52 bytes

 0           8           16          24          32          40          48 49 50 51 52
 +-----------+-----------+-----------+-----------+-----------+-----------+--+--+--+--+
 | name      | contents  | sourcemap | bytecode  | module_   | bytecode_ |E |L |F |S |
 |           |           |           |           | info      | origin_   |  |  |  |  |
 |           |           |           |           |           | path      |  |  |  |  |
 +-----------+-----------+-----------+-----------+-----------+-----------+--+--+--+--+

 E encoding   L loader   F module_format   S side (V1_2_23, V1_3_14; padding in V1_1_30)
```

| Field | Meaning |
|---|---|
| `name` | The module's path under Bun's virtual root (below). |
| `contents` | The module's bytes: source text, or the file itself for assets. |
| `sourcemap` | A serialized source map; length 0 if none. |
| `bytecode` | JavaScriptCore bytecode; length 0 if none. In Bun 1.4 its meaning depends on a flag (section 8.3). |
| `module_info` | Bun 1.3.14+; a record JavaScriptCore reads when loading ES module bytecode. Not specified further here. |
| `bytecode_origin_path` | Bun 1.3.14+; the path the bytecode was compiled against, which must match at run time for the bytecode to be used. |
| `encoding`, `loader`, `module_format`, `side` | Section 7. |

The V1_0 layout is the one place Bun's source misleads. Bun 1.0 and 1.1
declare `loader` second, but the record is a plain Zig `struct`, whose field
order the compiler is free to change, and the bytes have `loader` after the
three pointers. In every layout before 1.4 the pointer fields were found in
declared order; that is an observation about the Zig compiler of the time,
not a guarantee. Bun 1.4's Rust port declares the record `#[repr(C)]`, so its
order is fixed.

The two 36-byte layouts differ only in byte 35. A reader that treats it as
`side` and reads an absent `side` as 0 (`server`) decodes both alike.

Module names start with one of these prefixes:

| Prefix | Seen in |
|---|---|
| `compiled://root/` | Bun 1.0.0 |
| `/$bunfs/root/` | Bun 1.0.36 and later, macOS and Linux |
| `B:/~BUN/root/` | Windows |

## 7. Field values

### 7.1 Loader

`loader` is the `u8` value of Bun's `Loader` enum, which decides how Bun
treats the module's bytes. The enum was read at all 127 Bun 1.x release
tags. Most releases appended variants; two inserted one in the middle,
moving every loader above it up by one:

| Release | Change | Commit |
|---|---|---|
| 1.0.0 | `jsx` … `text`, values 0 – 12 | `822a00c4d508` |
| 1.0.23 | appended `sqlite`, `sqlite_embedded` | `4ccf6fb28492` |
| 1.0.24 | **inserted `bunsh` at 13** | `d4794bbac82c` |
| 1.1.43 | appended `html` | `76800b049a70` |
| 1.2.5 | **inserted `jsonc` at 7** | `013fdddc6ed1` |
| 1.2.21 | appended `yaml` | `7c45ed97def1` |
| 1.3.7 | appended `json5` | `ba426210c28a` |
| 1.3.8 | appended `md` | `b64edcb490b4` |
| 1.4.0 | appended `xml` | `34cbb9a40b4b` |

That gives three numberings. A release defines a prefix of one of them, up
to its last appended variant:

| Value | 1.0.0 – 1.0.23 | 1.0.24 – 1.2.4 | 1.2.5 – 1.4.4 |
|---:|---|---|---|
| 0 – 6 | jsx, js, ts, tsx, css, file, json | same | same |
| 7 | toml | toml | jsonc |
| 8 | wasm | wasm | toml |
| 9 | napi | napi | wasm |
| 10 | base64 | base64 | napi |
| 11 | dataurl | dataurl | base64 |
| 12 | text | text | dataurl |
| 13 | sqlite (1.0.23) | bunsh | text |
| 14 | sqlite_embedded (1.0.23) | sqlite | bunsh |
| 15 | | sqlite_embedded | sqlite |
| 16 | | html (1.1.43) | sqlite_embedded |
| 17 | | | html |
| 18 | | | yaml (1.2.21) |
| 19 | | | json5 (1.3.7) |
| 20 | | | md (1.3.8) |
| 21 | | | xml (1.4.0) |

The 1.2.5 change falls inside layout V1_1_30 (1.1.30 – 1.2.12), so the
layout alone does not determine the name of values 7 and above. A reader
needs the exact release (section 10). A value the release does not define
names nothing, even where a later release defines it.
`crates/pao-core/src/names.rs` implements these tables, and they were
checked against the source enum at every tag.

Most loaders never appear in a module table, because Bun compiles the file
into JavaScript and stores the result as a `js` module. Seen on bytes:

| File | Stored as | Releases |
|---|---|---|
| bundled JavaScript | `js` | all |
| `.db`, imported with `type: "sqlite", embed: "true"` | `file` | 1.0.0 |
| | `sqlite_embedded` | 1.0.36 – 1.4.4 |
| `.node` | `napi` | all |
| `.wasm` | `wasm` | all |
| `.html` | `file` | 1.0.0 – 1.2.12 |
| | `html` | 1.2.23 – 1.4.4 |
| `.sh`, given as an extra entrypoint | `file` | 1.1.30 |
| | `bunsh` | 1.1.45 – 1.2.0 |
| | not stored | 1.2.12 – 1.4.4 |
| `.txt`, imported with `type: "text"` | `text`, as its own entry | 1.4.4 |
| | inlined into the importing module | 1.0.0 – 1.4.0 |
| `.toml`, `.jsonc`, `.txt`, given as extra entrypoints | `js` | all tested |

The values `toml`, `jsonc`, `base64`, `dataurl`, `sqlite`, `yaml`, `json5`,
`md` and `xml` were not seen in any module table.

### 7.2 Module format, encoding and side

These have had the same values in every release that has them:

| Value | `module_format` | `encoding` | `side` |
|---:|---|---|---|
| 0 | none | binary | server |
| 1 | esm | latin1 | client |
| 2 | cjs | utf8, never written (≤ 1.4.0); utf16 (≥ 1.4.1) | |

Bun 1.4.1 (`4661e494f052`) reused `encoding` 2, which earlier releases defined but never wrote,
for UTF-16 code units, so that an older runtime still reads such a module
as plain bytes. Bytecode is not tied to one module format: the matrix's
`--bytecode` builds are CommonJS (`module_format` 2), and from 1.3.14,
ES modules (1) can carry bytecode too, with `module_info` alongside. Bun 1.4.4 writes
`side` 1 (`client`) for embedded assets such as `.db`, `.node` and `.wasm`
files.

## 8. Bun 1.4 extensions

### 8.1 Flags

`Offsets.flags` began as padding and became a bit set in Bun 1.3.3. Later
releases added bits, read from the flags type at all 31 release tags from
1.2.13 to 1.4.4:

| Bit | Name | Since | Meaning |
|---:|---|---|---|
| 0 | `DISABLE_DEFAULT_ENV_FILES` | 1.3.3 | Run-time behaviour. |
| 1 | `DISABLE_AUTOLOAD_BUNFIG` | 1.3.3 | Run-time behaviour. |
| 2 | `DISABLE_AUTOLOAD_TSCONFIG` | 1.3.4 | Run-time behaviour. |
| 3 | `DISABLE_AUTOLOAD_PACKAGE_JSON` | 1.3.4 | Run-time behaviour. |
| 4 | `SOURCE_TEXT_CONTIGUOUS` | 1.4.1 | Every module's `contents` lies in one run that no other region overlaps. |
| 5 | `HAS_SOURCE_HASHES` | 1.4.1 | Record present (8.2). |
| 6 | `HAS_BUILTIN_BYTECODE` | 1.4.1 | Record present. |
| 7 | `HAS_BYTECODE_STRING_TABLE` | 1.4.1 | Record present. |
| 8 | `HAS_STARTUP_MODULE_COUNT` | 1.4.1 | Record present. |
| 9 | `HAS_MODULE_INFO_STRING_TABLE` | 1.4.1 | Record present. |
| 10 | `CROSS_COMPILED_BYTECODE` | 1.4.1 | The bytecode was written by another platform's JavaScriptCore. |
| 11 | `HAS_PRELINKED_MODULE_GRAPH` | 1.4.3 | Record present. |
| 12 | `HAS_RUNTIME_OPTIONS` | 1.4.3 | Record present. |
| 13 | `HAS_LINKED_BYTECODE_PAYLOAD` | 1.4.3 | Record present, and `bytecode` pointers change meaning (8.3). |

Commits: 1.3.3 `abb1b0c4d75a`, 1.3.4 `5eb2145b3104`, 1.4.1 `4661e494f052`,
1.4.3 `c6da4a4d3010`. Bits 14 – 31 are undefined at 1.4.4.

The first two steps fall inside layout V1_2_23 (1.2.23 – 1.3.7), so a reader
must read these 4 bytes in both 32-byte layouts and decide what they mean from
the release (section 10): a 1.3.7 build writes `0xc`. A bit the release does
not define must be reported, not named; in every matrix build, no such bit is
set. `crates/pao-core/src/flags.rs` implements this table.

### 8.2 Extension records

The extension records follow the module table directly, in bit order, each
present only if its bit is set:

```text
 end of module table
 |
 v
 +------------------------------+  bit 5   source hashes: u32 per module
 | u32 x count                  |          (WTF string hash; 0 = none)
 +------------------------------+  bit 6   builtin bytecode
 | u32 n                        |
 | n x { u32 id, StringPointer }|          ahead-of-time bytecode for Bun's
 +------------------------------+          internal modules
 | StringPointer                |  bit 7   shared bytecode string table
 +------------------------------+
 | u32                          |  bit 8   number of leading modules loaded
 +------------------------------+          before the first import()
 | StringPointer                |  bit 9   string table for module_info
 +------------------------------+
 | StringPointer                |  bit 11  prelinked module graph,
 | u32 n                        |          then the module-table index of
 | n x u32                      |          each graph module
 +------------------------------+
 | u32 flags, u32 value         |  bit 12  runtime options
 +------------------------------+
 | StringPointer                |  bit 13  linked bytecode payload,
 | u32 x 6                      |          then where each of its 6 regions
 +------------------------------+          ends (payload-relative to it)
 | ... remaining data           |
 +------------------------------+
 | Offsets                      |
 +------------------------------+
```

In the one tested executable with all 14 bits set, these records, followed by
the 1-byte terminator of an empty `compile_exec_argv` string, fill exactly
the space between the module table and `Offsets`. The region count, 6, is
`bun_bundler::bytecode_order::REGION_COUNT`; it was measured, because that
constant's source was not read. The last region end equals the linked
payload's length, which Bun's own decoder requires.

### 8.3 Linked bytecode

Without `HAS_LINKED_BYTECODE_PAYLOAD`, a module's `bytecode` pointer names
that module's own bytecode, and its length is the bytecode's size.

With the flag, all modules share one bytecode payload produced by
JavaScriptCore's `BytecodeLinkEncoder`, and each module's `bytecode` pointer
runs from that module's cache entry to the end of the shared payload:

```text
 linked payload (the StringPointer in the bit-13 record)
 +--------+---------------+------------------+---------------------+
 | ...    | entry A  ...  | entry B  ...     | entry C  ...        |
 +--------+---------------+------------------+---------------------+
          ^               ^                  ^                     ^
          |               |                  |                     |
          |               |                  |<--- C.bytecode ---->|
          |               |<------------ B.bytecode -------------->|
          |<------------------- A.bytecode ----------------------->|
                                                                   end of
                                                                   payload
```

So:

- every module's `bytecode` ends at the same payload offset, the end of the
  linked payload;
- `bytecode.offset - payload.offset` is the module's entry offset within the
  shared payload;
- a module's bytecode has no length of its own. Its entry refers to data
  elsewhere in the shared payload, so no byte range belongs to one module
  alone, and a module's bytecode cannot be extracted separately. Bun passes
  JavaScriptCore the whole payload and the entry offset.

Bun's decoder states the rule for internal modules in the same words: "Like
a module's `bytecode`, an internal module's runs from its cache entry to the
end of the payload." In the tested executable, every module's bytecode
pointer ended at the same offset, and the linked payload ended exactly where
the first module's contents began, as `SOURCE_TEXT_CONTIGUOUS` implies.

None of the matrix fixtures set this flag. It needs a build with a payload
order file, and the option that produces one has not been identified.

## 9. Reading procedure

A conforming reader locates the payload and identifies its layout from the
bytes alone, as follows. Let `file` be the whole executable.

1. Find every occurrence of the trailer in `file`.
2. For each occurrence at file offset `t`, and for each layout in section 6:
   1. Let `o = t - size of Offsets` for that layout. Reject if negative.
      Decode `Offsets` from `file[o .. t]`.
   2. Let `base = o - byte_count`. Reject if negative. The payload is
      `file[base .. o]`.
   3. If the layout has `compile_exec_argv_ptr`, reject unless it lies
      within the payload.
   4. Reject unless `modules_ptr` lies within the payload and its length is
      non-zero and a multiple of the layout's entry size. Let `count` be the
      quotient. Reject unless `entry_point_id < count`.
   5. Decode every entry. Reject unless every `StringPointer` the layout has
      lies within the payload, and every name is non-empty, at most 4,096
      bytes, valid UTF-8, and free of control characters.
   6. Otherwise the pair (`t`, layout) is a candidate.
3. Exactly one candidate is the answer. No candidate means the file is not a
   Bun executable, if there was no trailer, or that it uses a layout this
   document does not specify. More than one candidate is an error to report;
   a reader must not pick one.

"Lies within the payload" means `offset + length <= byte_count`, computed
without overflow.

A reader must not require a particular name prefix: section 6's prefixes
have changed twice, and a structural check alone gave exactly one candidate
in every tested executable. The 4,096-byte name limit is Linux's `PATH_MAX`,
an arbitrary bound much larger than any name observed.

The procedure is implemented by `crates/pao-core/src/locate.rs`, and in
Python, with the older printable-ASCII name rule, by `tools/probe.py`.

## 10. Identifying the Bun release

No field records the Bun release. The runtime image does contain a version
string, of the form:

```text
Bun v<major>.<minor>.<patch> (<commit>)
```

where `<commit>` is 8 or 9 lowercase hexadecimal digits. In each of the 74
matrix executables, this pattern had exactly one distinct match, and its
version was the release that built the file. A reader should search only the
runtime image, the bytes outside `base .. t + 16`, because bundled code can
contain the same text.

Anything other than exactly one distinct match should be reported as an
error. Falling back to the layout's release range would hide a missing or
altered string and make version-keyed tables silently wrong.
`crates/pao-core/src/version.rs` implements this.

A reader should key version-dependent tables, such as the loader table in
section 7.1, on the version number, not the commit. A Bun built from a fork
or from a commit other than the release reports the release's version with
its own commit; the tested production executable does exactly this.

## 11. Evidence

Three sets of executables back this document. The first two are rebuilt by
`tools/bun-matrix/build.sh`; see the comment at its top.

| Set | Contents | Checks |
|---|---|---|
| Matrix | `fixture/`, built by 14 releases from 1.0.0 to 1.4.4 for macOS arm64, Linux x64 and Windows x64, with and without `--bytecode`: 74 executables. | Layout detection gives one candidate per file, matching the release (`tests/locate_corpus.rs`); placement (section 3); version strings (section 10). |
| Loaders | `fixture-loaders/`, built by the same 14 releases for the host: 14 executables. | Loader values against section 7.1's tables (`tests/loaders_corpus.rs`). |
| Production | One large executable built by a custom Bun 1.4.4. | Layout detection on thousands of modules; flags, extension records and linked bytecode (section 8). |

The matrix has gaps. Cross-compiling failed for ten combinations at Bun
1.1.0 and earlier, where `--target` did not accept these platform names, and
the files built for the Linux and Windows targets by 1.0.0 are Mach-O
executables. The matrix executables hold one or two modules each and the
loader executables five to eight, so large tables in the 32- and 36-byte
layouts are untested.

## 12. Rationale

- **Structural inference instead of a version table.** Mapping the version
  string to a layout would be explicit, but it needs an entry for every
  release, fails on the first release not listed, and fails on a stripped or
  moved version string. Inference keeps working on a new release that reuses
  a known layout. The cost is the possibility of ambiguity: a table whose
  length is a multiple of two entry sizes (any multiple of 288 bytes for 32
  and 36) could decode both ways. Checking every entry makes that unlikely,
  and the procedure reports it rather than guessing. The version is still
  needed for field values (section 7.1), where no structural check helps.
- **Trailer scan instead of parsing Mach-O, PE and ELF.** One search over the
  file needs no per-platform code. Parsing the container would find the
  payload directly, but it would add three parsers to the path that must
  work, each tracking section 3's history of moves. That history is what
  breaks readers that look in one place: a reader that looks only in the
  `__BUN` section fails on macOS builds from 1.2.0 and earlier.

## 13. Open questions

- **Linked-payload builds.** Which Bun option produces a payload order file,
  so that a fixture can set `HAS_LINKED_BYTECODE_PAYLOAD`.
- **Bytecode format.** JavaScriptCore's, tied to the exact JavaScriptCore
  version. Out of scope; section 8.3 is as far as this document goes.
- **Extension records before 1.4.4.** Section 8.2's record order was read
  at 1.4.4 and checked on one 1.4.4 executable. Bits 5 – 9 date from 1.4.1
  and 11 – 13 from 1.4.3; whether their records had the same shape in those
  releases has not been read.
- **The `.toml`, `jsonc` and other unseen loaders.** No tested build stores
  them as their own entries; if one does, section 7.1's tables name them, but
  that naming is checked only against the source.

## 14. References

`StandaloneModuleGraph` is in `src/standalone_bun.zig` (1.0.0),
`src/StandaloneModuleGraph.zig` (1.0.36 – 1.3.7),
`src/standalone_graph/StandaloneModuleGraph.zig` (1.3.14) and
`src/standalone_graph/StandaloneModuleGraph.rs` (1.4.x). `Loader` is in
`src/options.zig` (through 1.3.7), `src/bundler/options.zig` (1.3.14) and
`src/ast/loader.rs` (1.4.4).

| Release | Commit | Used for |
|---|---|---|
| 1.0.0 | `822a00c4d508` | Layout V1_0, loader table |
| 1.1.20 | `ae19489250ab` | Layout V1_0, loader table |
| 1.1.30 | `7996d06b8f58` | Layout V1_1_30, loader table |
| 1.1.45 | `196621f253cf` | Loader table |
| 1.2.0 | `b0c5a7655dac` | Loader table |
| 1.2.4 | `fd9a5ea668e9` | Last release without `jsonc` |
| 1.2.5 | `013fdddc6ed1` | First release with `jsonc` |
| 1.2.12 | `0785effaab48` | Layout V1_1_30, loader table |
| 1.2.23 | `cf1367137da5` | Layout V1_2_23, loader table |
| 1.3.7 | `ba426210c28a` | Layout V1_2_23, loader table |
| 1.3.14 | `0d9b296af33f` | Layout V1_3_14, loader table |
| 1.4.4 | `38afee6c84fc` | Layout V1_3_14, flags, extension records, linked bytecode, loader table |
