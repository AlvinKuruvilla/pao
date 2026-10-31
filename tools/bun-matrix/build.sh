#!/bin/bash
# Build the fixture with every Bun version under bun/<version>/bun, for each
# cross-compile target and with and without --bytecode. Every attempt gets a
# row in out/builds.tsv, failures included: older Bun versions predate
# cross-compilation (~1.1) and --bytecode (1.1.30), and those rows are part of
# the result rather than noise.
#
# Usage: tools/bun-matrix/build.sh
# Needs: tools/bun-matrix/bun/<version>/bun for each version to test, from
# Bun's GitHub releases. Downloading those is a separate, deliberate step
# because it fetches and runs outside code.
#
# Then: PAO_MATRIX_DIR=tools/bun-matrix/out/bin cargo test -p pao-core

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
out="$here/out"
mkdir -p "$out/bin" "$out/log"
printf 'version\ttarget\tflags\tstatus\tbinary\n' > "$out/builds.tsv"

targets=(bun-darwin-arm64 bun-linux-x64 bun-windows-x64)

for bun in "$here"/bun/*/bun; do
    version=$(basename "$(dirname "$bun")")
    for target in "${targets[@]}"; do
        for flags in plain bytecode; do
            name="$version-$target-$flags"
            bin="$out/bin/$name"
            [[ $target == *windows* ]] && bin="$bin.exe"

            native=(build --compile "$here/fixture/index.ts" --outfile "$bin")
            [[ $flags == bytecode ]] && native+=(--bytecode)
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
            printf '%s\t%s\t%s\t%s\t%s\n' "$version" "$target" "$flags" "$status" "$bin" >> "$out/builds.tsv"
        done
    done
done

column -t -s $'\t' "$out/builds.tsv"
