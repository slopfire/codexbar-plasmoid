#!/usr/bin/env python3
"""Regenerate the app's provider icons from the plasmoid's SVG set.

The plasmoid ships one `ProviderIcon-<id>.svg` per provider. Android cannot render
SVG, and Glance (used by the home screen widgets) cannot decode it either, so the
icons are rasterized once into Android drawables and tinted at render time.

They are rasterized as **white silhouettes** on purpose:

- most source icons are `fill="white"` or `fill="currentColor"`, which would be
  invisible on a light surface;
- the app and the widgets tint them with the provider's accent colour, which is
  the provider's identity colour in this project and adapts to the theme.

Run from the repository root:

    python3 android/scripts/generate-provider-icons.py

Requires `inkscape` and ImageMagick (`magick`). Re-run it when the plasmoid gains
or changes a provider icon, then rebuild.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "plasmoid" / "contents" / "images"
TARGET = REPO / "android" / "app" / "src" / "main" / "res" / "drawable-xxhdpi"

# 144px lands at 48dp on xxhdpi: comfortably above the 24-48dp the app and the
# widgets draw at, so it stays crisp without shipping oversized bitmaps.
EXPORT_WIDTH = 144

# Below this mean alpha the export is effectively empty and is treated as a
# failure rather than shipped as an invisible icon.
MIN_MEAN_ALPHA = 0.005


def normalize(svg_text: str) -> str:
    """Make an upstream icon rasterizable by Inkscape.

    Two upstream quirks produce empty exports:

    - `fill="currentColor"` has no colour context, so nothing is drawn;
    - `width="1em" height="1em"` has no resolvable pixel size.
    """
    text = svg_text.replace("currentColor", "#ffffff")
    match = re.search(r"<svg\b[^>]*>", text)
    if not match:
        return text
    cleaned = re.sub(r'\s+(width|height)="[^"]*"', "", match.group(0))
    return text[: match.start()] + cleaned[:-1] + ' width="100" height="100">' + text[match.end() :]


def resource_name(provider_id: str) -> str:
    """Android resource names are lowercase letters, digits and underscores."""
    return "provider_" + re.sub(r"[^a-z0-9]", "", provider_id.lower())


def mean_alpha(png: Path) -> float:
    out = subprocess.run(
        ["magick", "identify", "-format", "%[fx:mean.a]", str(png)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return float(out)


def main() -> int:
    for tool in ("inkscape", "magick"):
        if shutil.which(tool) is None:
            print(f"error: {tool} is required on PATH", file=sys.stderr)
            return 2

    sources = sorted(SOURCE.glob("ProviderIcon-*.svg"))
    if not sources:
        print(f"error: no icons found in {SOURCE}", file=sys.stderr)
        return 2

    TARGET.mkdir(parents=True, exist_ok=True)
    for stale in TARGET.glob("provider_*.png"):
        stale.unlink()

    failures: list[str] = []
    total_bytes = 0
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        for svg in sources:
            provider_id = svg.stem.replace("ProviderIcon-", "")
            name = resource_name(provider_id)
            patched = tmp / f"{name}.svg"
            patched.write_text(normalize(svg.read_text()))
            raw = tmp / f"{name}.png"
            white = tmp / f"{name}-white.png"
            try:
                subprocess.run(
                    [
                        "inkscape",
                        "--export-type=png",
                        f"--export-filename={raw}",
                        f"--export-width={EXPORT_WIDTH}",
                        "--export-background-opacity=0",
                        str(patched),
                    ],
                    check=True,
                    capture_output=True,
                    timeout=180,
                )
                subprocess.run(
                    ["magick", str(raw), "-channel", "RGB", "-fill", "white", "-colorize", "100", str(white)],
                    check=True,
                    capture_output=True,
                    timeout=120,
                )
                destination = TARGET / f"{name}.png"
                destination.write_bytes(white.read_bytes())
                alpha = mean_alpha(destination)
                if alpha <= MIN_MEAN_ALPHA:
                    failures.append(f"{provider_id}: blank (mean alpha {alpha:.4f})")
                    destination.unlink()
                else:
                    total_bytes += destination.stat().st_size
            except (subprocess.SubprocessError, OSError) as error:
                failures.append(f"{provider_id}: {error}")

    written = len(list(TARGET.glob("provider_*.png")))
    print(f"wrote {written}/{len(sources)} icons to {TARGET.relative_to(REPO)} ({total_bytes // 1024} KB)")
    if failures:
        print("failures:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
