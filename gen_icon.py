"""
KitsuneEngine icon generator — geometric fox face, cyberpunk orange-on-dark.
Produces kitsune-icon.png (256x256 RGBA) + kitsune.ico (16/32/48/64/128/256).
Uses 4× supersampling then LANCZOS downsample for clean anti-aliasing.
"""
from PIL import Image, ImageDraw
import math, os

SCALE = 4          # supersampling factor
BASE  = 256        # logical size

W = BASE * SCALE   # internal canvas size (1024×1024)

# ── Color palette (RGBA) ─────────────────────────────────────────────────
BG      = ( 8,   8,  10, 255)   # BG_VOID  #08080A
PANEL   = (16,  16,  20, 255)   # BG_PANEL for inner detail
ORANGE  = (249, 115,  22, 255)  # brand orange #F97316
ORANGE2 = (255, 148,  50, 255)  # highlight orange
DARK_OR = (155,  68,   8, 255)  # shadow orange
WHITE   = (240, 240, 245, 255)  # TEXT_PRIMARY (snout mask)
DARK_EY = ( 8,   8,  10, 255)  # same as BG for pupil bg

def p(x):
    """Scale a logical-256 coordinate to the supersampled canvas."""
    return int(x * SCALE)

def draw_rounded_rect(draw, x0, y0, x1, y1, r, fill):
    """Filled rounded rectangle without using rounded_rectangle (older Pillow compat)."""
    draw.rectangle([x0 + r, y0, x1 - r, y1], fill=fill)
    draw.rectangle([x0, y0 + r, x1, y1 - r], fill=fill)
    draw.ellipse([x0, y0, x0 + 2*r, y0 + 2*r], fill=fill)
    draw.ellipse([x1 - 2*r, y0, x1, y0 + 2*r], fill=fill)
    draw.ellipse([x0, y1 - 2*r, x0 + 2*r, y1], fill=fill)
    draw.ellipse([x1 - 2*r, y1 - 2*r, x1, y1], fill=fill)

def make_icon():
    img  = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # ── Background ──────────────────────────────────────────────────────
    r_bg = p(36)
    draw_rounded_rect(draw, 0, 0, W - 1, W - 1, r_bg, BG)

    # Subtle inner gradient-like panel (slightly lighter center rectangle)
    draw_rounded_rect(draw, p(18), p(18), W - p(18) - 1, W - p(18) - 1, p(24), PANEL)

    # ── Orange "glow" ellipse behind head ───────────────────────────────
    # Very subtle warm halo
    glow_col = (249, 115, 22, 28)
    draw.ellipse([p(20), p(60), p(235), p(245)], fill=glow_col)

    # ── Fox ears ────────────────────────────────────────────────────────
    # Left ear (orange outer)
    draw.polygon([
        (p(38),  p(128)),   # bottom-left
        (p(72),  p(22)),    # tip
        (p(122), p(105)),   # bottom-right
    ], fill=ORANGE)

    # Right ear (orange outer)
    draw.polygon([
        (p(133), p(105)),   # bottom-left
        (p(183), p(22)),    # tip
        (p(217), p(128)),   # bottom-right
    ], fill=ORANGE)

    # Left ear inner (BG, smaller triangle inset)
    draw.polygon([
        (p(58),  p(118)),
        (p(76),  p(50)),
        (p(108), p(103)),
    ], fill=PANEL)

    # Right ear inner
    draw.polygon([
        (p(147), p(103)),
        (p(179), p(50)),
        (p(197), p(118)),
    ], fill=PANEL)

    # ── Fox face (main head ellipse) ─────────────────────────────────────
    draw.ellipse([p(32), p(88), p(223), p(235)], fill=ORANGE)

    # Bridge between ears and face top
    draw.polygon([
        (p(82),  p(108)),
        (p(128), p(95)),
        (p(173), p(108)),
        (p(150), p(120)),
        (p(105), p(120)),
    ], fill=ORANGE)

    # ── White snout mask (lower half of face) ────────────────────────────
    draw.ellipse([p(62), p(148), p(193), p(237)], fill=WHITE)

    # ── Eyes ─────────────────────────────────────────────────────────────
    # Eye sockets (dark oval on orange face)
    draw.ellipse([p(60), p(116), p(108), p(152)], fill=BG)
    draw.ellipse([p(147), p(116), p(195), p(152)], fill=BG)

    # Iris (orange ring)
    draw.ellipse([p(65), p(121), p(103), p(147)], fill=ORANGE2)
    draw.ellipse([p(152), p(121), p(190), p(147)], fill=ORANGE2)

    # Pupil (dark center dot)
    draw.ellipse([p(75), p(127), p(93), p(141)], fill=BG)
    draw.ellipse([p(162), p(127), p(180), p(141)], fill=BG)

    # Eye glint (tiny bright dot, upper-right of each pupil)
    draw.ellipse([p(88), p(128), p(93), p(133)], fill=WHITE)
    draw.ellipse([p(175), p(128), p(180), p(133)], fill=WHITE)

    # ── Nose ─────────────────────────────────────────────────────────────
    draw.polygon([
        (p(117), p(180)),
        (p(128), p(194)),
        (p(139), p(180)),
    ], fill=DARK_OR)

    # ── Subtle cheek divider lines ────────────────────────────────────────
    # Small horizontal line each side of nose (kitsune whisker lines, optional)
    lw = p(1.5)
    wc = (180, 180, 190, 100)  # faint grey
    # Left whisker
    draw.line([(p(65), p(185)), (p(112), p(182))], fill=wc, width=lw)
    draw.line([(p(65), p(192)), (p(110), p(191))], fill=wc, width=lw)
    # Right whisker
    draw.line([(p(143), p(182)), (p(190), p(185))], fill=wc, width=lw)
    draw.line([(p(145), p(191)), (p(190), p(192))], fill=wc, width=lw)

    # ── Orange chin accent bar (brand stripe at bottom) ──────────────────
    bar_h = p(7)
    bar_y0 = W - p(20) - bar_h
    bar_x0, bar_x1 = p(64), W - p(64)
    bar_r = bar_h // 2
    draw.rectangle([bar_x0 + bar_r, bar_y0, bar_x1 - bar_r, bar_y0 + bar_h], fill=ORANGE)
    draw.ellipse([bar_x0, bar_y0, bar_x0 + 2*bar_r, bar_y0 + 2*bar_r], fill=ORANGE)
    draw.ellipse([bar_x1 - 2*bar_r, bar_y0, bar_x1, bar_y0 + 2*bar_r], fill=ORANGE)

    # ── Downsample to 256×256 ─────────────────────────────────────────────
    out = img.resize((BASE, BASE), Image.LANCZOS)
    return out

def make_all_sizes():
    base = make_icon()

    sizes = [16, 24, 32, 48, 64, 128, 256]
    icons = []
    for s in sizes:
        if s == BASE:
            icons.append(base)
        else:
            # For very small sizes draw simplified version at SCALE*s then downsample
            ss = s * SCALE
            mini = Image.new("RGBA", (ss, ss), (0, 0, 0, 0))
            d    = ImageDraw.Draw(mini)
            sp   = lambda x: int(x * ss / 256)

            r_bg = sp(36)
            draw_rounded_rect(d, 0, 0, ss-1, ss-1, r_bg, BG)

            # Ears
            d.polygon([(sp(38), sp(128)), (sp(72), sp(22)), (sp(122), sp(105))], fill=ORANGE)
            d.polygon([(sp(133), sp(105)), (sp(183), sp(22)), (sp(217), sp(128))], fill=ORANGE)
            d.polygon([(sp(58), sp(118)), (sp(76), sp(50)), (sp(108), sp(103))], fill=PANEL)
            d.polygon([(sp(147), sp(103)), (sp(179), sp(50)), (sp(197), sp(118))], fill=PANEL)

            # Face
            d.ellipse([sp(32), sp(88), sp(223), sp(235)], fill=ORANGE)
            d.polygon([(sp(82), sp(108)), (sp(128), sp(95)), (sp(173), sp(108)),
                       (sp(150), sp(120)), (sp(105), sp(120))], fill=ORANGE)

            # Snout
            d.ellipse([sp(62), sp(148), sp(193), sp(237)], fill=WHITE)

            # Eyes
            d.ellipse([sp(60), sp(116), sp(108), sp(152)], fill=BG)
            d.ellipse([sp(147), sp(116), sp(195), sp(152)], fill=BG)
            d.ellipse([sp(65), sp(121), sp(103), sp(147)], fill=ORANGE2)
            d.ellipse([sp(152), sp(121), sp(190), sp(147)], fill=ORANGE2)
            d.ellipse([sp(75), sp(127), sp(93), sp(141)], fill=BG)
            d.ellipse([sp(162), sp(127), sp(180), sp(141)], fill=BG)

            icons.append(mini.resize((s, s), Image.LANCZOS))

    return icons, base

# ── Output paths ─────────────────────────────────────────────────────────
out_dir = os.path.join(os.path.dirname(__file__), "crates", "kitsune-ui", "assets")
os.makedirs(out_dir, exist_ok=True)

icons, base256 = make_all_sizes()

# PNG (256×256) for embedding in binary
png_path = os.path.join(out_dir, "kitsune-icon.png")
base256.save(png_path, "PNG")
print("PNG saved: " + png_path)

# ICO (multi-size) to replace root kitsune.ico
ico_path = os.path.join(os.path.dirname(__file__), "kitsune.ico")
icons[0].save(
    ico_path,
    format="ICO",
    sizes=[(i.width, i.height) for i in icons],
    append_images=icons[1:],
)
print("ICO saved: " + ico_path)
print("Done.")
