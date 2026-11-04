"""Inserts an APP2 ICC_PROFILE segment of junk (only the RGB colour space bytes are right) after
the SOI of a JPEG. Usage: python3 make_jpeg_junk_icc.py IN.jpg OUT.jpg"""
import struct, sys

data = open(sys.argv[1], 'rb').read()
assert data[:2] == b'\xff\xd8'
junk = b'\0' * 16 + b'RGB ' + b'\xff' * 108
payload = b'ICC_PROFILE\0' + bytes([1, 1]) + junk
segment = b'\xff\xe2' + struct.pack('>H', len(payload) + 2) + payload
open(sys.argv[2], 'wb').write(data[:2] + segment + data[2:])
