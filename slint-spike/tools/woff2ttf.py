"""Converts a WOFF 1.0 font to an sfnt (TTF/OTF) using only the standard library."""
import struct, sys, zlib

def convert(src, dst):
    data = open(src, "rb").read()
    sig, flavor, length, num_tables = struct.unpack(">4sIIH", data[:14])
    assert sig == b"wOFF", "not a WOFF 1.0 file"
    entries = []
    for i in range(num_tables):
        tag, off, clen, olen, csum = struct.unpack(">4sIIII", data[44 + i * 20: 64 + i * 20])
        raw = data[off:off + clen]
        if clen != olen:
            raw = zlib.decompress(raw)
        entries.append((tag, csum, raw))
    entries.sort(key=lambda e: e[0])
    es = 1
    while es * 2 <= num_tables:
        es *= 2
    search_range = es * 16
    entry_selector = es.bit_length() - 1
    range_shift = num_tables * 16 - search_range
    header = struct.pack(">IHHHH", flavor, num_tables, search_range, entry_selector, range_shift)
    offset = 12 + 16 * num_tables
    dirs, bodies = b"", b""
    for tag, csum, raw in entries:
        dirs += struct.pack(">4sIII", tag, csum, offset + len(bodies), len(raw))
        bodies += raw + b"\0" * ((4 - len(raw) % 4) % 4)
    open(dst, "wb").write(header + dirs + bodies)

if __name__ == "__main__":
    convert(sys.argv[1], sys.argv[2])
