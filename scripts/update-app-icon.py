#!/usr/bin/env python3
"""Gera os ícones do app a partir da arte original (`assets/icons/app-icon-source.png`).

    python3 scripts/update-app-icon.py
    python3 scripts/update-app-icon.py --flavor gerasync [--source assets/gerasync/app-icon.svg]

Limpa o recorte do fundo (pontos soltos em volta e a borda quase opaca do desenho), centraliza
num quadrado com margem e grava:

- o tema do sistema: `assets/icons/hicolor/<N>x<N>/apps/<id>.png` (menu de aplicativos);
- o ícone da janela (Tauri): `crates/gdl-ui/icons/icon.png`;
- o logo da tela Sobre: `crates/gdl-ui/ui/icons/app.png`.

Os dois últimos repetem o de 256 px do tema: a janela e a página só leem arquivos das suas pastas.

Com `--flavor gerasync`, os do GeraSync, a partir do SVG dele (desenhado pelo Inkscape): o tema em
`assets/gerasync/icons/`, a janela em `crates/gdl-ui/icons/gerasync.png`, o Sobre em
`crates/gdl-ui/ui/icons/app-gerasync.png` e o logotipo de 120 px do Console do Google em
`assets/gerasync/logo-120.png`.

Precisa de Pillow e NumPy (só para regerar; o app não depende deles) e, no GeraSync, do Inkscape.
"""

import argparse
import subprocess
import tempfile
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
SOURCE = REPO / "assets/icons/app-icon-source.png"
APP_ID = "io.github.junior9283.GDriveDesktopLinux"
GERASYNC_ID = "io.github.junior9283.GeraSync"
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


def render_svg(source: Path, size: int) -> Image.Image:
    """O SVG desenhado em `size`×`size` pelo Inkscape (cada tamanho direto do vetor)."""
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "icon.png"
        subprocess.run(
            ["inkscape", str(source), "--export-type=png", f"--export-filename={out}",
             f"--export-width={size}", f"--export-height={size}"],
            check=True, capture_output=True,
        )
        return Image.open(out).copy()


def emit(render, icons: str, app_id: str, window: str, about: str) -> None:
    """Grava `render(tamanho)` no tema (`<icons>/hicolor/...`) e o de 256 px também como ícone da
    janela (`crates/gdl-ui/icons/<window>`) e logo do Sobre (`crates/gdl-ui/ui/icons/<about>`)."""
    for size in SIZES:
        paths = [REPO / f"{icons}/hicolor/{size}x{size}/apps/{app_id}.png"]
        if size == 256:
            paths += [REPO / "crates/gdl-ui/icons" / window, REPO / "crates/gdl-ui/ui/icons" / about]
        save(render(size), *paths)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--flavor", choices=["default", "gerasync"], default="default")
    parser.add_argument("--source", type=Path, default=REPO / "assets/gerasync/app-icon.svg")
    args = parser.parse_args()
    if args.flavor == "gerasync":
        emit(lambda size: render_svg(args.source, size), "assets/gerasync/icons", GERASYNC_ID,
             "gerasync.png", "app-gerasync.png")
        save(render_svg(args.source, 120), REPO / "assets/gerasync/logo-120.png")
        return
    icon = square(clean(Image.open(SOURCE)))
    emit(lambda size: icon.resize((size, size), Image.Resampling.LANCZOS), "assets/icons", APP_ID,
         "icon.png", "app.png")


if __name__ == "__main__":
    main()
