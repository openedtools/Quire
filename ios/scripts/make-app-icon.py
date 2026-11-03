#!/usr/bin/env python3
"""The iPhone app icon from the brand's tile icon (brand/source/app-icon.webp).

iOS wants an opaque 1024 square and rounds the corners itself, so the tile's transparent
corners are filled with its own background colour, and a thin margin is cut so the tile's
rounded edge falls outside iOS's mask. Run from the repository root after the brand icon changes.
"""
from PIL import Image

source = Image.open("brand/source/app-icon.webp").convert("RGBA")
size = source.size[0]
background = source.getpixel((size // 2, 150))[:3]
square = Image.new("RGBA", source.size, background + (255,))
square.alpha_composite(source)
margin = int(size * 0.035)
icon = square.crop((margin, margin, size - margin, size - margin)).convert("RGB").resize((1024, 1024), Image.LANCZOS)
icon.save("ios/Quire/Assets.xcassets/AppIcon.appiconset/AppIcon.png", optimize=True)
