"""Writes a valid 1x1 PNG with N empty ancillary chunks before IDAT.
Usage: python3 make_png_many_chunks.py OUT N"""
import struct, sys, zlib

def chunk(t, data=b''):
    return struct.pack('>I', len(data)) + t + data + struct.pack('>I', zlib.crc32(t + data))

out, n = sys.argv[1], int(sys.argv[2])
ihdr = struct.pack('>IIBBBBB', 1, 1, 8, 0, 0, 0, 0)
filler = chunk(b'zzZz')
with open(out, 'wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', ihdr))
    f.write(filler * n)
    f.write(chunk(b'IDAT', zlib.compress(b'\x00\x80')) + chunk(b'IEND'))
