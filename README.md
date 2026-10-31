# pao

pao reads the module graph that `bun build --compile` embeds in a single-file
executable: every bundled module's path, source, source map and bytecode.

It finds the payload by scanning for Bun's trailer instead of parsing Mach-O,
PE or ELF, and infers the layout structurally because Bun records no format
version. One scanner covers every platform and every release tested.

## Status

- **`pao-core`**: locates the payload and decodes the module table. Done for
  the four layouts below.
- **`pao` CLI**: `inspect` prints the payload summary and module table.
  `extract` exists but is not implemented yet.
- **GUI** (Tauri): not started.

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

```rust
let binary = std::fs::read("app")?;
let located = pao_core::locate::locate(&binary)?;
for entry in &located.entries {
    let name = located.payload.get(entry.name).expect("locate checked every pointer");
    println!("{}", String::from_utf8_lossy(name));
}
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
