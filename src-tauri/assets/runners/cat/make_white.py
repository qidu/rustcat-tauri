"""One-off: derive the white cat frames from the black ones.

The cat art is a pure-black alpha mask: RGB is (0,0,0) for every pixel and the
shape lives entirely in the alpha channel. So the "white" variant is just the
same image with RGB forced to 255 and alpha untouched.

Pure stdlib on purpose -- this machine has no Pillow and no ImageMagick.
"""
import struct
import sys
import zlib
from pathlib import Path

SIG = b"\x89PNG\r\n\x1a\n"


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def decode_rgba8(path):
    raw = path.read_bytes()
    assert raw[:8] == SIG, f"{path}: not a PNG"
    pos, idat, ihdr = 8, [], None
    while pos < len(raw):
        (n,) = struct.unpack(">I", raw[pos : pos + 4])
        typ = raw[pos + 4 : pos + 8]
        data = raw[pos + 8 : pos + 8 + n]
        if typ == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", data)
        elif typ == b"IDAT":
            idat.append(data)
        elif typ == b"IEND":
            break
        pos += 12 + n

    w, h, depth, ctype, comp, filt, interlace = ihdr
    assert (depth, ctype, comp, filt, interlace) == (8, 6, 0, 0, 0), (
        f"{path}: expected non-interlaced 8-bit RGBA, got {ihdr}"
    )

    bpp = 4
    stride = w * bpp
    flat = zlib.decompress(b"".join(idat))
    assert len(flat) == h * (stride + 1), f"{path}: unexpected IDAT size"

    out = bytearray(h * stride)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        ft = flat[p]
        p += 1
        line = bytearray(flat[p : p + stride])
        p += stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            x = line[i]
            if ft == 0:
                v = x
            elif ft == 1:
                v = x + a
            elif ft == 2:
                v = x + b
            elif ft == 3:
                v = x + ((a + b) >> 1)
            elif ft == 4:
                v = x + paeth(a, b, c)
            else:
                raise AssertionError(f"{path}: bad filter {ft}")
            line[i] = v & 0xFF
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return w, h, out


def chunk(typ, data):
    return (
        struct.pack(">I", len(data))
        + typ
        + data
        + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
    )


def encode_rgba8(w, h, pixels):
    stride = w * 4
    # Filter type 0 (None) on every row: these are 32x32 icons, the size cost of
    # a smarter filter is not worth the code.
    filtered = bytearray()
    for y in range(h):
        filtered.append(0)
        filtered += pixels[y * stride : (y + 1) * stride]
    return (
        SIG
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(filtered), 9))
        + chunk(b"IEND", b"")
    )


def main():
    src_dir = Path(sys.argv[1])
    for i in range(5):
        src = src_dir / f"cat_{i}.png"
        dst = src_dir / f"cat_white_{i}.png"
        w, h, px = decode_rgba8(src)
        for o in range(0, len(px), 4):
            px[o] = px[o + 1] = px[o + 2] = 255
        dst.write_bytes(encode_rgba8(w, h, px))
        print(f"{dst.name}: {w}x{h}, {len(px)} bytes rgba -> {dst.stat().st_size} bytes png")


if __name__ == "__main__":
    main()
