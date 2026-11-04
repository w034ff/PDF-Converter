"""Writes a grey 8-bit PNG of WIDTH x HEIGHT, all white. Usage: python3 make_png_strip.py OUT WIDTH HEIGHT"""
import struct, sys, zlib

def chunk(t, data=b''):
    return struct.pack('>I', len(data)) + t + data + struct.pack('>I', zlib.crc32(t + data))

out, w, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
comp = zlib.compressobj(9)
idat = bytearray()
row = b'\x00' + b'\xff' * w
for _ in range(h):
    idat += comp.compress(row)
idat += comp.flush()
with open(out, 'wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 0, 0, 0, 0)))
    f.write(chunk(b'IDAT', bytes(idat)) + chunk(b'IEND'))
