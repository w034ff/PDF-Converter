"""Writes small hand-made TIFFs for the edge-case spike. Usage: python3 make_tiff.py OUT_DIR"""
import struct, sys, os

def tiff(width, height, pixels, extra_tags=(), photometric=2, spp=3, bits=8):
    # Little-endian, one IFD, one strip, no compression.
    entries = []
    blobs = bytearray()
    header_len = 8
    def tag(t, typ, count, value_bytes):
        entries.append((t, typ, count, value_bytes))
    bps = struct.pack('<' + 'H' * spp, *([bits] * spp))
    tag(256, 4, 1, struct.pack('<I', width))
    tag(257, 4, 1, struct.pack('<I', height))
    tag(258, 3, spp, bps)
    tag(259, 3, 1, struct.pack('<H', 1))
    tag(262, 3, 1, struct.pack('<H', photometric))
    tag(273, 4, 1, None)  # strip offset, patched
    tag(277, 3, 1, struct.pack('<H', spp))
    tag(278, 4, 1, struct.pack('<I', height))
    tag(279, 4, 1, struct.pack('<I', len(pixels)))
    for t in extra_tags:
        tag(*t)
    entries.sort(key=lambda e: e[0])
    n = len(entries)
    ifd_len = 2 + 12 * n + 4
    data_start = header_len + ifd_len
    out_entries = []
    for t, typ, count, vb in entries:
        if vb is not None and len(vb) > 4:
            off = data_start + len(blobs)
            blobs += vb
            if len(blobs) % 2:
                blobs += b'\0'
            out_entries.append((t, typ, count, struct.pack('<I', off)))
        else:
            out_entries.append((t, typ, count, vb))
    strip_off = data_start + len(blobs)
    body = bytearray(b'II*\0' + struct.pack('<I', 8) + struct.pack('<H', n))
    for t, typ, count, vb in out_entries:
        if vb is None:
            vb = struct.pack('<I', strip_off)
        body += struct.pack('<HHI', t, typ, count) + vb.ljust(4, b'\0')
    body += struct.pack('<I', 0) + blobs + pixels
    return bytes(body)

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
px = bytes([255, 0, 0] * 4)
# An "ICC profile" that has the RGB colour space bytes and nothing else valid.
junk_icc = b'\0' * 16 + b'RGB ' + b'\xff' * 108
open(os.path.join(out, 'tiff_junk_icc.tif'), 'wb').write(tiff(2, 2, px, [(34675, 7, len(junk_icc), junk_icc)]))
# The shortest that passes the check: exactly 20 bytes.
short_icc = b'\0' * 16 + b'RGB '
open(os.path.join(out, 'tiff_short_icc.tif'), 'wb').write(tiff(2, 2, px, [(34675, 7, len(short_icc), short_icc)]))
