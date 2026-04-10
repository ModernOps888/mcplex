"""Build LinkedIn banner at exactly 1584x396 from scratch."""
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import math, os

W, H = 1584, 396
OUT = r"C:\Users\bchmi\.gemini\antigravity\brain\0b117669-bfb1-4739-b490-51ec64a6636c\linkedin_banner_built.png"

# Colors
BG = (6, 6, 18)
CYAN = (0, 229, 255)
GREEN = (105, 240, 174)
PURPLE = (179, 136, 255)
GOLD = (255, 183, 77)
MAGENTA = (255, 64, 129)
ORANGE = (255, 145, 0)
WHITE = (230, 230, 230)
DIM = (120, 120, 140)
CARD_BG = (16, 18, 35)
CARD_BORDER = (40, 42, 65)
DIVIDER = (30, 50, 70)

def try_font(size, bold=False):
    """Try to load a good font, fallback to default."""
    names = [
        "C:/Windows/Fonts/consola.ttf" if not bold else "C:/Windows/Fonts/consolab.ttf",
        "C:/Windows/Fonts/segoeui.ttf" if not bold else "C:/Windows/Fonts/segoeuib.ttf",
    ]
    for n in names:
        try:
            return ImageFont.truetype(n, size)
        except:
            pass
    return ImageFont.load_default()

# Fonts
font_header = try_font(13, bold=True)
font_title = try_font(15, bold=True)
font_sub = try_font(10)
font_badge = try_font(9, bold=True)
font_stat = try_font(12, bold=True)
font_small = try_font(8)
font_footer = try_font(9)
font_big = try_font(18, bold=True)

# Create image
img = Image.new("RGB", (W, H), BG)
draw = ImageDraw.Draw(img)

# --- Background effects ---
# Subtle radial glow
glow = Image.new("RGB", (W, H), (0, 0, 0))
glow_draw = ImageDraw.Draw(glow)
for r in range(300, 0, -2):
    alpha = int(8 * (1 - r/300))
    c = (alpha, alpha*2, alpha*3)
    glow_draw.ellipse([W//2-r, H//2-r-50, W//2+r, H//2+r-50], fill=c)
img = Image.blend(img, glow, 0.5)
draw = ImageDraw.Draw(img)

# Stardust particles
import random
random.seed(42)
for _ in range(80):
    x, y = random.randint(0, W), random.randint(0, H)
    brightness = random.randint(30, 80)
    size = random.choice([1, 1, 1, 2])
    c = (brightness//2, brightness, brightness + 20) if random.random() > 0.5 else (brightness, brightness//2, brightness)
    draw.ellipse([x, y, x+size, y+size], fill=c)

# --- Header bar ---
header_h = 32
draw.rectangle([0, 0, W, header_h], fill=(8, 10, 22))
draw.line([0, header_h, W, header_h], fill=(CYAN[0]//4, CYAN[1]//4, CYAN[2]//4), width=1)

header_text = "∞  INFINITYTECHSTACK.UK  •  AI  •  COMPILERS  •  FREE TOOLS  •  ACADEMIES  •  OPEN SOURCE"
tw = draw.textlength(header_text, font=font_header)
draw.text(((W - tw) / 2, 8), header_text, fill=CYAN, font=font_header)

# --- Column positions ---
col_w = W // 4
col_starts = [col_w * i for i in range(4)]
content_top = header_h + 8

# Divider lines between columns
for i in range(1, 4):
    x = col_starts[i]
    for y in range(content_top + 10, H - 30, 4):
        alpha = 1 if y % 8 < 4 else 0
        if alpha:
            draw.line([x, y, x, y+2], fill=DIVIDER, width=1)

# --- Helper functions ---
def draw_card(x, y, w, h, border_color=CARD_BORDER):
    """Draw a glassmorphism-style card with corner brackets."""
    draw.rounded_rectangle([x, y, x+w, y+h], radius=6, fill=CARD_BG, outline=border_color)
    # Corner brackets
    blen = 10
    bc = tuple(min(255, c+30) for c in border_color)
    # Top-left
    draw.line([x, y, x+blen, y], fill=bc, width=1)
    draw.line([x, y, x, y+blen], fill=bc, width=1)
    # Top-right
    draw.line([x+w-blen, y, x+w, y], fill=bc, width=1)
    draw.line([x+w, y, x+w, y+blen], fill=bc, width=1)
    # Bottom-left
    draw.line([x, y+h, x+blen, y+h], fill=bc, width=1)
    draw.line([x, y+h-blen, x, y+h], fill=bc, width=1)
    # Bottom-right
    draw.line([x+w-blen, y+h, x+w, y+h], fill=bc, width=1)
    draw.line([x+w, y+h-blen, x+w, y+h], fill=bc, width=1)

def draw_badge(x, y, text, color, bg_alpha=30):
    """Draw a colored badge/tag."""
    tw = draw.textlength(text, font=font_badge)
    pad_x, pad_y = 6, 3
    bg_color = (color[0]//8, color[1]//8, color[2]//8)
    draw.rounded_rectangle([x, y, x+tw+pad_x*2, y+14+pad_y], radius=3, fill=bg_color, outline=tuple(c//3 for c in color))
    draw.text((x+pad_x, y+pad_y), text, fill=color, font=font_badge)
    return tw + pad_x * 2 + 5  # Return width for next badge

def draw_col_title(col_idx, text):
    """Draw column section title."""
    x = col_starts[col_idx] + 14
    y = content_top
    draw.text((x, y), text, fill=DIM, font=font_small)
    return y + 16

# ============================
# COLUMN 1: AI & AGI Research
# ============================
y = draw_col_title(0, "AI & AGI RESEARCH")
cx = col_starts[0] + 10
cw = col_w - 24

# Neuromantix card
draw_card(cx, y, cw, 125, border_color=(0, 80, 100))
draw.text((cx+10, y+8), "🧠", font=font_title)
draw.text((cx+30, y+8), "Neuromantix AGI", fill=CYAN, font=font_title)
draw.text((cx+10, y+28), "Self-Conscious Neuromorphic AGI", fill=DIM, font=font_small)

bx = cx + 10
by = y + 44
bx += draw_badge(bx, by, "133 Modules", CYAN)
bx += draw_badge(bx, by, "73K LOC", CYAN)

bx = cx + 10
by = y + 64
bx += draw_badge(bx, by, "Consciousness Loop", PURPLE)
bx += draw_badge(bx, by, "Free Energy", PURPLE)

bx = cx + 10
by = y + 84
bx += draw_badge(bx, by, "Spiking Neurons", PURPLE)
bx += draw_badge(bx, by, "ARC-AGI", CYAN)

by = y + 104
draw.text((cx+10, by), "ACTIVE", fill=GREEN, font=font_badge)

# Void LLM card
y2 = y + 135
draw_card(cx, y2, cw, 95, border_color=(80, 30, 60))
draw.text((cx+10, y2+8), "⚡", font=font_title)
draw.text((cx+30, y2+8), "Void LLM", fill=PURPLE, font=font_title)
draw.text((cx+10, y2+28), "From-Scratch LLM Engine", fill=DIM, font=font_small)

bx = cx + 10
by = y2 + 44
bx += draw_badge(bx, by, "7,800 tok/s", MAGENTA)
bx += draw_badge(bx, by, "CUDA", MAGENTA)

bx = cx + 10
by = y2 + 64
bx += draw_badge(bx, by, "3B Params", PURPLE)
bx += draw_badge(bx, by, "GPU Accel", PURPLE)

# ============================
# COLUMN 2: Compilers & Systems
# ============================
y = draw_col_title(1, "COMPILERS & SYSTEMS")
cx = col_starts[1] + 10
cw = col_w - 24

# Vitalis card
draw_card(cx, y, cw, 145, border_color=(30, 80, 50))
draw.text((cx+10, y+8), "⚙️", font=font_title)
draw.text((cx+30, y+8), "Vitalis Compiler", fill=GREEN, font=font_title)
draw.text((cx+10, y+28), "Self-Hosting • AI-Native", fill=DIM, font=font_small)

# Flow diagram
fy = y + 46
flow_items = ["Source", "IR", "Cranelift", "Binary"]
fx = cx + 10
for i, item in enumerate(flow_items):
    tw = draw.textlength(item, font=font_badge)
    draw.rounded_rectangle([fx, fy, fx+tw+10, fy+16], radius=3, fill=(15, 40, 25), outline=(40, 100, 60))
    draw.text((fx+5, fy+3), item, fill=GREEN, font=font_badge)
    fx += tw + 14
    if i < len(flow_items) - 1:
        draw.text((fx-6, fy+2), "→", fill=DIM, font=font_badge)
        fx += 6

bx = cx + 10
by = y + 70
bx += draw_badge(bx, by, "345 Modules", GREEN)
bx += draw_badge(bx, by, "6,742 Tests", GREEN)

by = y + 92
draw.text((cx+10, by), "139× Faster Than Python", fill=GREEN, font=font_stat)

bx = cx + 10
by = y + 112
bx += draw_badge(bx, by, "Cranelift JIT", GREEN)
bx += draw_badge(bx, by, "SIMD", GREEN)

by = y + 130
draw.text((cx+10, by), "ACTIVE", fill=GREEN, font=font_badge)

# Freedom OS card
y2 = y + 155
draw_card(cx, y2, cw, 75, border_color=(80, 60, 25))
draw.text((cx+10, y2+8), "🖥️", font=font_title)
draw.text((cx+30, y2+8), "Freedom OS", fill=GOLD, font=font_title)
draw.text((cx+10, y2+28), "Bare-metal x86_64 Kernel", fill=DIM, font=font_small)

bx = cx + 10
by = y2 + 44
bx += draw_badge(bx, by, "8.8K LOC", GOLD)
bx += draw_badge(bx, by, "UEFI", GOLD)
bx += draw_badge(bx, by, "GUI", GOLD)

# ============================
# COLUMN 3: Free Developer Tools
# ============================
y = draw_col_title(2, "FREE DEVELOPER TOOLS")
cx = col_starts[2] + 10
cw = col_w - 24

# Big stat
draw.text((cx, y), "1.8M+", fill=GREEN, font=font_big)
draw.text((cx + draw.textlength("1.8M+ ", font=font_big), y+4), "Monthly Searches", fill=DIM, font=font_sub)
y += 28

# Tool cards in 2-column mini grid
tools = [
    ("🔍", "Forge SEO", "85K", CYAN),
    ("📄", "ResumeForge", "900K", GREEN),
    ("🎨", "ColorForge", "480K", MAGENTA),
    ("🏗️", "SchemaForge", "110K", ORANGE),
    ("⚙️", "JSONForge", "300K", GOLD),
    ("🧰", "Claude Toolkit", "16 files", CYAN),
]

mini_w = (cw - 6) // 2
for i, (icon, name, stat, color) in enumerate(tools):
    col = i % 2
    row = i // 2
    tx = cx + col * (mini_w + 6)
    ty = y + row * 36
    
    draw.rounded_rectangle([tx, ty, tx+mini_w, ty+30], radius=4, fill=CARD_BG, outline=CARD_BORDER)
    draw.text((tx+6, ty+4), icon, font=font_small)
    draw.text((tx+22, ty+4), name, fill=WHITE, font=font_badge)
    
    stat_w = draw.textlength(stat, font=font_small)
    draw.text((tx+mini_w-stat_w-6, ty+6), stat, fill=color, font=font_small)

y += 115
bx = cx
bx += draw_badge(bx, y, "ALL FREE", GREEN)
bx += draw_badge(bx, y, "NO SIGN-UP", GREEN)

y += 22
draw.text((cx, y), "// 7 TOOLS • OPEN SOURCE", fill=(60, 65, 80), font=font_small)

# ============================
# COLUMN 4: Learn & Open Source
# ============================
y = draw_col_title(3, "LEARN & OPEN SOURCE")
cx = col_starts[3] + 10
cw = col_w - 24

# Claude Academy
draw_card(cx, y, cw, 68, border_color=(80, 55, 15))
draw.text((cx+10, y+6), "🎓", font=font_title)
draw.text((cx+30, y+6), "Claude Academy", fill=GOLD, font=font_title)
draw.text((cx+10, y+26), "Master Claude AI & MCP", fill=DIM, font=font_small)
bx = cx + 10
by = y + 42
bx += draw_badge(bx, by, "6 Modules", ORANGE)
bx += draw_badge(bx, by, "30 Quizzes", ORANGE)
bx += draw_badge(bx, by, "🔥 Popular", MAGENTA)

# MCP Academy
y2 = y + 78
draw_card(cx, y2, cw, 55, border_color=(0, 80, 100))
draw.text((cx+10, y2+6), "🔌", font=font_title)
draw.text((cx+30, y2+6), "MCP Academy", fill=CYAN, font=font_title)
draw.text((cx+10, y2+26), "Master the Protocol", fill=DIM, font=font_small)
bx = cx + 10
by = y2 + 38
bx += draw_badge(bx, by, "New: 2025", CYAN)
bx += draw_badge(bx, by, "7 Modules", GREEN)

# MCPlex Gateway
y3 = y2 + 65
draw_card(cx, y3, cw, 68, border_color=(0, 60, 80))
draw.text((cx+10, y3+6), "🚀", font=font_title)
draw.text((cx+30, y3+6), "MCPlex Gateway", fill=CYAN, font=font_title)
draw.text((cx+10, y3+26), "Open Source MCP Smart Gateway", fill=DIM, font=font_small)
bx = cx + 10
by = y3 + 42
bx += draw_badge(bx, by, "97% Token Savings", CYAN)
bx += draw_badge(bx, by, "MIT", GREEN)
bx += draw_badge(bx, by, "Rust", ORANGE)

# --- Footer bar ---
footer_y = H - 24
draw.line([0, footer_y, W, footer_y], fill=(CYAN[0]//8, CYAN[1]//8, CYAN[2]//8), width=1)
draw.rectangle([0, footer_y, W, H], fill=(5, 5, 14))

footer_text = "360K+ LOC  •  RUST  •  CUDA  •  TYPESCRIPT  •  BUILT FROM SCRATCH"
fw = draw.textlength(footer_text, font=font_footer)
draw.text(((W - fw) / 2 - 80, footer_y + 5), footer_text, fill=(80, 85, 100), font=font_footer)

# Available for work badge
aw_text = "● AVAILABLE FOR WORK →"
aw_w = draw.textlength(aw_text, font=font_footer)
draw.text((W - aw_w - 20, footer_y + 5), aw_text, fill=GREEN, font=font_footer)

# Save
img.save(OUT, "PNG")
print(f"✅ Banner saved: {img.width}x{img.height} → {OUT}")
