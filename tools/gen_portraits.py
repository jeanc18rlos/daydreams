"""Generate the Backrooms' portraits (src/ext/painting.rs, Shaders/painting.frag) from the
variation sheets under assets/paintings/src/.

Each sheet is one public-domain painting, edited by hand into a BASE -- the sitter with blank
eye sockets and no mouth -- and a set of PARTS: the eyes looking at the viewer and looking to
the viewer's left, and a smiling, a sad and an angry mouth. The shader composites the parts
over the base, warps the irises toward the viewer inside each eye's ellipse, and crossfades
between the variants; this tool cuts the sheets up, finds where each part goes on the base,
and writes what the shader and the scene need:

  Textures/portrait_<name>.bmp        the base, 32-bit BGRA, cropped to the figure, on a dark
                                      painted ground inpainted from the sheet's own surround
  Textures/portrait_<name>_parts.bmp  the seven parts with alpha, packed and padded
  src/ext/portrait_atlas.rs           per portrait: sizes, each part's atlas rect and its
                                      placement rect on the base, both eyes' ellipses, all in
                                      top-origin UV (see below)

Nothing is parsed at runtime. Requires numpy and Pillow only.

UV CONVENTION: the engine's BMP loader leaves GL t=0 at the image TOP (texture.rs reads the
bottom-first rows into img[height-1] downward), so every rect and ellipse here is top-origin
-- (u, v) = (x / width, y / height) with y down the image -- and painting.frag flips the
quad's v once before sampling. Keep both halves of that in step.

THE TWO KINDS OF SHEET
* `alpha`: an RGBA sheet whose pieces are cutouts -- the figure on the left, the labelled eye
  and mouth pieces on the right, each with its own alpha, on a painted ground. A part is the
  connected component of alpha inside its manifest region (so the label text, which also has
  alpha, is never in a region). The base is the figure composited over the ground, which is
  the sheet's own surround inpainted under the figure and the pieces. Parts register against
  the base by normalised cross-correlation of the piece's opaque BORDER RING -- the lids,
  brows and skin around the socket, which the base has -- at every scale in the manifest's
  range; the socket's interior, which the base does not have, is left out of the ring.
* `black`: an RGB sheet on black whose tiles are rectangular crops of the painting -- an
  ORIGINAL tile to register against, the base tile (same framing as the original), and the
  eye and mouth crops. A tile is the widest, tallest run of non-black rows and columns inside
  its region (labels are a few rows tall and fall out). A crop registers against the original
  as a whole rectangle, and is placed on the base through the original's registration
  against the base. The crops have no alpha, so the tool gives each a soft superellipse
  mask, and a tile holding both eyes is split at the nose into a left and a right part whose
  masks cross-fade through the overlap, summing to one there: the shader composites parts
  additively in premultiplied form, so two parts may overlap only where their alphas
  partition.

ADDING A SHEET (the Hals "Laughing Cavalier" the user has, not yet saved): one more manifest
entry. For the layout the user described -- eyes and moustache/mouth PNGs -- it is an `alpha`
sheet: the base figure with the sockets and mouth blanked, and seven cutouts (eyes centre
left/right, eyes left left/right, mouth smile/sad/angry) each in its own region; if the eyes
come as one piece per variant, give that piece a `split` as the Vermeer entry does and the
tool makes two parts of it. `eye` per eye piece is the eye opening's ellipse in the piece's
own pixels (centre x, y, radius x, y): the lid edge, where the iris warp stops. Run the tool
with `--preview DIR` and look at the composites before trusting any number. Then add the
portrait to `level16.rs`'s rotation and a line to THIRD_PARTY.md.

Run from the repository root:  python3 tools/gen_portraits.py [--preview DIR]
"""
import argparse
import os
import struct
import sys

import numpy as np
from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT_RS = os.path.join(ROOT, "src", "ext", "portrait_atlas.rs")
TEX_DIR = os.path.join(ROOT, "Textures")

# The parts' pixel density over the base's: the sheets' pieces are cut at three to four times
# the base's zoom, and the eyes are where a viewer looks, so they keep twice the base's
# detail; more only costs atlas space and makes them look pasted on, sharper than the face.
PART_ZOOM = 2.0
# How far the painted glow round a cutout sheet's pieces reaches, in sheet pixels: the
# ground is not sampled that near one.
GLOW = 160
# Transparent pixels round every part in the atlas, and inside each part's rect, so the
# rect's edge samples transparent and a mipmap never pulls in a neighbour.
PAD = 4
# Part names, in the order the shader's uniform arrays take them (painting.rs `PART_ORDER`).
PART_ORDER = [
    "eyes_center_l", "eyes_center_r", "eyes_left_l", "eyes_left_r",
    "mouth_smile", "mouth_sad", "mouth_angry",
]
EYE_PARTS = PART_ORDER[:4]

# ── The manifest ─────────────────────────────────────────────────────────────────────────────
# Regions are (x0, y0, x1, y1) in sheet pixels, generous boxes round a piece and nothing else
# (no label text). `eye` ellipses are (cx, cy, rx, ry) in the piece's own pixels, hand-read
# off the piece with a grid and checked on the composite preview. `feather` is the width of
# the alpha edge softening as a fraction of the piece's height.
MANIFEST = [
    {
        "name": "mona",
        "title": "Mona Lisa (Leonardo da Vinci, c. 1503-1506); the sheet is the user's edit",
        "sheet": "assets/paintings/src/mona_sheet.png",
        "kind": "alpha",
        "base": {"region": (0, 0, 880, 1024)},
        # Where on the base the parts may land (the face), in base pixels, and the zoom range
        # to try: the pieces were cut at three to four times the base's scale.
        "register": {"search": (200, 0, 560, 420), "scales": (0.20, 0.55, 0.01), "ring": 0.30},
        "feather": 0.10,
        "parts": {
            "eyes_center_l": {"region": (800, 40, 1110, 230), "eye": (143, 77, 82, 30)},
            "eyes_center_r": {"region": (1110, 40, 1460, 230), "eye": (148, 76, 92, 30)},
            "eyes_left_l": {"region": (800, 250, 1110, 440), "eye": (142, 81, 85, 30)},
            "eyes_left_r": {"region": (1110, 250, 1460, 440), "eye": (155, 85, 95, 31)},
            "mouth_smile": {"region": (800, 470, 1180, 670)},
            "mouth_sad": {"region": (1180, 470, 1536, 670)},
            "mouth_angry": {"region": (980, 700, 1380, 920)},
        },
    },
    {
        "name": "vermeer",
        "title": "Girl with a Pearl Earring (Johannes Vermeer, c. 1665); the sheet is the user's edit",
        "sheet": "assets/paintings/src/vermeer_sheet.png",
        "kind": "black",
        "base": {"region": (0, 560, 560, 1149)},
        "reference": {"region": (0, 0, 560, 560)},
        "register": {"scales": (0.30, 0.90, 0.01)},
        # The soft mask every crop gets: a superellipse inset from the crop's rectangle,
        # opaque inside `solid` of its radius and fading to nothing at the rim.
        "mask": {"power": 3.0, "inset": 0.04, "solid": 0.70},
        "parts": {
            # Both eyes in one crop: split at the bridge of the nose (x, with this much
            # overlap either side) into the left and right parts.
            "eyes_center": {
                "region": (560, 160, 960, 420),
                "split": (128, 14),
                "eye": [(70, 69, 30, 15), (198, 86, 38, 16)],
            },
            "eyes_left": {"region": (960, 160, 1369, 420), "framing": "eyes_center"},
            "mouth_smile": {"region": (560, 505, 960, 740)},
            "mouth_sad": {"region": (960, 505, 1369, 740), "framing": "mouth_smile"},
            "mouth_angry": {"region": (700, 740, 1200, 1000), "framing": "mouth_smile"},
        },
    },
]


# ── Image helpers ────────────────────────────────────────────────────────────────────────────
def gray(rgb):
    return rgb[..., :3].astype(np.float64) @ np.array([0.299, 0.587, 0.114])


def erode(mask, k):
    m = mask.copy()
    for _ in range(k):
        e = m.copy()
        e[1:] &= m[:-1]
        e[:-1] &= m[1:]
        e[:, 1:] &= m[:, :-1]
        e[:, :-1] &= m[:, 1:]
        m = e
    return m


def dilate(mask, k):
    m = mask.copy()
    for _ in range(k):
        d = m.copy()
        d[1:] |= m[:-1]
        d[:-1] |= m[1:]
        d[:, 1:] |= m[:, :-1]
        d[:, :-1] |= m[:, 1:]
        m = d
    return m


def edge_distance(mask, limit):
    """Per pixel, how many erosions it survives, capped at `limit`: a distance from the
    mask's edge, in pixels, for the feathering."""
    d = np.zeros(mask.shape, np.float64)
    m = mask.copy()
    for _ in range(limit):
        d += m
        m = erode(m, 1)
    return d


def gauss_blur(g, sigma):
    r = max(1, int(3 * sigma))
    x = np.arange(-r, r + 1)
    k = np.exp(-0.5 * (x / sigma) ** 2)
    k /= k.sum()
    p = np.pad(g, r, mode="edge")
    t = np.apply_along_axis(lambda v: np.convolve(v, k, mode="valid"), 1, p)
    return np.apply_along_axis(lambda v: np.convolve(v, k, mode="valid"), 0, t)


def resize(arr, scale):
    """Lanczos resample of a float array by `scale`."""
    im = Image.fromarray(arr.astype(np.float32), mode="F")
    w, h = im.size
    nw, nh = max(1, int(round(w * scale))), max(1, int(round(h * scale)))
    return np.array(im.resize((nw, nh), Image.LANCZOS), dtype=np.float64)


def resize_rgba(rgba, size):
    return np.array(Image.fromarray(rgba).resize(size, Image.LANCZOS))


def inpaint(rgb, known):
    """Fill the pixels where `known` is false from the known ones: pull-push normalised
    convolution -- sums pulled down a pyramid to a single cell, then pushed back up, each
    level blending its own average (by how much of the cell is known) over the coarser
    level's, bilinearly upsampled -- so a hole the width of the figure fills with the broad
    colour round it and no block edge survives."""
    rgb = rgb.astype(np.float64)
    w = known.astype(np.float64)[..., None]
    levels = [(rgb * w, w)]
    while max(levels[-1][0].shape[:2]) > 1:
        c, m = levels[-1]
        h, wd = c.shape[:2]
        c = np.pad(c, ((0, h % 2), (0, wd % 2), (0, 0)))
        m = np.pad(m, ((0, h % 2), (0, wd % 2), (0, 0)))
        h, wd = c.shape[:2]
        c = c.reshape(h // 2, 2, wd // 2, 2, 3).sum(axis=(1, 3))
        m = m.reshape(h // 2, 2, wd // 2, 2, 1).sum(axis=(1, 3))
        levels.append((c, m))
    c, m = levels[-1]
    fill = c / np.maximum(m, 1e-9)
    for depth, (c, m) in reversed(list(enumerate(levels[:-1]))):
        h, wd = c.shape[:2]
        up = np.stack(
            [np.array(Image.fromarray(fill[..., i].astype(np.float32), mode="F").resize((wd, h), Image.BILINEAR)) for i in range(3)],
            axis=-1,
        ).astype(np.float64)
        own = c / np.maximum(m, 1e-9)
        # How much of this cell is known: its pixel count over the cell's full area.
        wt = np.clip(m / 4.0**depth, 0, 1)
        fill = own * wt + up * (1 - wt)
    return np.where(known[..., None], rgb, fill)


def runs(v, th):
    on = v > th
    out = []
    s = None
    for i, x in enumerate(on):
        if x and s is None:
            s = i
        if not x and s is not None:
            out.append((s, i))
            s = None
    if s is not None:
        out.append((s, len(v)))
    return out


# ── Cross-correlation ────────────────────────────────────────────────────────────────────────
def xcorr(img, tpl):
    """out[y, x] = sum_ji tpl[j, i] * img[y + j, x + i] over every position the template fits."""
    H, W = img.shape
    h, w = tpl.shape
    fh, fw = H + h, W + w
    F = np.fft.rfft2(img, (fh, fw))
    T = np.fft.rfft2(tpl[::-1, ::-1], (fh, fw))
    full = np.fft.irfft2(F * T, (fh, fw))
    return full[h - 1 : H, w - 1 : W]


def masked_ncc(img, tpl, mask):
    """Normalised cross-correlation of `tpl` at every position of `img`, over the pixels
    `mask` (0..1, the template's shape) weights."""
    msum = mask.sum()
    tmean = (tpl * mask).sum() / msum
    tz = (tpl - tmean) * mask
    tnorm = np.sqrt((tz * (tpl - tmean)).sum())
    num = xcorr(img, tz)
    s1 = xcorr(img, mask)
    s2 = xcorr(img * img, mask)
    var = s2 - s1 * s1 / msum
    return num / (np.sqrt(np.maximum(var, 1e-9)) * tnorm + 1e-9)


def register(img, tpl, mask, scales, search=None):
    """The (scale, x, y, score) that best places `tpl` scaled on `img`, by the mean of the
    plain and the high-pass NCC (the high-pass pins the lids, the chin and the nose; the
    plain one the broad shading), within the `search` rect of `img`."""
    sx0 = sy0 = 0
    if search is not None:
        sx0, sy0, sx1, sy1 = search
        img = img[sy0:sy1, sx0:sx1]
    img_hp = img - gauss_blur(img, 2.5)
    best = None
    for s in scales:
        t = resize(tpl, s)
        m = np.clip(resize(mask, s), 0, 1)
        if t.shape[0] >= img.shape[0] or t.shape[1] >= img.shape[1]:
            continue
        n = 0.5 * (masked_ncc(img, t, m) + masked_ncc(img_hp, t - gauss_blur(t, 2.5), m))
        y, x = np.unravel_index(np.argmax(n), n.shape)
        if best is None or n[y, x] > best[3]:
            best = (float(s), int(x + sx0), int(y + sy0), float(n[y, x]))
    return best


# ── Pieces ───────────────────────────────────────────────────────────────────────────────────
class Piece:
    """One part cut from a sheet: an RGBA image in its own pixels, where it goes on the base
    (scale and top-left in base pixels), and for an eye its opening's ellipse in its own
    pixels."""

    def __init__(self, name, rgba, eye=None):
        self.name = name
        self.rgba = rgba
        self.eye = eye
        self.scale = None
        self.at = None
        self.score = None

    @property
    def size(self):
        return self.rgba.shape[1], self.rgba.shape[0]


def alpha_component(alpha, region, min_alpha=8):
    """The largest 4-connected blob of alpha inside `region`, as a full-sheet boolean mask
    (so text and a neighbouring piece that reach into the region are left out)."""
    x0, y0, x1, y1 = region
    sub = alpha[y0:y1, x0:x1] > min_alpha
    # Label by min-propagation at quarter resolution, then refine at full resolution.
    ds = 4
    h, w = sub.shape
    H, W = (h + ds - 1) // ds, (w + ds - 1) // ds
    pad = np.zeros((H * ds, W * ds), bool)
    pad[:h, :w] = sub
    small = pad.reshape(H, ds, W, ds).any(axis=(1, 3))
    lab = np.where(small, np.arange(1, H * W + 1).reshape(H, W), 0)
    while True:
        new = lab.copy()
        for src, dst in ((lab[:-1], new[1:]), (lab[1:], new[:-1]), (lab[:, :-1], new[:, 1:]), (lab[:, 1:], new[:, :-1])):
            both = (dst > 0) & (src > 0)
            dst[both] = np.minimum(dst[both], src[both])
        if np.array_equal(new, lab):
            break
        lab = new
    ids, counts = np.unique(lab[lab > 0], return_counts=True)
    biggest = ids[np.argmax(counts)]
    cell = np.repeat(np.repeat(lab == biggest, ds, axis=0), ds, axis=1)[:h, :w]
    # Pieces are separated by clear gaps: a cell touching the blob is the blob's.
    grown = cell.copy()
    grown[1:] |= cell[:-1]
    grown[:-1] |= cell[1:]
    grown[:, 1:] |= cell[:, :-1]
    grown[:, :-1] |= cell[:, 1:]
    full = np.zeros(alpha.shape, bool)
    full[y0:y1, x0:x1] = sub & grown
    return full


def tile_rect(sheet_max, region, th=6.0, trim=1):
    """The tile inside `region` of a black sheet: the tallest run of rows whose mean
    brightness clears `th`, then the widest run of columns in it, then the rows again; less a
    pixel of the resampled halo round every tile."""
    x0, y0, x1, y1 = region
    sub = sheet_max[y0:y1, x0:x1].astype(np.float64)
    r0, r1 = max(runs(sub.mean(axis=1), th), key=lambda r: r[1] - r[0])
    c0, c1 = max(runs(sub[r0:r1].mean(axis=0), th), key=lambda c: c[1] - c[0])
    rr0, rr1 = max(runs(sub[r0:r1, c0:c1].mean(axis=1), th), key=lambda r: r[1] - r[0])
    return (x0 + c0 + trim, y0 + r0 + rr0 + trim, x0 + c1 - trim, y0 + r0 + rr1 - trim)


def bbox(mask):
    ys, xs = np.nonzero(mask)
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


def superellipse_mask(w, h, power, inset, solid):
    """A soft mask over a w x h crop: 1 inside `solid` of the superellipse inset from the
    rectangle by `inset` of its size, 0 at its rim, smooth between."""
    ys, xs = np.mgrid[0:h, 0:w]
    cx, cy = (w - 1) / 2, (h - 1) / 2
    ax, ay = cx * (1 - inset), cy * (1 - inset)
    r = (np.abs((xs - cx) / ax) ** power + np.abs((ys - cy) / ay) ** power) ** (1 / power)
    t = np.clip((r - solid) / (1 - solid), 0, 1)
    return 1 - t * t * (3 - 2 * t)


def feather(alpha, width_px):
    """Soften a cutout's alpha edge over `width_px` pixels inward."""
    opaque = alpha > 127
    d = edge_distance(opaque, int(width_px)) / max(width_px, 1)
    d = np.clip(d, 0, 1)
    soft = d * d * (3 - 2 * d)
    return (alpha.astype(np.float64) * soft).astype(np.uint8)


def match_colour(piece, base, mask_weights):
    """Scale the piece's colour so the mean under `mask_weights` (in the piece's pixels,
    where it overlaps skin the base has) matches the base under the same pixels: the sheets'
    pieces are a touch lighter than the face they come from, and a hard-edged tone step is
    what gives a pasted-on piece away."""
    s, (x, y) = piece.scale, piece.at
    w, h = piece.size
    sw, sh = int(round(w * s)), int(round(h * s))
    under = base[y : y + sh, x : x + sw, :3].astype(np.float64)
    small = resize_rgba(piece.rgba, (sw, sh)).astype(np.float64)
    wts = np.clip(resize(mask_weights, s), 0, 1)[: under.shape[0], : under.shape[1]]
    small = small[: under.shape[0], : under.shape[1]]
    wsum = max(wts.sum(), 1e-6)
    mb = (under * wts[..., None]).sum(axis=(0, 1)) / wsum
    mp = (small[..., :3] * wts[..., None]).sum(axis=(0, 1)) / wsum
    gain = np.clip(mb / np.maximum(mp, 1.0), 0.6, 1.7)
    rgb = np.clip(piece.rgba[..., :3].astype(np.float64) * gain, 0, 255)
    piece.rgba = np.concatenate([rgb.astype(np.uint8), piece.rgba[..., 3:]], axis=-1)
    return gain


# ── The two sheet kinds ──────────────────────────────────────────────────────────────────────
def load_alpha_sheet(entry):
    """An RGBA cutout sheet: the base on its inpainted ground, and the pieces with alpha."""
    sheet = np.array(Image.open(os.path.join(ROOT, entry["sheet"])).convert("RGBA"))
    alpha = sheet[..., 3]
    figure = alpha_component(alpha, entry["base"]["region"])
    masks = {name: alpha_component(alpha, spec["region"]) for name, spec in entry["parts"].items()}
    # The ground: the sheet where nothing has alpha, inpainted under everything else. The
    # pieces sit in a painted glow on the sheet, so nothing within `GLOW` of one seeds the
    # fill, and a few pixels are kept back from every edge: a piece's soft rim carries its
    # own colour at an alpha of one or two, and skin must not seed the ground either.
    near_piece = dilate(np.any(list(masks.values()), axis=0), GLOW)
    ground = inpaint(sheet[..., :3], erode(alpha == 0, 3) & ~near_piece)
    fa = np.where(figure, alpha, 0).astype(np.float64)[..., None] / 255.0
    comp = sheet[..., :3].astype(np.float64) * fa + ground * (1 - fa)
    bx0, by0, bx1, by1 = bbox(figure)
    base = np.clip(comp[by0:by1, bx0:bx1], 0, 255).astype(np.uint8)
    pieces = {}
    for name, m in masks.items():
        px0, py0, px1, py1 = bbox(m)
        rgba = sheet[py0:py1, px0:px1].copy()
        rgba[..., 3] = np.where(m[py0:py1, px0:px1], rgba[..., 3], 0)
        pieces[name] = Piece(name, rgba, entry["parts"][name].get("eye"))
        print(f"  {name}: cut {px1 - px0}x{py1 - py0} at ({px0}, {py0})")
    return base, None, pieces


def load_black_sheet(entry):
    """An RGB tiles-on-black sheet: the base tile, the original tile, and the crops, each
    given a soft mask; a both-eyes crop split into two parts."""
    sheet = np.array(Image.open(os.path.join(ROOT, entry["sheet"])).convert("RGB"))
    mx = sheet.max(axis=2)
    crop = lambda r: sheet[r[1] : r[3], r[0] : r[2]]
    base = crop(tile_rect(mx, entry["base"]["region"]))
    ref = crop(tile_rect(mx, entry["reference"]["region"]))
    mk = entry["mask"]
    pieces = {}
    tiles = {}
    for name, spec in entry["parts"].items():
        r = tile_rect(mx, spec["region"])
        rgb = crop(r)
        h, w = rgb.shape[:2]
        print(f"  {name}: tile {w}x{h} at ({r[0]}, {r[1]})")
        tiles[name] = (rgb, spec)
    for name, (rgb, spec) in tiles.items():
        framing = spec.get("framing")
        layout = tiles[framing][1] if framing else spec
        h, w = rgb.shape[:2]
        mask = superellipse_mask(w, h, mk["power"], mk["inset"], mk["solid"])
        if "split" in layout:
            sx, ov = layout["split"]
            ramp = np.clip((np.arange(w) - (sx - ov)) / (2.0 * ov), 0, 1)
            ramp = ramp * ramp * (3 - 2 * ramp)
            halves = {
                "l": (0, sx + ov, mask * (1 - ramp)[None, :], layout["eye"][0]),
                "r": (sx - ov, w, mask * ramp[None, :], layout["eye"][1]),
            }
            for side, (x0, x1, m, eye) in halves.items():
                rgba = np.dstack([rgb[:, x0:x1], (m[:, x0:x1] * 255).astype(np.uint8)])
                ex, ey, rx, ry = eye
                p = Piece(f"{name}_{side}", rgba, (ex - x0, ey, rx, ry))
                p.offset = (x0, 0)
                p.framing = framing
                p.whole = name
                pieces[p.name] = p
        else:
            rgba = np.dstack([rgb, (mask * 255).astype(np.uint8)])
            p = Piece(name, rgba, None)
            p.offset = (0, 0)
            p.framing = framing
            p.whole = name
            pieces[name] = p
    return base, ref, pieces


# ── Registration ─────────────────────────────────────────────────────────────────────────────
def register_alpha(entry, base, pieces):
    reg = entry["register"]
    lo, hi, step = reg["scales"]
    scales = np.arange(lo, hi + step / 2, step)
    bg = gray(base)
    for name in PART_ORDER:
        p = pieces[name]
        opaque = p.rgba[..., 3] > 127
        k = int(reg["ring"] * 0.5 * p.size[1])
        ring = (opaque & ~erode(opaque, k)).astype(np.float64)
        s, x, y, score = register(bg, gray(p.rgba), ring, scales, reg["search"])
        p.scale, p.at, p.score = s, (x, y), score
        gain = match_colour(p, base, ring)
        print(f"  {name}: scale {s:.3f} at ({x}, {y}) ncc {score:.3f} gain {np.round(gain, 3)}")


def register_black(entry, base, ref, pieces):
    lo, hi, step = entry["register"]["scales"]
    scales = np.arange(lo, hi + step / 2, step)
    rg, bg = gray(ref), gray(base)
    # The base against the original, at the original's scale: the same framing, give or
    # take the few pixels the sheet's layout moved it.
    inset = 40
    core = bg[inset:-inset, inset:-inset]
    n = masked_ncc(rg, core, np.ones_like(core))
    oy, ox = np.unravel_index(np.argmax(n), n.shape)
    dx, dy = inset - int(ox), inset - int(oy)  # original -> base
    print(f"  base is the original shifted by ({dx}, {dy}), ncc {n[oy, ox]:.3f}")
    placed = {}
    for name in PART_ORDER:
        p = pieces[name]
        key = p.framing or p.whole
        if key not in placed:
            # Register the whole tile this part came from (or the one it shares a framing
            # with) against the original, as a rectangle with a little border left out.
            whole = next(q for q in pieces.values() if q.whole == key)
            tile_rgb = whole_tile(pieces, key)
            mask = np.zeros(tile_rgb.shape[:2])
            mask[3:-3, 3:-3] = 1
            placed[key] = register(rg, gray(tile_rgb), mask, scales)
            s, x, y, score = placed[key]
            print(f"  {key}: scale {s:.3f} at ({x}, {y}) on the original, ncc {score:.3f}")
        s, x, y, score = placed[key]
        p.scale = s
        p.at = (int(round(x + dx + p.offset[0] * s)), int(round(y + dy + p.offset[1] * s)))
        p.score = score
        # Match the tone over the mask's soft band, which crosses skin the base has, not
        # over the middle, where the crop has eyes or lips the base does not.
        a = p.rgba[..., 3] / 255.0
        gain = match_colour(p, base, 4.0 * a * (1 - a))
        print(f"  {name}: at ({p.at[0]}, {p.at[1]}) on the base, gain {np.round(gain, 3)}")


def whole_tile(pieces, name):
    """The RGB of a tile split into parts, put back together for registration."""
    parts = [p for p in pieces.values() if p.whole == name]
    w = max(p.offset[0] + p.size[0] for p in parts)
    h = max(p.offset[1] + p.size[1] for p in parts)
    out = np.zeros((h, w, 3), np.uint8)
    for p in parts:
        x0, y0 = p.offset
        out[y0 : y0 + p.size[1], x0 : x0 + p.size[0]] = p.rgba[..., :3]
    return out


# ── Output ───────────────────────────────────────────────────────────────────────────────────
def write_bmp32(path, rgba):
    """32-bit uncompressed BMP, BGRA, bottom-first rows, 54-byte header (data offset 54):
    what texture.rs reads."""
    h, w = rgba.shape[:2]
    bgra = rgba[::-1, :, [2, 1, 0, 3]].astype(np.uint8)
    data = bgra.tobytes()
    hdr = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
    hdr += struct.pack("<IiiHHIIiiII", 40, w, h, 1, 32, 0, len(data), 2835, 2835, 0, 0)
    with open(path, "wb") as f:
        f.write(hdr + data)


def shelves(images, width):
    """Shelf-pack RGBA images (name -> array), tallest first, into rows of `width`: the
    rects (x, y, w, h) of each image's own pixels, PAD clear round each, and the height used."""
    rects = {}
    x = y = shelf = 0
    for n in sorted(images, key=lambda n: -images[n].shape[0]):
        h, w = images[n].shape[:2]
        if x + w + 2 * PAD > width:
            x = 0
            y += shelf
            shelf = 0
        rects[n] = (x + PAD, y + PAD, w, h)
        x += w + 2 * PAD
        shelf = max(shelf, h + 2 * PAD)
    return rects, y + shelf


def pack(images):
    """The images shelf-packed into the smallest atlas a power-of-two width gives (the height
    is rounded to a multiple of four; the loader takes any size): the atlas and the rects."""
    best = None
    for width in (128, 256, 512, 1024):
        rects, used = shelves(images, width)
        if max(r[0] + r[2] for r in rects.values()) + PAD > width:
            continue
        height = (used + 3) // 4 * 4
        if best is None or width * height < best[0] * best[1]:
            best = (width, height, rects)
    width, height, rects = best
    atlas = np.zeros((height, width, 4), np.uint8)
    for n, (rx, ry, w, h) in rects.items():
        atlas[ry : ry + h, rx : rx + w] = images[n]
    # Bleed the parts' colour into the clear texels: the atlas is mipmapped and sampled
    # bilinearly, and averaging a part's rim with transparent BLACK would ring every eye
    # and mouth with a dark fringe at distance.
    filled = inpaint(atlas[..., :3], atlas[..., 3] > 0)
    atlas = np.dstack([np.clip(filled, 0, 255).astype(np.uint8), atlas[..., 3]])
    return atlas, rects


def composite(base, pieces, names, ellipses=()):
    """A preview: the named pieces placed on the base, the ellipses drawn, as a PIL image."""
    from PIL import ImageDraw

    im = Image.fromarray(base).convert("RGBA")
    for n in names:
        p = pieces[n]
        s = p.scale
        w, h = p.size
        piece = Image.fromarray(p.rgba).resize((int(round(w * s)), int(round(h * s))), Image.LANCZOS)
        im.alpha_composite(piece, p.at)
    d = ImageDraw.Draw(im)
    for cx, cy, rx, ry in ellipses:
        d.ellipse([cx - rx, cy - ry, cx + rx, cy + ry], outline=(0, 255, 0, 255))
    return im.convert("RGB")


def generate(entry, preview):
    name = entry["name"]
    print(f"{name}: {entry['sheet']}")
    if entry["kind"] == "alpha":
        base, ref, pieces = load_alpha_sheet(entry)
        register_alpha(entry, base, pieces)
        for p in pieces.values():
            p.rgba[..., 3] = feather(p.rgba[..., 3], entry["feather"] * p.size[1])
    else:
        base, ref, pieces = load_black_sheet(entry)
        register_black(entry, base, ref, pieces)
    bh, bw = base.shape[:2]
    # The parts, resampled to PART_ZOOM times the base's density, padded inside their rects.
    images = {}
    placements = {}
    eyes = {}
    for n in PART_ORDER:
        p = pieces[n]
        s = p.scale
        w, h = p.size
        zoom = min(1.0, PART_ZOOM * s)
        small = resize_rgba(p.rgba, (max(1, int(round(w * zoom))), max(1, int(round(h * zoom)))))
        padded = np.zeros((small.shape[0] + 2 * PAD, small.shape[1] + 2 * PAD, 4), np.uint8)
        padded[PAD:-PAD, PAD:-PAD] = small
        images[n] = padded
        # Where the padded image lands on the base, in base pixels: the piece's placement
        # grown by the padding at the part's own scale.
        px = s / zoom
        x0, y0 = p.at[0] - PAD * px, p.at[1] - PAD * px
        x1, y1 = x0 + padded.shape[1] * px, y0 + padded.shape[0] * px
        placements[n] = (x0 / bw, y0 / bh, x1 / bw, y1 / bh)
        if p.eye is not None:
            cx, cy, rx, ry = p.eye
            eyes[n] = ((p.at[0] + cx * s) / bw, (p.at[1] + cy * s) / bh, rx * s / bw, ry * s / bh)
    atlas, rects = pack(images)
    ah, aw = atlas.shape[:2]
    write_bmp32(os.path.join(TEX_DIR, f"portrait_{name}.bmp"), np.dstack([base, np.full(base.shape[:2], 255, np.uint8)]))
    write_bmp32(os.path.join(TEX_DIR, f"portrait_{name}_parts.bmp"), atlas)
    print(f"  base {bw}x{bh}, parts atlas {aw}x{ah}")
    if preview:
        os.makedirs(preview, exist_ok=True)
        ell = [
            (pieces[n].at[0] + pieces[n].eye[0] * pieces[n].scale, pieces[n].at[1] + pieces[n].eye[1] * pieces[n].scale,
             pieces[n].eye[2] * pieces[n].scale, pieces[n].eye[3] * pieces[n].scale)
            for n in EYE_PARTS[:2]
        ]
        composite(base, pieces, ["eyes_center_l", "eyes_center_r", "mouth_smile"]).save(os.path.join(preview, f"{name}_center_smile.png"))
        composite(base, pieces, ["eyes_left_l", "eyes_left_r", "mouth_sad"]).save(os.path.join(preview, f"{name}_left_sad.png"))
        composite(base, pieces, ["eyes_center_l", "eyes_center_r", "mouth_angry"], ell).save(os.path.join(preview, f"{name}_center_angry_ellipses.png"))
        Image.fromarray(base).save(os.path.join(preview, f"{name}_base.png"))
        Image.fromarray(atlas).save(os.path.join(preview, f"{name}_parts.png"))
    return {
        "name": name,
        "title": entry["title"],
        "base_size": (bw, bh),
        "parts_size": (aw, ah),
        "atlas": {n: (rects[n][0] / aw, rects[n][1] / ah, (rects[n][0] + rects[n][2]) / aw, (rects[n][1] + rects[n][3]) / ah) for n in PART_ORDER},
        "place": placements,
        "eyes": eyes,
        "scores": {n: pieces[n].score for n in PART_ORDER},
    }


def write_rust(results):
    v4 = lambda t: "[" + ", ".join(f"{x:.5f}" for x in t) + "]"
    with open(OUT_RS, "w") as f:
        f.write("//! EXT: generated by tools/gen_portraits.py -- DO NOT EDIT.\n")
        f.write("//! The Backrooms' portraits: each base texture, the parts atlas over it, and where every\n")
        f.write("//! part and eye sits, in top-origin UV (see the tool's UV note).\n")
        f.write("// A table: the renderer reads the rects and the ellipses, the tests read the rest.\n")
        f.write("#![allow(dead_code)]\n\n")
        f.write("/// One part: its rect in the parts atlas and its placement rect on the base, both as\n")
        f.write("/// (u0, v0, u1, v1) with v measured from the image TOP.\n")
        f.write("#[derive(Clone, Copy, Debug)]\n")
        f.write("pub struct Part {\n    pub atlas: [f32; 4],\n    pub place: [f32; 4],\n}\n\n")
        f.write("/// One portrait. `parts` are in `PART_ORDER`: eyes centre left/right, eyes left\n")
        f.write("/// left/right, mouth smile/sad/angry (\"left\" and \"right\" are the image's, the viewer's).\n")
        f.write("/// `eyes` are the eye openings' ellipses as (cx, cy, rx, ry) in base UV, left then right,\n")
        f.write("/// the same for every variant.\n")
        f.write("#[derive(Clone, Copy, Debug)]\n")
        f.write("pub struct Portrait {\n")
        f.write("    pub name: &'static str,\n    pub base: &'static str,\n    pub parts_texture: &'static str,\n")
        f.write("    pub base_size: (u32, u32),\n    pub parts_size: (u32, u32),\n")
        f.write("    pub parts: [Part; 7],\n    pub eyes: [[f32; 4]; 2],\n}\n\n")
        f.write("pub const PART_ORDER: [&str; 7] = [" + ", ".join(f'"{n}"' for n in PART_ORDER) + "];\n\n")
        f.write(f"pub const PORTRAITS: [Portrait; {len(results)}] = [\n")
        for r in results:
            f.write(f"    // {r['title']}\n")
            f.write("    Portrait {\n")
            f.write(f"        name: \"{r['name']}\",\n")
            f.write(f"        base: \"portrait_{r['name']}.bmp\",\n")
            f.write(f"        parts_texture: \"portrait_{r['name']}_parts.bmp\",\n")
            f.write(f"        base_size: ({r['base_size'][0]}, {r['base_size'][1]}),\n")
            f.write(f"        parts_size: ({r['parts_size'][0]}, {r['parts_size'][1]}),\n")
            f.write("        parts: [\n")
            for n in PART_ORDER:
                f.write(f"            // {n} (ncc {r['scores'][n]:.2f})\n")
                f.write(f"            Part {{ atlas: {v4(r['atlas'][n])}, place: {v4(r['place'][n])} }},\n")
            f.write("        ],\n")
            f.write(f"        eyes: [{v4(r['eyes']['eyes_center_l'])}, {v4(r['eyes']['eyes_center_r'])}],\n")
            f.write("    },\n")
        f.write("];\n")
    print(f"rust -> {os.path.relpath(OUT_RS, ROOT)}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--preview", metavar="DIR", help="write composite previews here (not into the repo)")
    ap.add_argument("--only", metavar="NAME", help="generate one portrait (the Rust file still lists all)")
    args = ap.parse_args()
    results = []
    for entry in MANIFEST:
        if args.only and entry["name"] != args.only:
            continue
        results.append(generate(entry, args.preview))
    if args.only:
        print("(--only: src/ext/portrait_atlas.rs not rewritten)")
        return
    write_rust(results)


if __name__ == "__main__":
    sys.exit(main())
