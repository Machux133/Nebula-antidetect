#!/usr/bin/env python3
"""Line-by-line Python model of nebula_noise.h (patch 0007). Run: python noise_selftest.py

Proves, for the pixel / audio / advance logic: solid and transparent data is untouched, results are
deterministic, getImageData-with-margin == toDataURL for any rectangle, RGBA == BGRA, GL bottom-up
== top-down. It does NOT prove anything about the Chromium glue code (never compiled) and must be
kept in sync with the C++ by hand.
"""
import math
import random
import struct

M = 0xFFFFFFFF


def mix(a, b):
    h = ((a * 0x9E3779B1) & M) ^ ((b + 0x7F4A7C15) & M)
    h ^= h >> 15
    h = (h * 0x85EBCA6B) & M
    h ^= h >> 13
    h = (h * 0xC2B2AE35) & M
    h ^= h >> 16
    return h


def is_blend(p, a, b):
    if p[3] != 255 or a[3] != 255 or b[3] != 255:  # opaque only (0013): premul == unpremul
        return False
    if a == b or p == a or p == b:
        return False
    t_min, t_max, d_min = 2.0, -1.0, 256
    for c in range(4):
        lo, hi = min(a[c], b[c]), max(a[c], b[c])
        if p[c] < lo or p[c] > hi:
            return False
        d = hi - lo
        if d >= 8:
            t = (p[c] - a[c]) / (b[c] - a[c])
            t_min, t_max, d_min = min(t_min, t), max(t_max, t), min(d_min, d)
    return d_min == 256 or t_max - t_min <= 0.06 + 1.0 / d_min


class Pm:
    """Tightly packed 8-bit pixmap; px[y][x] = 4 bytes in memory order."""

    def __init__(self, w, h, bgra=False, premul=False, px=None):
        self.w, self.h, self.bgra, self.premul = w, h, bgra, premul
        self.px = px or [[[0, 0, 0, 0] for _ in range(w)] for _ in range(h)]

    def copy(self):
        return Pm(self.w, self.h, self.bgra, self.premul,
                  [[list(p) for p in r] for r in self.px])


def noise_region(pm, seed, ox, oy, y_dir, cw, ch, inner):
    x0, y0, x1, y1 = inner
    x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, pm.w), min(y1, pm.h)
    edits = []
    for y in range(y0, y1):
        for x in range(x0, x1):
            p = pm.px[y][x]
            if p[3] == 0:
                continue
            ax, ay = ox + x, oy + y_dir * y
            if ax < 0 or ay < 0 or ax >= cw or ay >= ch:
                continue
            h = mix(seed, (((ay * 65537) & M) + ax) & M)
            if h & 3:
                continue
            ok = False
            if 0 < x < pm.w - 1 and ax > 0 and ax + 1 < cw:
                ok = is_blend(p, pm.px[y][x - 1], pm.px[y][x + 1])
            if not ok and 0 < y < pm.h - 1 and ay > 0 and ay + 1 < ch:
                ok = is_blend(p, pm.px[y - 1][x], pm.px[y + 1][x])
            if not ok:
                continue
            channel = (h >> 3) % 3
            idx = 2 - channel if pm.bgra else channel
            bit = (h >> 5) & 1
            new = (p[idx] & ~1) | bit  # idempotent (0014)
            if new != p[idx]:
                edits.append((p, idx, new))
    for p, idx, v in edits:
        p[idx] = v


def noise_full(pm, seed):
    r = pm.copy()
    noise_region(r, seed, 0, 0, 1, pm.w, pm.h, (0, 0, pm.w, pm.h))
    return r


def get_image_data(full, seed, sx, sy, sw, sh):
    """Model of getImageDataInternal: read with a 1 px margin (zero outside canvas), noise inner."""
    pad = Pm(sw + 2, sh + 2, full.bgra, full.premul)
    for y in range(sh + 2):
        for x in range(sw + 2):
            cx, cy = sx - 1 + x, sy - 1 + y
            if 0 <= cx < full.w and 0 <= cy < full.h:
                pad.px[y][x] = list(full.px[cy][cx])
    noise_region(pad, seed, sx - 1, sy - 1, 1, full.w, full.h, (1, 1, sw + 1, sh + 1))
    return [[pad.px[y + 1][x + 1] for x in range(sw)] for y in range(sh)]


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def unbits(b):
    return struct.unpack('<f', struct.pack('<I', b & M))[0]


def noise_audio(samples, seed, first=0):
    out = list(samples)
    for i, s in enumerate(out):
        h = mix(seed, (first + i) & M)
        if h & 1:
            continue
        b = bits(s)
        e = (b >> 23) & 0xFF
        if e == 0 or e >= 0xFE:
            continue
        out[i] = unbits(b + (1 if (h >> 1) & 1 else -1))
    return out


def noise_advance(a, seed):
    if a <= 0 or (a & 0xFFFF) == 0:
        return a
    d = (mix(seed, a & M) & 0x3FF) - 512
    return a + d if a + d > 0 else a


def fill(w, h, c):
    return Pm(w, h, px=[[list(c) for _ in range(w)] for _ in range(h)])


def gradient(w, h):
    pm = Pm(w, h)
    for y in range(h):
        for x in range(w):
            t = x / (w - 1)
            pm.px[y][x] = [round(255 * (1 - t)), 40, round(255 * t), 255]
    return pm


def circle(w, h, fg=(20, 60, 200), bg=(255, 255, 255)):
    pm = Pm(w, h)
    cov = {}
    for y in range(h):
        for x in range(w):
            n = 0
            for sy in range(4):
                for sx in range(4):
                    dx, dy = x + (sx + .5) / 4 - w / 2, y + (sy + .5) / 4 - h / 2
                    n += dx * dx + dy * dy <= (w / 2.6) ** 2
            t = n / 16
            cov[(x, y)] = t
            pm.px[y][x] = [round(bg[c] + (fg[c] - bg[c]) * t) for c in range(3)] + [255]
    return pm, cov


def diff(a, b):
    return [(x, y) for y in range(a.h) for x in range(a.w) if a.px[y][x] != b.px[y][x]]


def main():
    seeds = [1, 7, 123456789, 0xFFFFFFFF]

    # 1. solid / transparent / hard-edged data is bit-exact
    for s in seeds:
        for c in [(0, 0, 0, 0), (255, 255, 255, 255), (37, 120, 200, 255), (10, 20, 30, 128)]:
            img = fill(40, 30, c)
            assert not diff(img, noise_full(img, s)), (s, c)
        r = fill(60, 40, (255, 255, 255, 255))
        for y in range(10, 30):
            for x in range(10, 50):
                r.px[y][x] = [200, 0, 0, 255]
        for x in range(60):
            r.px[5][x] = [0, 0, 0, 255]
        r.px[35][5] = [0, 0, 0, 255]
        assert not diff(r, noise_full(r, s)), "hard edges / 1px line / dot changed"
    print("ok  solid, transparent, hard-edged rect, 1px line and dot are bit-exact")

    # 2. gradient and anti-aliased circle: real content changes, only blend pixels, by exactly 1
    g = gradient(256, 12)
    gn = noise_full(g, 5)
    d = diff(g, gn)
    assert 100 < len(d) < 256 * 12 // 3, len(d)
    for x, y in d:
        ch = [abs(a - b) for a, b in zip(g.px[y][x], gn.px[y][x])]
        assert sorted(ch) == [0, 0, 0, 1] and ch[3] == 0
    c, cov = circle(60, 60)
    cn = noise_full(c, 5)
    dc = diff(c, cn)
    assert len(dc) > 8 and all(0 < cov[p] < 1 for p in dc), len(dc)  # parity noise leaves about half of the chosen pixels unchanged
    print(f"ok  gradient changes {len(d)} px, circle edge {len(dc)} px, each +-1 in one RGB channel, "
          "interior/background exact")

    # 3. deterministic, repeated drawing identical, seed dependent
    c2, _ = circle(60, 60)
    assert not diff(noise_full(c, 5), noise_full(c2, 5)) and diff(noise_full(c, 5), noise_full(c, 6))
    print("ok  deterministic for same seed/content, different for a different seed")

    # 4. getImageData(any rect, incl. partly outside canvas) == toDataURL (full image) cropped
    full = noise_full(c, 9)
    rnd = random.Random(1)
    for _ in range(200):
        sx, sy = rnd.randint(-10, 65), rnd.randint(-10, 65)
        sw, sh = rnd.randint(1, 30), rnd.randint(1, 30)
        got = get_image_data(c, 9, sx, sy, sw, sh)
        for y in range(sh):
            for x in range(sw):
                cx, cy = sx + x, sy + y
                want = full.px[cy][cx] if 0 <= cx < c.w and 0 <= cy < c.h else [0, 0, 0, 0]
                assert got[y][x] == want, (sx, sy, sw, sh, x, y)
    print("ok  200 random getImageData rectangles (some off-canvas) == toDataURL of the same pixels")

    # 5. BGRA vs RGBA give the same logical result
    cb = c.copy()
    cb.bgra = True
    for row in cb.px:
        for p in row:
            p[0], p[2] = p[2], p[0]
    nb = noise_full(cb, 9)
    for y in range(c.h):
        for x in range(c.w):
            p = nb.px[y][x]
            assert [p[2], p[1], p[0], p[3]] == full.px[y][x]
    print("ok  RGBA and BGRA buffers agree channel for channel")

    # 6. GL bottom-up read-back with y flip == top-down result
    flipped = Pm(c.w, c.h, px=[[list(p) for p in row] for row in reversed(c.px)])
    noise_region(flipped, 9, 0, c.h - 1, -1, c.w, c.h, (0, 0, c.w, c.h))
    assert not diff(Pm(c.w, c.h, px=list(reversed(flipped.px))), full)
    print("ok  readPixels (bottom-up, y_dir=-1) == toDataURL")

    # 7. write-then-read tests: tiny round trips exact, random data almost exact
    rnd = random.Random(2)
    for _ in range(50):
        w, h = rnd.randint(1, 3), rnd.randint(1, 3)
        im = Pm(w, h, px=[[[rnd.randrange(256) for _ in range(4)] for _ in range(w)] for _ in range(h)])
        assert not diff(im, noise_full(im, 3))
    changed = total = 0
    for s in range(40):
        im = Pm(64, 64, px=[[[rnd.randrange(256) for _ in range(3)] + [255] for _ in range(64)]
                           for _ in range(64)])
        changed += len(diff(im, noise_full(im, s)))
        total += 64 * 64
    print(f"ok  tiny random write/read exact; 64x64 random-data residual change rate "
          f"{changed / total:.5%}")
    assert changed / total < 0.002

    # 8. premultiplied pixels never get a channel above alpha
    pm = Pm(60, 60, premul=True)
    for y in range(60):
        for x in range(60):
            a = round(255 * x / 59)
            pm.px[y][x] = [a, a // 2, a // 3, a]
    for s in seeds:
        for row in noise_full(pm, s).px:
            for p in row:
                assert max(p[:3]) <= p[3]
    print("ok  premultiplied pixels stay valid")

    # 9. audio
    sig = [f32(math.sin(i * 0.01) * 0.5) for i in range(5000)] + [0.0] * 100 + [f32(1e-42), float('inf')]
    a1 = noise_audio(sig, 4)
    assert a1 == noise_audio(sig, 4) and a1 != noise_audio(sig, 5)
    moved = 0
    for o, n in zip(sig, a1):
        if o != n:
            moved += 1
            assert abs(bits(o) - bits(n)) == 1 and (o < 0) == (n < 0)
    assert 2000 < moved < 3000 and a1[5000:] == sig[5000:]
    assert noise_audio([0.0] * 1000, 4) == [0.0] * 1000
    assert noise_audio(sig[:3000], 4) + noise_audio(sig[3000:], 4, 3000) == a1  # chunk offset
    print(f"ok  audio: silence/denormal/inf untouched, {moved} of 5000 samples moved by exactly 1 ulp, "
          "chunk-offset safe")

    # 10. text advances
    for s in seeds:
        assert noise_advance(8 * 65536, s) == 8 * 65536
        a = int(7.3125 * 65536) + 5
        assert noise_advance(a, s) == noise_advance(a, s) and abs(noise_advance(a, s) - a) <= 512
    assert any(noise_advance(int(w * 65536) + 3, 1) != int(w * 65536) + 3 for w in (5.5, 6.4, 7.7))
    print("ok  text advances: integer advances exact, |delta| <= 1/128 px, deterministic")
    # idempotent: noising already noised data changes nothing (decoded toDataURL read back with getImageData)
_pm = Pm(64, 64, False, False)
for _y in range(64):
    for _x in range(64):
        _t = _x * 4
        _pm.px[_y][_x][:] = [10 + _t, 200 - _t, 90 + (_x ^ _y) // 8, 255]
_a = noise_full(_pm, 1234); _b = noise_full(_a, 1234)
assert _a.px == _b.px, "noise is not idempotent"
print("ok  noise is idempotent (noising twice == noising once)")
print("ALL PASSED")


if __name__ == "__main__":
    main()
