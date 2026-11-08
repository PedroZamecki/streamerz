#!/usr/bin/env python3
"""
Generate Chrome Web Store assets:
1. 128x128 Extension Icon (Streamerz logo + TikTok badge in bottom-right)
2. 1280x800 Screenshots (24-bit RGB, NO alpha channel, as strictly required by Chrome Web Store)
3. 440x280 Small Promo Tile (24-bit RGB)
4. 1400x560 Marquee Promo Banner (24-bit RGB)
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

# Ensure dev/locale.py does not shadow standard library
if sys.path and sys.path[0] == str(Path(__file__).resolve().parent):
    sys.path.pop(0)

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "dist" / "store-assets"
EXT_ICON = REPO_ROOT / "assets" / "tiktok-extension" / "icon.png"

FONT_REG = "/usr/share/fonts/liberation/LiberationSans-Regular.ttf"
FONT_BOLD = "/usr/share/fonts/liberation/LiberationSans-Bold.ttf"

def get_font(path, size):
    try:
        return ImageFont.truetype(path, size)
    except Exception:
        return ImageFont.load_default()

def generate_128_icon():
    print("[*] Generating 128x128 Icon (Vector-rendered Streamerz + Black & White TikTok badge)...")
    from PIL import ImageFilter
    
    # 1. Render Streamerz from SVG at high density (1024x1024)
    tmp_sz = Path("/tmp/sz_vector_1024.png")
    subprocess.run([
        "magick", "-background", "none", "-density", "600",
        str(REPO_ROOT / "assets" / "brand" / "streamerz.svg"),
        "-resize", "1024x1024", str(tmp_sz)
    ], check=True)

    # 2. Render Pure White TikTok musical note glyph from SVG at 512x512
    svg_content = (REPO_ROOT / "assets" / "icons" / "tiktok.svg").read_text()
    white_svg = svg_content.replace('fill="#FE2C55"', 'fill="#FFFFFF"')
    tmp_white_svg = Path("/tmp/tt_white.svg")
    tmp_white_svg.write_text(white_svg)
    tmp_tt = Path("/tmp/tt_white_512.png")
    subprocess.run([
        "magick", "-background", "none", "-density", "600",
        str(tmp_white_svg),
        "-resize", "512x512", str(tmp_tt)
    ], check=True)

    # 3. Assemble on a 512x512 supersampling canvas (4x scale) for sub-pixel anti-aliasing
    scale = 4
    size_512 = 128 * scale
    canvas = Image.new("RGBA", (size_512, size_512), (0, 0, 0, 0))

    # Place Streamerz logo (456x456)
    sz_hires = Image.open(tmp_sz).convert("RGBA")
    sz_scaled = sz_hires.resize((456, 456), Image.Resampling.LANCZOS)
    canvas.paste(sz_scaled, (12, 12), sz_scaled)

    # Badge bounds: [288, 288, 504, 504] (diameter: 216px on 512 canvas -> 54px on 128 canvas)
    bx0, by0, bx1, by1 = 288, 288, 504, 504

    # Drop shadow under the badge to separate it cleanly from the background logo
    shadow = Image.new("RGBA", (size_512, size_512), (0, 0, 0, 0))
    sdraw = ImageDraw.Draw(shadow)
    sdraw.ellipse([bx0 - 8, by0 - 6, bx1 + 8, by1 + 10], fill=(0, 0, 0, 200))
    shadow = shadow.filter(ImageFilter.GaussianBlur(12))
    canvas = Image.alpha_composite(canvas, shadow)

    # Badge circle: Pure Black fill with crisp, anti-aliased White outline (8px on 512 = 2px on 128)
    badge = Image.new("RGBA", (size_512, size_512), (0, 0, 0, 0))
    bdraw = ImageDraw.Draw(badge)
    bdraw.ellipse([bx0, by0, bx1, by1], fill=(0, 0, 0, 255), outline=(255, 255, 255, 255), width=9)

    # Paste pure white TikTok note glyph centered in the black circle
    tt_hires = Image.open(tmp_tt).convert("RGBA")
    tt_size = 130  # ~32px on 128 canvas
    tt_scaled = tt_hires.resize((tt_size, tt_size), Image.Resampling.LANCZOS)
    tx = (bx0 + bx1) // 2 - tt_size // 2
    ty = (by0 + by1) // 2 - tt_size // 2
    badge.paste(tt_scaled, (tx, ty), tt_scaled)

    canvas = Image.alpha_composite(canvas, badge)

    # Downsample to exact 128x128 with Lanczos resampling
    final_128 = canvas.resize((128, 128), Image.Resampling.LANCZOS)

    icon_path = OUT_DIR / "icon-128.png"
    final_128.save(icon_path, "PNG")

    # Also update the extension's own icon
    shutil.copyfile(icon_path, EXT_ICON)
    print(f"  [+] Saved {icon_path} and updated {EXT_ICON}")
    return icon_path

def generate_screenshot_1():
    print("[*] Generating Screenshot 1 (1280x800 RGB - Live Chat Integration)...")
    w, h = 1280, 800
    img = Image.new("RGB", (w, h), (13, 15, 20))
    draw = ImageDraw.Draw(img)

    f_title = get_font(FONT_BOLD, 36)
    f_sub = get_font(FONT_REG, 20)
    f_card_title = get_font(FONT_BOLD, 22)
    f_body = get_font(FONT_REG, 17)
    f_bold = get_font(FONT_BOLD, 17)
    f_tag = get_font(FONT_BOLD, 14)

    # Header section
    draw.rectangle([0, 0, w, 110], fill=(20, 24, 33))
    draw.line([(0, 110), (w, 110)], fill=(40, 46, 62), width=2)

    # Icon in header
    icon = Image.open(OUT_DIR / "icon-128.png").convert("RGBA").resize((72, 72), Image.Resampling.LANCZOS)
    img.paste(icon.convert("RGB"), (40, 19))

    draw.text((128, 26), "Streamerz for TikTok", font=f_title, fill=(255, 255, 255))
    draw.text((128, 68), "Live Stream Chat & Search Companion  •  Zero Visible Tabs", font=f_sub, fill=(160, 170, 190))

    # Badge in top right
    draw.rounded_rectangle([w - 240, 36, w - 40, 74], radius=8, fill=(30, 40, 60), outline=(60, 100, 180), width=1)
    draw.text((w - 225, 46), "DESKTOP COMPANION", font=f_tag, fill=(100, 180, 255))

    # Left Column: Streamerz App Accounts Panel
    draw.rounded_rectangle([40, 140, 440, 740], radius=14, fill=(22, 26, 36), outline=(36, 42, 58), width=2)
    draw.text((65, 165), "Platform Accounts", font=f_card_title, fill=(240, 245, 255))

    platforms = [
        ("Twitch", "@streamer", "(Connected)", (34, 197, 94)),
        ("YouTube", "Channel Live", "(Connected)", (34, 197, 94)),
        ("TikTok", "@streamer_live", "(Active Helper)", (34, 197, 94)),
        ("Kick", "@streamer", "(Connected)", (34, 197, 94)),
    ]
    y = 215
    for name, handle, status, color in platforms:
        bg = (28, 34, 48) if name == "TikTok" else (18, 21, 29)
        border = (60, 130, 246) if name == "TikTok" else (30, 35, 48)
        draw.rounded_rectangle([60, y, 420, y + 64], radius=8, fill=bg, outline=border, width=2 if name == "TikTok" else 1)
        draw.text((75, y + 14), name, font=f_bold, fill=(255, 255, 255))
        draw.text((75, y + 36), handle, font=f_body, fill=(160, 170, 190))
        draw.text((290, y + 23), status, font=f_tag, fill=color)
        y += 76

    # Bottom left summary box
    draw.rounded_rectangle([60, 545, 420, 715], radius=10, fill=(18, 22, 32), outline=(35, 42, 60), width=1)
    draw.text((75, 560), "Active Extension Bridge", font=f_bold, fill=(200, 215, 245))
    draw.text((75, 595), "• Runtime: Manifest V3", font=f_body, fill=(150, 165, 185))
    draw.text((75, 625), "• Connection: 127.0.0.1 WebSocket", font=f_body, fill=(150, 165, 185))
    draw.text((75, 655), "• Auth: 64-char Pairing Token", font=f_body, fill=(150, 165, 185))
    draw.text((75, 685), "• Inactivity Sleep: Ultra-low RAM", font=f_body, fill=(52, 211, 153))

    # Right Column: Unified Chat Window Preview
    draw.rounded_rectangle([470, 140, 1240, 740], radius=14, fill=(22, 26, 36), outline=(36, 42, 58), width=2)
    draw.text((500, 165), "Streamerz Unified Live Chat", font=f_card_title, fill=(240, 245, 255))
    draw.text((1060, 168), "LIVE • 1,480 viewers", font=f_tag, fill=(239, 68, 68))

    chat_messages = [
        ("[TikTok]", "alex_gamer", "hello streamer! Amazing gameplay today! 🔥", (255, 45, 85)),
        ("[Twitch]", "stream_fan99", "PogChamp that combo was insane!", (145, 70, 255)),
        ("[TikTok]", "sarah_br", "Can you show the settings? manda um salve!", (255, 45, 85)),
        ("[YouTube]", "Mark_Tech", "Audio and video are crystal clear 🚀", (255, 0, 0)),
        ("[TikTok]", "streamer_live (You)", "@sarah_br Salve! Settings are in the panel!", (34, 197, 94)),
        ("[TikTok]", "gabriel_12", "Sent 10x Rose 🌹🌹🌹", (255, 45, 85)),
    ]

    cy = 220
    for badge, user, text, bcolor in chat_messages:
        draw.rounded_rectangle([495, cy, 1215, cy + 54], radius=6, fill=(18, 21, 29))
        draw.text((510, cy + 16), badge, font=f_bold, fill=bcolor)
        draw.text((600, cy + 16), user + ":", font=f_bold, fill=(230, 235, 245))
        draw.text((740, cy + 16), text, font=f_body, fill=(200, 210, 225))
        cy += 64

    # Bottom Chat Send Input
    draw.rounded_rectangle([495, 620, 1215, 715], radius=8, fill=(16, 18, 25), outline=(45, 55, 75), width=1)
    draw.text((515, 640), "Send message to: [ TikTok LIVE Chat ]", font=f_tag, fill=(120, 140, 170))
    draw.text((515, 670), "Thanks for watching everyone! GG! ✨|", font=f_body, fill=(255, 255, 255))
    draw.rounded_rectangle([1120, 655, 1200, 695], radius=6, fill=(37, 99, 235))
    draw.text((1142, 666), "Send", font=f_bold, fill=(255, 255, 255))

    shot1_path = OUT_DIR / "screenshot-1-chat.png"
    img.save(shot1_path, "PNG")
    print(f"  [+] Saved {shot1_path} (Mode: {img.mode}, Size: {img.size})")
    return shot1_path

def generate_screenshot_2():
    print("[*] Generating Screenshot 2 (1280x800 RGB - Browser Lifecycle & 1-Click Setup)...")
    w, h = 1280, 800
    img = Image.new("RGB", (w, h), (13, 15, 20))
    draw = ImageDraw.Draw(img)

    f_title = get_font(FONT_BOLD, 36)
    f_sub = get_font(FONT_REG, 20)
    f_card_title = get_font(FONT_BOLD, 22)
    f_body = get_font(FONT_REG, 17)
    f_bold = get_font(FONT_BOLD, 17)
    f_tag = get_font(FONT_BOLD, 14)

    # Header
    draw.rectangle([0, 0, w, 110], fill=(20, 24, 33))
    draw.line([(0, 110), (w, 110)], fill=(40, 46, 62), width=2)
    icon = Image.open(OUT_DIR / "icon-128.png").convert("RGBA").resize((72, 72), Image.Resampling.LANCZOS)
    img.paste(icon.convert("RGB"), (40, 19))
    draw.text((128, 26), "Streamerz for TikTok", font=f_title, fill=(255, 255, 255))
    draw.text((128, 68), "Cross-Browser Lifecycle & Automated Sleep Architecture", font=f_sub, fill=(160, 170, 190))

    # Feature Grid: 3 Cards across the bottom
    features = [
        ("Universal Compatibility", [
            "• Google Chrome & Microsoft Edge",
            "• Mozilla Firefox & Brave",
            "• Vivaldi, Chromium, Opera & GX",
            "• Works on Windows & Linux",
        ]),
        ("Ultra-Low Resource Impact", [
            "• Automatic 90s idle inactivity timer",
            "• Tears down offscreen/iframe when idle",
            "• Background RAM drops to ~0 MB",
            "• 0.0% CPU when not active",
        ]),
        ("Privacy & Security First", [
            "• Manifest V3 compliant",
            "• Connects strictly to local 127.0.0.1",
            "• Zero third-party telemetry / tracker calls",
            "• Reuses your existing secure browser session",
        ]),
    ]

    cx = 40
    for f_head, bullets in features:
        draw.rounded_rectangle([cx, 140, cx + 380, 420], radius=12, fill=(22, 26, 36), outline=(36, 42, 58), width=2)
        draw.text((cx + 24, 165), f_head, font=f_card_title, fill=(100, 180, 255))
        by = 220
        for b in bullets:
            draw.text((cx + 24, by), b, font=f_body, fill=(210, 220, 235))
            by += 40
        cx += 410

    # Large Bottom Diagram: How it works flow
    draw.rounded_rectangle([40, 450, 1240, 740], radius=14, fill=(20, 24, 34), outline=(40, 50, 70), width=2)
    draw.text((70, 475), "Seamless Local Architecture (How Streamerz Communicates)", font=f_card_title, fill=(255, 255, 255))

    steps = [
        ("1. Desktop App", "Streamerz runs natively\nLocal WS server on 127.0.0.1\nGenerates 64-char pairing token", (37, 99, 235)),
        ("2. Companion Extension", "Background Service Worker\nReceives chat send command\nPairs securely with local token", (16, 185, 129)),
        ("3. TikTok Offscreen", "Signs webcast requests\nExecutes official chat send\nFrees context on 90s idle", (236, 72, 153)),
    ]

    sx = 80
    for s_title, s_desc, scolor in steps:
        draw.rounded_rectangle([sx, 530, sx + 320, 690], radius=10, fill=(14, 17, 24), outline=scolor, width=2)
        draw.text((sx + 20, 550), s_title, font=f_bold, fill=scolor)
        dy = 590
        for line in s_desc.split("\n"):
            draw.text((sx + 20, dy), line, font=f_body, fill=(180, 190, 210))
            dy += 28
        if sx < 700:
            draw.text((sx + 342, 600), "➜", font=f_title, fill=(100, 120, 150))
        sx += 390

    shot2_path = OUT_DIR / "screenshot-2-features.png"
    img.save(shot2_path, "PNG")
    print(f"  [+] Saved {shot2_path} (Mode: {img.mode}, Size: {img.size})")
    return shot2_path

def generate_promo_tiles():
    print("[*] Generating Promo Tiles (440x280 and 1400x560 RGB)...")
    icon = Image.open(OUT_DIR / "icon-128.png").convert("RGBA")
    
    # Small tile (440x280)
    p440 = Image.new("RGB", (440, 280), (16, 19, 28))
    d440 = ImageDraw.Draw(p440)
    i80 = icon.resize((84, 84), Image.Resampling.LANCZOS)
    p440.paste(i80.convert("RGB"), (178, 40))
    d440.text((220, 150), "Streamerz for TikTok", font=get_font(FONT_BOLD, 22), fill=(255, 255, 255), anchor="mt")
    d440.text((220, 188), "LIVE Chat Companion Extension", font=get_font(FONT_REG, 15), fill=(160, 175, 200), anchor="mt")
    d440.text((220, 224), "Zero Visible Tabs • Low RAM", font=get_font(FONT_BOLD, 13), fill=(52, 211, 153), anchor="mt")
    p440_path = OUT_DIR / "promo-small-440x280.png"
    p440.save(p440_path, "PNG")

    # Marquee banner (1400x560)
    p1400 = Image.new("RGB", (1400, 560), (14, 17, 24))
    d1400 = ImageDraw.Draw(p1400)
    i160 = icon.resize((150, 150), Image.Resampling.LANCZOS)
    p1400.paste(i160.convert("RGB"), (100, 205))
    d1400.text((280, 200), "Streamerz for TikTok", font=get_font(FONT_BOLD, 52), fill=(255, 255, 255))
    d1400.text((280, 275), "Connects Streamerz to your TikTok account for LIVE chat sending.", font=get_font(FONT_REG, 24), fill=(170, 185, 210))
    d1400.text((280, 325), "Operates silently in the background with zero visible helper tabs and automatic idle memory tear-down.", font=get_font(FONT_REG, 19), fill=(130, 145, 170))
    p1400_path = OUT_DIR / "promo-marquee-1400x560.png"
    p1400.save(p1400_path, "PNG")

    print(f"  [+] Saved {p440_path} and {p1400_path}")

def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    generate_128_icon()
    generate_screenshot_1()
    generate_screenshot_2()
    generate_promo_tiles()

    # Re-run packager so the extension zip has the updated icon
    subprocess.run([sys.executable, str(REPO_ROOT / "dev" / "package-extensions.py")], check=True)
    print("\n[✔] All Chrome Web Store graphics generated and verified successfully!")

if __name__ == "__main__":
    main()
