#!/usr/bin/env python3
"""Gera os ícones do app a partir da arte original (`assets/icons/app-icon-source.png`).

    python3 scripts/update-app-icon.py

Limpa o recorte do fundo (pontos soltos em volta e a borda quase opaca do desenho), centraliza
num quadrado com margem e grava:

- o tema do sistema: `assets/icons/hicolor/<N>x<N>/apps/<id>.png` (menu de aplicativos);
- o ícone da janela (Tauri): `crates/gdl-ui/icons/icon.png`;
- o logo da tela Sobre: `crates/gdl-ui/ui/icons/app.png`.

Os dois últimos repetem o de 256 px do tema: a janela e a página só leem arquivos das suas pastas.

Precisa de Pillow e NumPy (só para regerar; o app não depende deles).
"""

from collections import deque
import base64
import io
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
SOURCE = REPO / "assets/icons/app-icon-source.png"
SIMBOLIC_SOURCE = REPO / "assets/icons/app-icon-simbolic.png"
APP_ID = "io.github.junior9283.GDriveDesktopLinux"
SIZES = [16, 22, 24, 32, 48, 64, 128, 256, 512]
# Margem em volta do desenho, em fração do lado (como os ícones do Breeze).
MARGIN = 0.06

INDEX_THEME = """[Icon Theme]
Name=Hicolor
Comment=Hicolor default theme
Hidden=true
Directories=scalable/status,scalable/emblems,16x16/status,22x22/status,24x24/status,32x32/status,48x48/status,16x16/apps,22x22/apps,24x24/apps,32x32/apps,48x48/apps,64x64/apps,128x128/apps,256x256/apps,512x512/apps

[scalable/status]
Size=24
Type=Scalable
MinSize=16
MaxSize=256

[scalable/emblems]
Size=24
Type=Scalable
MinSize=16
MaxSize=256

[16x16/status]
Size=16
Type=Fixed

[22x22/status]
Size=22
Type=Fixed

[24x24/status]
Size=24
Type=Fixed

[32x32/status]
Size=32
Type=Fixed

[48x48/status]
Size=48
Type=Fixed

[16x16/apps]
Size=16
Type=Fixed

[22x22/apps]
Size=22
Type=Fixed

[24x24/apps]
Size=24
Type=Fixed

[32x32/apps]
Size=32
Type=Fixed

[48x48/apps]
Size=48
Type=Fixed

[64x64/apps]
Size=64
Type=Fixed

[128x128/apps]
Size=128
Type=Fixed

[256x256/apps]
Size=256
Type=Fixed

[512x512/apps]
Size=512
Type=Fixed
"""


def largest_shape(alpha: np.ndarray) -> np.ndarray:
    """Máscara do maior conjunto ligado de pixels visíveis: o desenho, sem os pontos soltos."""
    visible = alpha > 24
    seen = np.zeros_like(visible)
    best: list[tuple[int, int]] = []
    height, width = visible.shape
    for start in zip(*np.nonzero(visible)):
        if seen[start]:
            continue
        seen[start] = True
        shape, queue = [start], deque([start])
        while queue:
            y, x = queue.popleft()
            for ny, nx in ((y + 1, x), (y - 1, x), (y, x + 1), (y, x - 1)):
                if 0 <= ny < height and 0 <= nx < width and visible[ny, nx] and not seen[ny, nx]:
                    seen[ny, nx] = True
                    shape.append((ny, nx))
                    queue.append((ny, nx))
        if len(shape) > len(best):
            best = shape
    mask = np.zeros_like(visible)
    ys, xs = zip(*best)
    mask[list(ys), list(xs)] = True
    return mask


def clean(image: Image.Image) -> Image.Image:
    rgba = np.array(image.convert("RGBA"))
    alpha = rgba[..., 3]
    # O recorte deixou o desenho com alfa ~250: fica opaco; a borda suave continua suave.
    rgba[..., 3] = np.where(largest_shape(alpha), np.where(alpha >= 240, 255, alpha), 0)
    return Image.fromarray(rgba)


def square(image: Image.Image) -> Image.Image:
    left, top, right, bottom = image.getbbox()
    shape = image.crop((left, top, right, bottom))
    side = round(max(shape.size) / (1 - 2 * MARGIN))
    canvas = Image.new("RGBA", (side, side))
    canvas.paste(shape, ((side - shape.width) // 2, (side - shape.height) // 2))
    return canvas


def save(icon: Image.Image, *paths: Path) -> None:
    for path in paths:
        path.parent.mkdir(parents=True, exist_ok=True)
        icon.save(path, optimize=True)
        print(f"gera {path.relative_to(REPO)}")


def update_status_icons() -> None:
    if not SIMBOLIC_SOURCE.is_file():
        return
    simbolic = Image.open(SIMBOLIC_SOURCE)
    bbox = simbolic.getbbox()
    shape = simbolic.crop(bbox)
    side = round(max(shape.size) / (1 - 2 * 0.08))
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    canvas.paste(shape, ((side - shape.width) // 2, (side - shape.height) // 2))

    # Base de 256 px embutida em base64 nos SVGs
    c256 = canvas.resize((256, 256), Image.Resampling.LANCZOS)
    buf = io.BytesIO()
    c256.save(buf, format="PNG")
    b64 = base64.b64encode(buf.getvalue()).decode()

    svgs = {
        "gdl-synced": f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <image width="24" height="24" href="data:image/png;base64,{b64}"/>
</svg>''',
        "gdl-paused": f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <image width="24" height="24" href="data:image/png;base64,{b64}"/>
  <circle cx="17" cy="17" r="5.5" fill="#2b2b2b"/>
  <path d="M15.5 14.5v5M18.5 14.5v5" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/>
</svg>''',
        "gdl-error": f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <image width="24" height="24" href="data:image/png;base64,{b64}"/>
  <circle cx="17" cy="17" r="5.5" fill="#d9473f"/>
  <path d="M17 14.2v3.2" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/>
  <circle cx="17" cy="19.2" r="0.8" fill="#ffffff"/>
</svg>''',
        "gdl-offline": f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <g opacity="0.45">
    <image width="24" height="24" href="data:image/png;base64,{b64}"/>
  </g>
  <path d="M4 4l16 16" stroke="#9e9e9e" stroke-width="1.8" stroke-linecap="round"/>
</svg>''',
        "gdl-signed-out": f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <g opacity="0.35">
    <image width="24" height="24" href="data:image/png;base64,{b64}"/>
  </g>
</svg>''',
    }

    angles = [0, 45, 90, 135]
    for i, a in enumerate(angles, 1):
        svgs[f"gdl-syncing-{i}"] = f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="24" height="24" viewBox="0 0 24 24">
  <image width="24" height="24" href="data:image/png;base64,{b64}"/>
  <circle cx="17" cy="17" r="5.5" fill="#2b2b2b"/>
  <g transform="rotate({a} 17 17)">
    <path d="M15 16.5a2.2 2.2 0 0 1 3.5-1.5" stroke="#ffffff" stroke-width="1.4" stroke-linecap="round" fill="none"/>
    <path d="M19 17.5a2.2 2.2 0 0 1-3.5 1.5" stroke="#ffffff" stroke-width="1.4" stroke-linecap="round" fill="none"/>
    <path d="M18.8 13.8l-.2 1.4-1.4-.2M15.2 20.2l.2-1.4 1.4.2" stroke="#ffffff" stroke-width="1.4" stroke-linecap="round" fill="none"/>
  </g>
</svg>'''

    status_dir = REPO / "assets/icons/hicolor/scalable/status"
    status_dir.mkdir(parents=True, exist_ok=True)
    for name, content in svgs.items():
        svg_file = status_dir / f"{name}.svg"
        svg_file.write_text(content)
        print(f"gera {svg_file.relative_to(REPO)}")

    import gi, cairo
    gi.require_version('Rsvg', '2.0')
    from gi.repository import Rsvg
    for name, content in svgs.items():
        handle = Rsvg.Handle.new_from_data(content.encode())
        for size in [16, 22, 24, 32, 48]:
            png_path = REPO / f"assets/icons/hicolor/{size}x{size}/status/{name}.png"
            png_path.parent.mkdir(parents=True, exist_ok=True)
            surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, size, size)
            cr = cairo.Context(surf)
            cr.scale(size / 24.0, size / 24.0)
            handle.render_cairo(cr)
            surf.write_to_png(str(png_path))
            print(f"gera {png_path.relative_to(REPO)}")

    index_file = REPO / "assets/icons/hicolor/index.theme"
    index_file.write_text(INDEX_THEME)
    print(f"gera {index_file.relative_to(REPO)}")


def main() -> None:
    icon = square(clean(Image.open(SOURCE)))
    for size in SIZES:
        paths = [REPO / f"assets/icons/hicolor/{size}x{size}/apps/{APP_ID}.png"]
        if size == 256:
            paths += [REPO / "crates/gdl-ui/icons/icon.png", REPO / "crates/gdl-ui/ui/icons/app.png"]
        save(icon.resize((size, size), Image.Resampling.LANCZOS), *paths)
    update_status_icons()


if __name__ == "__main__":
    main()
