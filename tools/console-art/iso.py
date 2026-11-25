"""Isometric pixel art in the style of Hydra's console illustrations.

Screen mapping (2:1 iso): sx = 2x + 2y, sy = x - y - z. The face at y = min
looks front-left (mid tone), the face at x = max looks right (dark), the top is
light. Shapes are extruded layer by layer, so any outline (rounded consoles,
dog-bone controllers) gets proper sides. Output: RGBA PNG, no dependencies.
"""
import math, struct, sys, zlib

OUTLINE = (34, 35, 44, 255)


def hexc(h, a=255):
    h = h.lstrip('#')
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), a)


def shade(c, f):
    r, g, b, a = c
    if f >= 1:
        return (min(255, int(r + (255 - r) * (f - 1))), min(255, int(g + (255 - g) * (f - 1))), min(255, int(b + (255 - b) * (f - 1))), a)
    return (int(r * f), int(g * f), int(b * f), a)


class Iso:
    def __init__(self, w, h, ox, oy):
        self.w, self.h, self.ox, self.oy = w, h, ox, oy
        self.px = [[None] * w for _ in range(h)]
        self.depth = [[-1e9] * w for _ in range(h)]

    def project(self, x, y, z):
        return self.ox + 2 * x + 2 * y, self.oy + x - y - z

    def put(self, sx, sy, c, d=None):
        sx, sy = int(round(sx)), int(round(sy))
        if 0 <= sx < self.w and 0 <= sy < self.h:
            self.px[sy][sx] = c

    # A shape on the floor plan: inside(u, v) -> bool, within a bounding box.
    def extrude(self, inside, x0, y0, x1, y1, z, h, top, front=None, side=None, step=0.25):
        front = front or shade(top, 0.78)
        side = side or shade(top, 0.58)
        n = int(round(h / step))
        for k in range(n + 1):
            zz = z + k * step
            last = k == n
            v = y0
            while v <= y1:
                u = x0
                while u <= x1:
                    if inside(u, v):
                        if last:
                            col = top
                            # A lighter rim along the edges seen from the front.
                            if not inside(u, v - 0.75) or not inside(u + 0.75, v):
                                col = shade(top, 1.12)
                        elif not inside(u, v - step * 2):
                            col = front
                        elif not inside(u + step * 2, v):
                            col = side
                        else:
                            col = None
                        if col:
                            sx, sy = self.project(u, v, zz)
                            self.put(sx, sy, col)
                    u += step
                v += step

    def box(self, x, y, z, w, d, h, top, front=None, side=None, r=0):
        def inside(u, v):
            if r <= 0:
                return x <= u <= x + w and y <= v <= y + d
            cu = min(max(u, x + r), x + w - r)
            cv = min(max(v, y + r), y + d - r)
            return (u - cu) ** 2 + (v - cv) ** 2 <= r * r and x <= u <= x + w and y <= v <= y + d
        self.extrude(inside, x, y, x + w, y + d, z, h, top, front, side)

    def top_decal(self, inside, x0, y0, x1, y1, z, col, step=0.25):
        """Paint on a top face (z), only where inside() says."""
        v = y0
        while v <= y1:
            u = x0
            while u <= x1:
                if inside(u, v):
                    sx, sy = self.project(u, v, z + 0.01)
                    self.put(sx, sy, col)
                u += step
            v += step

    def front_decal(self, x0, x1, z0, z1, y, col, step=0.25):
        """Paint on the front face (constant y)."""
        z = z0
        while z <= z1:
            u = x0
            while u <= x1:
                sx, sy = self.project(u, y - 0.01, z)
                self.put(sx, sy, col)
                u += step
            z += step

    def side_decal(self, y0, y1, z0, z1, x, col, step=0.25):
        z = z0
        while z <= z1:
            v = y0
            while v <= y1:
                sx, sy = self.project(x + 0.01, v, z)
                self.put(sx, sy, col)
                v += step
            z += step

    def outline(self, c=OUTLINE):
        edge = []
        for j in range(self.h):
            for i in range(self.w):
                if self.px[j][i] is None and any(
                        0 <= i + dx < self.w and 0 <= j + dy < self.h and self.px[j + dy][i + dx] not in (None, c)
                        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                    edge.append((i, j))
        for i, j in edge:
            self.px[j][i] = c

    def crop(self, pad=2):
        rows = [j for j in range(self.h) if any(self.px[j])]
        cols = [i for i in range(self.w) if any(self.px[j][i] for j in range(self.h))]
        j0, j1 = max(rows[0] - pad, 0), min(rows[-1] + pad, self.h - 1)
        i0, i1 = max(cols[0] - pad, 0), min(cols[-1] + pad, self.w - 1)
        self.px = [row[i0:i1 + 1] for row in self.px[j0:j1 + 1]]
        self.h, self.w = len(self.px), len(self.px[0])

    def png(self, path, scale=2):
        raw = b''
        for row in self.px:
            line = b''.join(bytes(c if c else (0, 0, 0, 0)) * scale for c in row)
            raw += (b'\x00' + line) * scale
        def chunk(tag, data):
            return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xffffffff)
        png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', self.w * scale, self.h * scale, 8, 6, 0, 0, 0))
        png += chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
        open(path, 'wb').write(png)


def ellipse(cx, cy, rx, ry):
    return lambda u, v: ((u - cx) / rx) ** 2 + ((v - cy) / ry) ** 2 <= 1


def rect(x0, y0, x1, y1):
    return lambda u, v: x0 <= u <= x1 and y0 <= v <= y1


def union(*fs):
    return lambda u, v: any(f(u, v) for f in fs)


# ---------------------------------------------------------------- controllers

def dogbone(c, x, y, z, w, d, h, body, buttons, dpad=(40, 42, 50, 255), sticks=0):
    """A pad seen from above: a bar with two rounded grips toward the viewer."""
    r = d * 0.42
    shape = union(rect(x + r, y + d * 0.35, x + w - r, y + d), ellipse(x + r, y + d * 0.55, r, r), ellipse(x + w - r, y + d * 0.55, r, r))
    c.extrude(shape, x, y, x + w, y + d, z, h, body)
    top = z + h
    cx, cy = x + r, y + d * 0.58
    c.top_decal(union(rect(cx - 2.2, cy - 0.7, cx + 2.2, cy + 0.7), rect(cx - 0.7, cy - 2.2, cx + 0.7, cy + 2.2)), x, y, x + w, y + d, top, dpad)
    bx, by = x + w - r, y + d * 0.58
    spots = [(bx + 1.6, by), (bx, by - 1.6), (bx - 1.6, by), (bx, by + 1.6)]
    for (sx, sy), col in zip(spots, buttons):
        c.top_decal(ellipse(sx, sy, 0.75, 0.75), x, y, x + w, y + d, top, col)
    if sticks:
        for sx in (x + w * 0.38, x + w * 0.62):
            c.extrude(ellipse(sx, y + d * 0.8, 1.2, 1.2), sx - 1.3, y + d * 0.8 - 1.3, sx + 1.3, y + d * 0.8 + 1.3, top, 0.8, (60, 62, 70, 255))
    mid = x + w / 2
    c.top_decal(rect(mid - 2.4, y + d * 0.55, mid - 1.0, y + d * 0.65), x, y, x + w, y + d, top, shade(body, 0.6))
    c.top_decal(rect(mid + 1.0, y + d * 0.55, mid + 2.4, y + d * 0.65), x, y, x + w, y + d, top, shade(body, 0.6))


# ---------------------------------------------------------------- consoles

RED, YEL, GRN, BLU = hexc('#d6332d'), hexc('#e8c22c'), hexc('#2fa65a'), hexc('#2f6fd6')


def nes():
    c = Iso(320, 220, 40, 120)
    grey, dark = hexc('#c9c9cf'), hexc('#2c2d33')
    c.box(0, 0, 0, 34, 26, 9, grey)
    c.top_decal(rect(0, 0, 34, 10), 0, 0, 34, 26, 9, hexc('#b9b9c0'))          # front half of the lid
    c.top_decal(rect(9, 2, 27, 9), 0, 0, 34, 26, 9, hexc('#aeaeb6'))           # cartridge door
    c.top_decal(rect(10, 3, 26, 3.6), 0, 0, 34, 26, 9, hexc('#9a9aa2'))
    c.front_decal(0, 34, 5.5, 8.6, 0, dark)                                     # the black band
    c.front_decal(2, 7, 1.5, 3.5, 0, hexc('#8f8f98'))                           # power, reset
    c.front_decal(9, 14, 1.5, 3.5, 0, hexc('#8f8f98'))
    c.front_decal(3, 4, 4.0, 4.8, 0, RED)
    c.front_decal(19, 24, 1.2, 3.4, 0, dark); c.front_decal(26, 31, 1.2, 3.4, 0, dark)   # ports
    dogbone(c, 20, -9, 0, 24, 8, 1.6, hexc('#2c2d33'), [RED, RED, hexc('#2c2d33'), hexc('#2c2d33')], dpad=hexc('#cfcfd6'))
    return c


def snes():
    c = Iso(320, 220, 40, 120)
    body = hexc('#d2d3d9')
    c.box(0, 0, 0, 32, 26, 8, body, r=4)
    c.top_decal(rect(9, 9, 23, 17), 0, 0, 32, 26, 8, hexc('#7d7e88'))          # cartridge slot well
    c.top_decal(rect(10, 10.5, 22, 15.5), 0, 0, 32, 26, 8, hexc('#4a4b53'))
    c.top_decal(rect(3, 4, 8, 7), 0, 0, 32, 26, 8, hexc('#9c9da6'))             # power
    c.top_decal(rect(24, 4, 29, 7), 0, 0, 32, 26, 8, hexc('#9c9da6'))           # reset
    c.top_decal(rect(4, 19, 28, 20.2), 0, 0, 32, 26, 8, hexc('#8a3fa0'))        # purple trim
    c.front_decal(5, 10, 1.5, 3.5, 0, hexc('#8a8b94')); c.front_decal(12, 17, 1.5, 3.5, 0, hexc('#8a8b94'))
    dogbone(c, 18, -11, 0, 26, 10, 1.8, hexc('#cacbd2'), [hexc('#d6332d'), hexc('#e8c22c'), hexc('#2f6fd6'), hexc('#2fa65a')], dpad=hexc('#3a3b42'))
    return c


def n64():
    c = Iso(320, 220, 40, 120)
    body = hexc('#3d4048')
    c.box(0, 0, 0, 34, 28, 8, body, r=5)
    c.box(9, 14, 8, 16, 9, 2.5, hexc('#34363d'), r=1.5)                         # the raised slot block
    c.top_decal(rect(11, 16, 23, 21), 0, 0, 34, 28, 10.5, hexc('#1c1d22'))
    c.top_decal(rect(12.5, 4, 21.5, 7.5), 0, 0, 34, 28, 8, hexc('#2e3036'))
    for k, col in enumerate((RED, GRN, BLU, YEL)):
        c.top_decal(rect(13 + k * 2.1, 5, 14.4 + k * 2.1, 6.5), 0, 0, 34, 28, 8, col)
    for k in range(4):
        c.front_decal(4 + k * 7, 8 + k * 7, 1.2, 3.6, 0, hexc('#202126'))       # four ports
    # The three-handled controller: a bar and three prongs.
    g = hexc('#a3a5ad')
    x, y = 18, -13
    shape = union(rect(x + 3, y + 3, x + 25, y + 9), ellipse(x + 3, y + 7, 3.2, 3.6), ellipse(x + 25, y + 7, 3.2, 3.6),
                  rect(x + 12, y + 7, x + 16, y + 14), ellipse(x + 14, y + 14, 2.2, 1.8))
    c.extrude(shape, x, y, x + 29, y + 16, 0, 1.8, g)
    t = 1.8
    c.top_decal(union(rect(x + 2, y + 6.4, x + 5, y + 7.6), rect(x + 2.9, y + 5.5, x + 4.1, y + 8.5)), x, y, x + 29, y + 16, t, hexc('#2c2d33'))
    c.extrude(ellipse(x + 14, y + 8, 1.2, 1.2), x + 12.5, y + 6.5, x + 15.5, y + 9.5, t, 1.0, hexc('#55575f'))
    c.top_decal(ellipse(x + 23, y + 8.5, 0.9, 0.9), x, y, x + 29, y + 16, t, BLU)
    c.top_decal(ellipse(x + 21.4, y + 7.2, 0.9, 0.9), x, y, x + 29, y + 16, t, GRN)
    for sx, sy in ((x + 25.5, y + 5.2), (x + 26.8, y + 6.5), (x + 25.5, y + 7.8), (x + 24.2, y + 6.5)):
        c.top_decal(ellipse(sx, sy, 0.55, 0.55), x, y, x + 29, y + 16, t, YEL)
    return c


def genesis():
    c = Iso(320, 220, 40, 120)
    body = hexc('#2a2c33')
    c.box(0, 0, 0, 36, 24, 6, body, r=2)
    c.extrude(ellipse(18, 13, 9, 8), 9, 5, 27, 21, 6, 1.4, hexc('#33363e'))      # the round top
    c.top_decal(ellipse(18, 13, 6.5, 5.6), 9, 5, 27, 21, 7.4, hexc('#24262c'))
    c.top_decal(ellipse(18, 13, 4.3, 3.6), 9, 5, 27, 21, 7.4, hexc('#2e3037'))
    c.top_decal(rect(14, 12, 22, 14), 9, 5, 27, 21, 7.4, hexc('#15161a'))       # the slot
    c.top_decal(rect(2, 3, 8, 4.4), 0, 0, 36, 24, 6, RED)                        # red stripe
    c.top_decal(rect(29, 3, 33, 6), 0, 0, 36, 24, 6, hexc('#4a4d55'))
    c.front_decal(3, 8, 1.4, 3.2, 0, hexc('#16171b')); c.front_decal(10, 15, 1.4, 3.2, 0, hexc('#16171b'))
    dogbone(c, 20, -11, 0, 24, 10, 1.8, hexc('#2a2c33'), [hexc('#4a4d55')] * 3 + [hexc('#2a2c33')], dpad=hexc('#4a4d55'))
    return c


def sms():
    c = Iso(320, 220, 40, 120)
    body = hexc('#25272d')
    c.box(0, 0, 0, 36, 24, 6, body)
    c.box(6, 8, 6, 24, 12, 2.5, hexc('#2c2f36'))                                  # raised wedge
    c.top_decal(rect(9, 12, 27, 14.5), 6, 8, 30, 20, 8.5, hexc('#121316'))
    c.front_decal(0, 36, 2.6, 3.4, 0, RED)                                        # red line
    c.top_decal(rect(30, 2, 34, 5), 0, 0, 36, 24, 6, RED)
    c.front_decal(26, 33, 4.0, 5.2, 0, hexc('#4a4d55'))
    dogbone(c, 21, -9, 0, 22, 8, 1.6, hexc('#25272d'), [RED, RED, hexc('#25272d'), hexc('#25272d')], dpad=hexc('#4a4d55'))
    return c


def handheld(body, screen_frame, screen, buttons, wide=False, length=None):
    """A handheld lying down, screen up, like Hydra's PSP."""
    c = Iso(320, 220, 60, 120)
    if wide:
        w, d = 40, 18
        shape = union(rect(5, 0, w - 5, d), ellipse(5, d / 2, 5, d / 2), ellipse(w - 5, d / 2, 5, d / 2))
        c.extrude(shape, 0, 0, w, d, 0, 3, body)
        c.top_decal(rect(12, 3, 28, 15), 0, 0, w, d, 3, screen_frame)
        c.top_decal(rect(14, 4.6, 26, 13.4), 0, 0, w, d, 3, screen)
        c.top_decal(rect(15, 11.5, 19, 12.5), 0, 0, w, d, 3, shade(screen, 1.35))
        cx, cy = 5.5, 9
        c.top_decal(union(rect(cx - 2.2, cy - 0.7, cx + 2.2, cy + 0.7), rect(cx - 0.7, cy - 2.2, cx + 0.7, cy + 2.2)), 0, 0, w, d, 3, hexc('#1d1f24'))
        c.top_decal(ellipse(w - 6.5, 10.5, 1.2, 1.2), 0, 0, w, d, 3, buttons)
        c.top_decal(ellipse(w - 4, 8, 1.2, 1.2), 0, 0, w, d, 3, buttons)
    else:
        w, d = 22, 36
        c.box(0, 0, 0, w, d, 3.4, body, r=1.5)
        c.top_decal(rect(2, 17, 20, 33), 0, 0, w, d, 3.4, screen_frame)
        c.top_decal(rect(5, 19.5, 17, 30.5), 0, 0, w, d, 3.4, screen)
        c.top_decal(rect(6, 28.6, 10, 29.6), 0, 0, w, d, 3.4, shade(screen, 1.35))
        c.top_decal(ellipse(3.2, 25, 0.6, 0.6), 0, 0, w, d, 3.4, RED)
        cx, cy = 6, 10
        c.top_decal(union(rect(cx - 2.2, cy - 0.7, cx + 2.2, cy + 0.7), rect(cx - 0.7, cy - 2.2, cx + 0.7, cy + 2.2)), 0, 0, w, d, 3.4, hexc('#1d1f24'))
        c.top_decal(ellipse(16.5, 11.5, 1.3, 1.3), 0, 0, w, d, 3.4, buttons)
        c.top_decal(ellipse(13.8, 9.2, 1.3, 1.3), 0, 0, w, d, 3.4, buttons)
        for k in range(4):
            c.top_decal(rect(14 + k * 1.4, 2, 14.6 + k * 1.4, 5.5), 0, 0, w, d, 3.4, shade(body, 0.7))
    return c


def wiiu():
    c = Iso(320, 220, 40, 120)
    body = hexc('#2b2e35')
    c.box(0, 4, 0, 40, 12, 5.5, body, r=1)
    c.front_decal(3, 30, 2.6, 3.0, 4, hexc('#5d636e'))                             # disc slot
    c.front_decal(33, 36, 2.2, 3.4, 4, hexc('#3a3e46'))
    c.front_decal(34.2, 34.8, 2.6, 3.0, 4, hexc('#3aa0ff'))
    # The GamePad in front, lying down.
    x, y = 14, -15
    shape = union(rect(x + 4, y, x + 30, y + 16), ellipse(x + 4, y + 8, 4, 8), ellipse(x + 30, y + 8, 4, 8))
    c.extrude(shape, x, y, x + 34, y + 16, 0, 2, hexc('#24272d'))
    c.top_decal(rect(x + 9, y + 2.5, x + 25, y + 13.5), x, y, x + 34, y + 16, 2, hexc('#0e1013'))
    c.top_decal(rect(x + 10.2, y + 3.6, x + 23.8, y + 12.4), x, y, x + 34, y + 16, 2, hexc('#1f6fd1'))
    c.top_decal(rect(x + 11, y + 10.5, x + 15, y + 11.4), x, y, x + 34, y + 16, 2, hexc('#6cb3ff'))
    for sx in (x + 4, x + 30):
        c.extrude(ellipse(sx, y + 12, 1.3, 1.3), sx - 1.4, y + 10.6, sx + 1.4, y + 13.4, 2, 0.8, hexc('#4a4f58'))
    c.top_decal(union(rect(x + 2, y + 5.3, x + 6, y + 6.7), rect(x + 3.3, y + 4, x + 4.7, y + 8)), x, y, x + 34, y + 16, 2, hexc('#14161a'))
    for sx, sy in ((x + 31.5, y + 5.5), (x + 30, y + 4), (x + 28.5, y + 5.5), (x + 30, y + 7)):
        c.top_decal(ellipse(sx, sy, 0.6, 0.6), x, y, x + 34, y + 16, 2, hexc('#d9dce1'))
    return c


ART = {
    'nes': nes, 'snes': snes, 'n64': n64, 'genesis': genesis, 'sms': sms, 'wiiu': wiiu,
    'gb': lambda: handheld(hexc('#c9c9c1'), hexc('#6a6b78'), hexc('#8bac0f'), hexc('#a12d5a')),
    'gbc': lambda: handheld(hexc('#7a4bbd'), hexc('#1d1e24'), hexc('#3b8fd6'), hexc('#2a2b31')),
    'gba': lambda: handheld(hexc('#5143a8'), hexc('#1d1e24'), hexc('#3fa0a8'), hexc('#d9d9e0'), wide=True),
    'gg': lambda: handheld(hexc('#26282e'), hexc('#0d0e11'), hexc('#2f6fd6'), RED, wide=True),
}

if __name__ == '__main__':
    out = sys.argv[1]
    only = sys.argv[2:] or list(ART)
    for name in only:
        c = ART[name]()
        c.outline()
        c.crop()
        c.png(f'{out}/{name}.png', 2)
        print(name, c.w * 2, c.h * 2)
