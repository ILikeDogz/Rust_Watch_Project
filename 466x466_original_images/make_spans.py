#!/usr/bin/env python3
import sys, zlib, struct
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    print("Install Pillow: pip install pillow")
    sys.exit(1)

LIME_KEY = (0x8B, 0xE3, 0x08)  # #8BE308

def rgb888_to_rgb565_be(r, g, b):
    return struct.pack(">H", ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3))

def pick_key_color(_img):
    return rgb888_to_rgb565_be(*LIME_KEY), LIME_KEY

def choose_file():
    try:
        import tkinter as tk
        from tkinter import filedialog
        root = tk.Tk()
        root.withdraw()
        path = filedialog.askopenfilename(title="Select PNG image", filetypes=[("PNG","*.png")])
        return Path(path) if path else None
    except Exception:
        return None

def main():
    if len(sys.argv) > 1:
        targets = [Path(sys.argv[1])]
    else:
        pngs = list(Path(".").glob("*.png"))
        if pngs:
            targets = pngs
        else:
            sel = choose_file()
            if not sel:
                print("No image selected.")
                return
            targets = [sel]

    for src in targets:
        if not src.is_file():
            print("Skip (not found):", src)
            continue

        img = Image.open(src).convert("RGBA")
        w, h = img.size
        key565_be, key_rgb = pick_key_color(img)
        key565 = struct.unpack(">H", key565_be)[0]
        pixels = img.load()

        full_raw = bytearray()
        for y in range(h):
            for x in range(w):
                r,g,b,a = pixels[x,y]
                full_raw += rgb888_to_rgb565_be(r,g,b)
        full_z = zlib.compress(full_raw, 9)

        rle = bytearray(struct.pack(">HHH", w, h, key565))
        run_pixels_total = 0
        for y in range(h):
            row_runs = []
            x = 0
            while x < w:
                r,g,b,a = pixels[x,y]
                if (r,g,b) == key_rgb or a == 0:
                    x += 1
                    continue
                start = x
                while x < w:
                    r2,g2,b2,a2 = pixels[x,y]
                    if (r2,g2,b2) == key_rgb or a2 == 0:
                        break
                    x += 1
                row_runs.append((start, x - start))
            rle += struct.pack(">H", len(row_runs))
            for sx, ln in row_runs:
                rle += struct.pack(">HH", sx, ln)
                for px in range(sx, sx+ln):
                    r,g,b,a = pixels[px,y]
                    rle += rgb888_to_rgb565_be(r,g,b)
                run_pixels_total += ln

        base = src.with_suffix("")
        base_dir = base.parent
        full_out = base_dir / (base.name + "_full.rgb565.be.zlib")
        rle_out  = base_dir / (base.name + "_sprite.rle.bin")
        meta_out = base_dir / (base.name + "_sprite.meta.txt")

        full_out.write_bytes(full_z)
        rle_out.write_bytes(rle)
        meta_out.write_text(
            f"Source: {src.name}\nSize: {w}x{h}\nKey RGB: {key_rgb}\n"
            f"Key RGB565: 0x{key565:04X}\nFull raw: {len(full_raw)} bytes\n"
            f"Full zlib: {len(full_z)} bytes\nRLE size: {len(rle)} bytes\n"
            f"Opaque kept: {run_pixels_total}/{w*h} ({run_pixels_total/(w*h):.2%})\n"
        )
        print(f"[OK] {src.name} -> {rle_out.name}")

if __name__ == "__main__":
    main()