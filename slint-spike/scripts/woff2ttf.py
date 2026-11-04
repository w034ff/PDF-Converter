"""Converts a WOFF 1.0 file to an sfnt (TTF/OTF) using only the standard library.

Usage: python3 -I woff2ttf.py <in.woff> <out.ttf>
"""
import struct
import sys
import zlib


def convert(data: bytes) -> bytes:
    sig, flavor, _len, num_tables = struct.unpack(">4sIIH", data[:14])
    if sig != b"wOFF":
        raise ValueError("not a WOFF 1.0 file")
    entries = []
    for i in range(num_tables):
        tag, off, clen, olen, csum = struct.unpack(">4sIIII", data[44 + 20 * i : 64 + 20 * i])
        raw = data[off : off + clen]
        table = zlib.decompress(raw) if clen < olen else raw
        if len(table) != olen:
            raise ValueError(f"bad table length for {tag!r}")
        entries.append((tag, csum, table))
    entries.sort(key=lambda e: e[0])
    search = 1
    sel = 0
    while search * 2 <= num_tables:
        search *= 2
        sel += 1
    out = bytearray(struct.pack(">IHHHH", flavor, num_tables, search * 16, sel, num_tables * 16 - search * 16))
    offset = 12 + 16 * num_tables
    body = bytearray()
    for tag, csum, table in entries:
        out += struct.pack(">4sIII", tag, csum, offset + len(body), len(table))
        body += table
        body += b"\0" * ((4 - len(table) % 4) % 4)
    return bytes(out + body)


if __name__ == "__main__":
    with open(sys.argv[1], "rb") as f:
        result = convert(f.read())
    with open(sys.argv[2], "wb") as f:
        f.write(result)
