"""Writes a 2x2 RGB PNG with an iCCP chunk of junk (only the RGB colour space bytes are right).
Usage: python3 make_png_junk_icc.py OUT"""
import struct, sys, zlib

def chunk(t, data=b''):
    return struct.pack('>I', len(data)) + t + data + struct.pack('>I', zlib.crc32(t + data))

junk = b'\0' * 16 + b'RGB ' + b'\xff' * 108
rows = b''.join(b'\x00' + b'\xff\x00\x00' * 2 for _ in range(2))
with open(sys.argv[1], 'wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 2, 0, 0, 0)))
    f.write(chunk(b'iCCP', b'junk\0\0' + zlib.compress(junk)))
    f.write(chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND'))
