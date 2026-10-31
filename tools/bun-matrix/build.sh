#!/bin/bash
# Build the fixture with every Bun version under bun/<version>/bun, for each
# cross-compile target, plain, with --bytecode and with --sourcemap. Every
# attempt gets a row in out/builds.tsv, failures included: older Bun versions
# predate cross-compilation (~1.1) and --bytecode (1.1.30), and those rows are
# part of the result rather than noise.
#
# Usage: tools/bun-matrix/build.sh
# Needs: tools/bun-matrix/bun/<version>/bun for each version to test, from
# Bun's GitHub releases. Downloading those is a separate, deliberate step
# because it fetches and runs outside code.
#
# Then, from the repository root (cargo runs tests from the crate directory,
# so the paths must be absolute):
#   PAO_MATRIX_DIR=$PWD/tools/bun-matrix/out/bin \
#   PAO_LOADERS_DIR=$PWD/tools/bun-matrix/out/bin-loaders cargo test -p pao-core

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
out="$here/out"
mkdir -p "$out/bin" "$out/log" "$out/maps"
printf 'version\ttarget\tflags\tstatus\tbinary\n' > "$out/builds.tsv"

targets=(bun-darwin-arm64 bun-linux-x64 bun-windows-x64)

for bun in "$here"/bun/*/bun; do
    version=$(basename "$(dirname "$bun")")
    for target in "${targets[@]}"; do
        for flags in plain bytecode sourcemap; do
            name="$version-$target-$flags"
            bin="$out/bin/$name"
            [[ $target == *windows* ]] && bin="$bin.exe"

            native=(build --compile "$here/fixture/index.ts" --outfile "$bin")
            [[ $flags == bytecode ]] && native+=(--bytecode)
            [[ $flags == sourcemap ]] && native+=(--sourcemap)
            args=("${native[@]}" "--target=$target")

            # Before 1.1.20 (measured; 1.1.0 fails, 1.1.20 works) `--target`
            # rejects the bun-<os>-<arch> names with InvalidTarget. For the
            # host target, retry without the flag so the oldest module-graph
            # layouts still get a binary; the other targets stay failed.
            if "$bun" "${args[@]}" > "$out/log/$name.log" 2>&1; then
                status=ok
            elif [[ $target == bun-darwin-arm64 ]] \
                && grep -q InvalidTarget "$out/log/$name.log" \
                && "$bun" "${native[@]}" >> "$out/log/$name.log" 2>&1; then
                status=ok-native
            else
                status=fail
                bin=-
            fi
            # Some --sourcemap builds also write a JSON source map into bin/,
            # as <binary>.map or index.js.map. Move it to maps/: the corpus
            # test expects bin/ to hold binaries only, and the JSON is the
            # reference for decoding the map embedded in the binary.
            for map in "$out"/bin/*.map; do
                [[ -e $map ]] && mv "$map" "$out/maps/$name.map"
            done
            printf '%s\t%s\t%s\t%s\t%s\n' "$version" "$target" "$flags" "$status" "$bin" >> "$out/builds.tsv"
        done
    done
done

# The loader fixture: files Bun stores as their own module entries, so their
# loader values can be checked against each release's enum. The numbering
# does not depend on the target, so only the host is built. Its binary inputs
# are generated rather than checked in: an empty SQLite database, an empty
# shared library (Bun embeds a .node file without loading it), and the
# 8-byte header of an empty WebAssembly module.
loaders="$here/fixture-loaders"
mkdir -p "$out/bin-loaders"
python3 -I -c 'import sqlite3, sys; sqlite3.connect(sys.argv[1]).execute("create table if not exists t(x)")' "$loaders/data.db"
printf 'void pao_fixture(void) {}\n' | cc -shared -x c -o "$loaders/addon.node" -
printf '\0asm\1\0\0\0' > "$loaders/module.wasm"

for bun in "$here"/bun/*/bun; do
    version=$(basename "$(dirname "$bun")")
    name="$version-loaders"
    bin="$out/bin-loaders/$name"
    # script.sh is an extra entrypoint: Bun 1.1.45 to 1.2.0 store it as a
    # `bunsh` entry. Before 1.1.30 (measured: 1.1.20 fails) `--compile` takes
    # one entrypoint only, so retry without it to keep the import checks.
    if "$bun" build --compile "$loaders/index.ts" "$loaders/script.sh" --outfile "$bin" \
        > "$out/log/$name.log" 2>&1; then
        status=ok
    elif grep -q "multiple entry points are not supported" "$out/log/$name.log" \
        && "$bun" build --compile "$loaders/index.ts" --outfile "$bin" \
            >> "$out/log/$name.log" 2>&1; then
        status=ok-single
    else
        status=fail
        bin=-
    fi
    # Bun 1.0.0 to 1.1.0 (measured) also leave a ~50 MB copy of the binary
    # beside the entrypoint, despite --outfile. Why is not known; delete it so
    # it does not sit untracked in the fixture.
    rm -f "$loaders/$name"
    printf '%s\t%s\t%s\t%s\t%s\n' "$version" host loaders "$status" "$bin" >> "$out/builds.tsv"
done

column -t -s $'\t' "$out/builds.tsv"
