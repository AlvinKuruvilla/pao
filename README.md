<p align="center">
  <img src="crates/pao-gui/icons/logo.svg" width="128" alt="Pao icon">
</p>

<h1 align="center">Pao</h1>

<p align="center">
  Read the code inside Bun single-file executables.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Bun-1.0_to_1.4.4-f9f1e1?logo=bun&logoColor=black" alt="Bun 1.0 to 1.4.4">
  <img src="https://img.shields.io/badge/rust-2024_edition-orange?logo=rust" alt="Rust 2024 edition">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT_OR_Apache--2.0-green" alt="MIT OR Apache-2.0 license"></a>
</p>

`bun build --compile` ships a program as one executable: Bun's runtime with
the bundled code, assets and source maps attached. The layout of that payload
records no version and has changed three times since Bun 1.0, so what an
executable runs is opaque to anyone who did not build it.

Pao reads the module graph back: every module's path, code, source map and
bytecode, from executables built for macOS, Linux and Windows by Bun 1.0.0
to 1.4.4. It finds the payload by scanning for Bun's trailer instead of
parsing Mach-O, PE or ELF, and infers the layout from the bytes. When the
program was built with `--sourcemap`, the original files come back too,
TypeScript and all.

It comes as a library, `pao-core`; a command line tool, `pao`; and a desktop
app that browses an executable the way ILSpy browses a .NET assembly.

## Who needs this

- **Anyone handed an executable to run.** See what code it contains, which
  Bun release built it, and what files it embeds, before running it.
- **Developers who lost their source.** An app built with `--sourcemap`
  carries its original files, byte for byte; `pao extract` writes them out.
- **Developers checking a build.** Confirm which modules, assets and
  bytecode made it into the executable, and where each sits in the file.

## Contents

- [Who needs this](#who-needs-this)
- [Status](#status)
- [Supported Bun releases](#supported-bun-releases)
- [Usage](#usage)
- [Desktop app](#desktop-app)
- [Testing](#testing)
- [License](#license)

## Status

- **`pao-core`**: locates the payload, decodes the module table, and decodes
  embedded source maps. Done for the four layouts below.
- **`pao` CLI**: `inspect` prints the payload summary and module table.
  `extract` writes each module's code, and the original source files from any
  embedded source map. Splitting a bundle back into its files without a source
  map is not done yet.
- **Pao** (desktop app, Tauri): a read-only viewer over the same files
  `extract` writes, with the module details `inspect` prints. Not packaged
  for release yet.

## Supported Bun releases

| Layout | Bun | Offsets | Entry |
|---|---|---|---|
| `V1_0` | 1.0.0 – 1.1.20 | 24 bytes | 32 bytes |
| `V1_1_30` | 1.1.30 – 1.2.12 | 24 bytes | 36 bytes |
| `V1_2_23` | 1.2.23 – 1.3.7 | 32 bytes | 36 bytes |
| `V1_3_14` | 1.3.14 – 1.4.4 | 32 bytes | 52 bytes |

Checked against binaries for macOS arm64, Linux x64 and Windows x64 built by
each release in `tools/bun-matrix`. Releases between those tested are assumed
to share their neighbours' layout. A binary from a newer Bun with an unknown
layout fails with an error saying so, not a wrong answer.

[`docs/format.md`](docs/format.md) describes the format, where the payload sits
on each platform, how detection works, and what is still unknown.

## Usage

```sh
pao inspect app       # the module table
pao extract app       # writes app.extracted/{bundled,sources}/
```

`bundled/` is made readable rather than byte-exact: text is converted to UTF-8,
JavaScript modules get a `.js` name, and embedded `.zst` files are
decompressed (`--no-decompress` keeps them). `pao inspect` gives the offsets
of the stored bytes.

`sources/` holds the original files when the app was built with
`--sourcemap`, at their paths relative to the directory Bun ran in. Bun does
not record that directory's name, so when a source lies outside it
(`../shared.ts`), the directory is named after the executable.

As a library:

```rust
let binary = std::fs::read("app")?;
let located = pao_core::locate::locate(&binary)?;
for module in &located.modules {
    println!("{}: {} bytes", module.name, module.contents.len());
}
```

## Desktop app

```sh
(cd gui && pnpm install)
cd crates/pao-gui && cargo tauri dev
```

Needs Node, pnpm and the Tauri CLI (`cargo install tauri-cli`). The frontend
in `gui/` is React, Monaco and Prettier; the backend in `crates/pao-gui` holds
the opened executable's files in memory and serves them one at a time.

- **Open**: drag an executable onto the window, or ⌘O.
- **Find a file**: ⌘P opens a picker with a preview.
- **Formatting**: files up to 1 MB are formatted with Prettier when opened;
  larger ones have a Format button.

`pao-gui` is not a default workspace member, so `cargo build` and
`cargo test` skip Tauri; build it with `-p pao-gui`. The app icons are
generated from `crates/pao-gui/icons/logo.svg`:

```sh
rsvg-convert -w 1024 -h 1024 crates/pao-gui/icons/logo.svg -o logo.png
cd crates/pao-gui && cargo tauri icon ../../logo.png && rm -rf icons/android icons/ios
```

## Testing

```sh
cargo test
```

The unit tests need nothing else. The corpus tests skip unless pointed at real
binaries:

```sh
# Needs tools/bun-matrix/bun/<version>/bun for each release, from Bun's
# GitHub releases.
tools/bun-matrix/build.sh
PAO_MATRIX_DIR=$PWD/tools/bun-matrix/out/bin \
PAO_LOADERS_DIR=$PWD/tools/bun-matrix/out/bin-loaders \
    cargo test -p pao-core

# Any other Bun executable: checks only that it decodes.
PAO_EXTRA_BINARY=path/to/app cargo test -p pao-core
```

`tools/probe.py` is the Python reference for detection:
`python3 -I tools/probe.py BINARY...`.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT), at your option.

The desktop app bundles the file and folder icons of
[Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme),
copyright (c) 2025 Material Extensions, under the MIT License. Its license
text ships in the `material-icon-theme` package, at
`gui/node_modules/material-icon-theme/LICENSE`.
