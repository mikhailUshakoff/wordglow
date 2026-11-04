#!/usr/bin/env python3
"""Convert PPM to small grayscale PNG using stdlib only."""
import sys, zlib, struct, os

def write_png(path, width, height, gray_rows):
    def chunk(tag, data):
        c = struct.pack('>I', len(data)) + tag + data
        return c + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)

    raw = b''.join(b'\x00' + row for row in gray_rows)
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 0, 0, 0, 0))
    idat = chunk(b'IDAT', zlib.compress(raw, 6))
    iend = chunk(b'IEND', b'')
    with open(path, 'wb') as f:
        f.write(sig + ihdr + idat + iend)

def convert(src, dst, scale=2):
    with open(src, 'rb') as f:
        magic = f.readline().strip()
        assert magic == b'P6'
        while True:
            line = f.readline()
            if not line.startswith(b'#'):
                break
        w, h = map(int, line.split())
        maxval = int(f.readline().strip())
        data = f.read()

    bpp = 2 if maxval > 255 else 1
    out_w = w // scale
    out_h = h // scale
    rows = []
    for y in range(out_h):
        row = bytearray()
        src_y = y * scale
        for x in range(out_w):
            src_x = x * scale
            off = (src_y * w + src_x) * 3 * bpp
            if bpp == 1:
                r, g, b = data[off], data[off+1], data[off+2]
            else:
                r = int.from_bytes(data[off:off+2], 'big') >> (maxval.bit_length() - 8)
                g = int.from_bytes(data[off+2:off+4], 'big') >> (maxval.bit_length() - 8)
                b = int.from_bytes(data[off+4:off+6], 'big') >> (maxval.bit_length() - 8)
            gray = (r * 299 + g * 587 + b * 114) // 1000
            row.append(gray)
        rows.append(bytes(row))
    write_png(dst, out_w, out_h, rows)
    print(f"{src} -> {dst} ({out_w}x{out_h})")

if __name__ == '__main__':
    src = sys.argv[1]
    dst = sys.argv[2]
    scale = int(sys.argv[3]) if len(sys.argv) > 3 else 2
    convert(src, dst, scale)
