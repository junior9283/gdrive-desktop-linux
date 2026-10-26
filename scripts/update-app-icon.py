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
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
SOURCE = REPO / "assets/icons/app-icon-source.png"
APP_ID = "io.github.junior9283.GDriveDesktopLinux"
SIZES = [16, 22, 24, 32, 48, 64, 128, 256, 512]
# Margem em volta do desenho, em fração do lado (como os ícones do Breeze).
MARGIN = 0.06


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


def main() -> None:
    icon = square(clean(Image.open(SOURCE)))
    for size in SIZES:
        paths = [REPO / f"assets/icons/hicolor/{size}x{size}/apps/{APP_ID}.png"]
        if size == 256:
            paths += [REPO / "crates/gdl-ui/icons/icon.png", REPO / "crates/gdl-ui/ui/icons/app.png"]
        save(icon.resize((size, size), Image.Resampling.LANCZOS), *paths)


if __name__ == "__main__":
    main()
