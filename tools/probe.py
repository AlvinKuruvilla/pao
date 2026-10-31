"""Check, on real binaries, whether a Bun payload can be located and its
layout identified without parsing Mach-O/PE/ELF headers or knowing the Bun
version.

For each occurrence of the trailer, try every candidate Offsets size and
module-entry size, and keep the combinations whose pointers all land inside
the payload. Prints one line per file.

Candidate sizes come from Bun's source (StandaloneModuleGraph at the tagged
releases): Offsets is 24 bytes before compile_exec_argv_ptr was added and 32
after; entries are 32 (name, contents, sourcemap, then the loader byte, which
Zig moves last and pads to 8), 36 (four pointers plus up to four u8 fields)
or 52 (six pointers plus four u8 fields). The 32 was measured on Bun 1.0.0 and
1.1.20 binaries: the source declares loader second, and 28 bytes did not fit.

Usage: python3 -I probe.py BINARY...
"""

import mmap
import struct
import sys

TRAILER = b"\n---- Bun! ----\n"
OFFSETS_SIZES = (24, 32)
ENTRY_SIZES = (32, 36, 52)


def pointer_ok(off: int, length: int, byte_count: int) -> bool:
    return off + length <= byte_count


def try_layout(buf, end: int, osize: int, esize: int):
    """Return a description if (osize, esize) decodes cleanly, else None."""
    o = end - len(TRAILER) - osize
    if o < 0:
        return None
    byte_count, mod_off, mod_len, entry_id = struct.unpack_from("<QIII", buf, o)
    start = o - byte_count
    if start < 0 or mod_len == 0 or mod_len % esize:
        return None
    if not pointer_ok(mod_off, mod_len, byte_count):
        return None
    count = mod_len // esize
    if entry_id >= count:
        return None
    first_name = None
    for i in range(count):
        e = start + mod_off + i * esize
        name_off, name_len, contents_off, contents_len = struct.unpack_from("<IIII", buf, e)
        if not (0 < name_len < 4096 and pointer_ok(name_off, name_len, byte_count)):
            return None
        if not pointer_ok(contents_off, contents_len, byte_count):
            return None
        name = bytes(buf[start + name_off : start + name_off + name_len])
        if not all(32 <= c < 127 for c in name):
            return None
        if first_name is None:
            first_name = name.decode()
    header = struct.unpack_from("<Q", buf, start - 8)[0] if start >= 8 else None
    return {
        "layout": f"O{osize}/E{esize}",
        "count": count,
        "first": first_name,
        "at_eof": end == len(buf),
        # Mach-O sections begin with a u64 length; record whether it matches.
        "u64_header": header == byte_count + osize + len(TRAILER),
    }


def probe(path: str) -> str:
    with open(path, "rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as buf:
        hits = []
        pos = buf.find(TRAILER)
        occurrences = 0
        while pos != -1:
            occurrences += 1
            end = pos + len(TRAILER)
            for osize in OFFSETS_SIZES:
                for esize in ENTRY_SIZES:
                    r = try_layout(buf, end, osize, esize)
                    if r:
                        hits.append(r)
            pos = buf.find(TRAILER, pos + 1)
    if not hits:
        return f"NONE  trailers={occurrences}"
    parts = [
        f"{h['layout']} n={h['count']} eof={int(h['at_eof'])} hdr={int(h['u64_header'])} {h['first']}"
        for h in hits
    ]
    tag = "OK   " if len(hits) == 1 else "AMBIG"
    return f"{tag} trailers={occurrences} " + " | ".join(parts)


if __name__ == "__main__":
    for p in sys.argv[1:]:
        print(f"{p.rsplit('/', 1)[-1]:42} {probe(p)}")
