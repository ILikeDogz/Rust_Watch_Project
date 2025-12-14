# image_converter.py
# quick and dirty script to convert images to RGB565 .raw files
# Convert an image to BIG-ENDIAN RGB565 .raw for embedded-graphics ImageRaw::<Rgb565>
# No flags, just click. Requires: pip install pillow

import os
import struct
from PIL import Image
import tkinter as tk
from tkinter import filedialog, simpledialog, messagebox

def to_rgb565_be_bytes(img: Image.Image) -> bytes:
    """Convert an RGB image (exact size) to BIG-endian RGB565 raw bytes."""
    if img.mode != "RGB":
        img = img.convert("RGB")
    w, h = img.size
    px = img.load()
    out = bytearray(w * h * 2)
    i = 0
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            v = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
            out[i:i+2] = struct.pack(">H", v)  # BIG-endian
            i += 2
    return bytes(out)

def main():
    root = tk.Tk()
    root.withdraw()

    path = filedialog.askopenfilename(
        title="Select image",
        filetypes=[("Images", "*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.tga;*.tif;*.tiff"),
                   ("All files", "*.*")],
    )
    if not path:
        return

    try:
        img = Image.open(path)
    except Exception as e:
        messagebox.showerror("Error", f"Failed to open image:\n{e}")
        return

    iw, ih = img.size
    w = simpledialog.askinteger("Width", "Output width (pixels):", initialvalue=iw, minvalue=1, maxvalue=4096)
    if w is None:
        return
    h = simpledialog.askinteger("Height", "Output height (pixels):", initialvalue=ih, minvalue=1, maxvalue=4096)
    if h is None:
        return

    # Stretch to exactly (w, h) (simple & predictable)
    img2 = img.convert("RGB").resize((w, h), Image.LANCZOS)

    raw_bytes = to_rgb565_be_bytes(img2)
    base, _ = os.path.splitext(path)
    out_path = f"{base}_{w}x{h}_rgb565_be.raw"

    try:
        with open(out_path, "wb") as f:
            f.write(raw_bytes)
    except Exception as e:
        messagebox.showerror("Error", f"Failed to save file:\n{e}")
        return

    messagebox.showinfo("Done", f"Saved:\n{out_path}\nBytes: {len(raw_bytes)} (expected {w*h*2})")

if __name__ == "__main__":
    main()
