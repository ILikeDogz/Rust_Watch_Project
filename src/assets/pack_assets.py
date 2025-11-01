#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys
import zlib

# python3 /home/ilikedogz/rust_test/rust_test_area/esp32s3_tests/src/assets/pack_assets.py -r -o

# Match names like: alien1_240x240_rgb565_be.raw
RAW_RE = re.compile(r'_(\d+)x(\d+)_rgb565_be\.raw$', re.IGNORECASE)

def size_from_name(p: pathlib.Path):
    m = RAW_RE.search(p.name)
    if not m:
        return None
    return int(m.group(1)), int(m.group(2))

def compress_one(path: pathlib.Path, level: int, force: bool, overwrite: bool) -> bool:
    wh = size_from_name(path)
    if wh is None:
        print(f"skip: {path.name} (name must end with _<W>x<H>_rgb565_be.raw)")
        return False

    w, h = wh
    expected = w * h * 2
    data = path.read_bytes()
    if len(data) != expected and not force:
        print(f"ERROR: {path.name}: size {len(data)} != {expected} (W={w}, H={h}). Use --force to override.")
        return False

    out = path.with_suffix(path.suffix + ".zlib")
    if out.exists() and not overwrite:
        print(f"skip: {out.name} already exists (use --overwrite to replace)")
        return True

    comp = zlib.compress(data, level=level)
    out.write_bytes(comp)
    ratio = (len(comp) / len(data)) if len(data) else 1.0
    print(f"ok: {path.name} -> {out.name}  {len(data)} -> {len(comp)} bytes ({ratio:.2%})")
    return True

def main():
    ap = argparse.ArgumentParser(description="Zlib-compress RGB565 BE .raw files in this folder to .raw.zlib")
    ap.add_argument("-l", "--level", type=int, default=9, help="compression level 0..9 (default 9)")
    ap.add_argument("-f", "--force", action="store_true", help="ignore size check (W*H*2) derived from filename")
    ap.add_argument("-o", "--overwrite", action="store_true", help="overwrite existing .zlib files")
    ap.add_argument("-r", "--recursive", action="store_true", help="recurse into subdirectories")
    args = ap.parse_args()

    if args.level < 0 or args.level > 9:
        print("compression level must be 0..9")
        sys.exit(2)

    base = pathlib.Path(__file__).parent.resolve()
    files = sorted((base.rglob if args.recursive else base.glob)("*.raw"))

    if not files:
        print("no *.raw files found in this folder.")
        sys.exit(1)

    ok = 0
    for f in files:
        try:
            if compress_one(f, args.level, args.force, args.overwrite):
                ok += 1
        except Exception as e:
            print(f"fail: {f.name}: {e}")

    print(f"done: {ok}/{len(files)} files processed.")
    sys.exit(0 if ok == len(files) else 1)

if __name__ == "__main__":
    main()