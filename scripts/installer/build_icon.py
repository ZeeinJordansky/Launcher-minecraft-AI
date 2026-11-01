#!/usr/bin/env python3
"""Generate a multi-resolution ICO for the Windows installer."""
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parents[2]
source = root / "minty-launcher" / "assets" / "minty-launcher-icon.png"
target = root / "scripts" / "installer" / "Minty.ico"
target.parent.mkdir(parents=True, exist_ok=True)
with Image.open(source) as image:
    image.convert("RGBA").save(target, format="ICO", sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
print(f"Created {target}")
