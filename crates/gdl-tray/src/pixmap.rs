//! Carregamento e cache dos pixmaps ARGB32 para os ícones da bandeja.

use std::collections::HashMap;
use std::sync::LazyLock;

use image::GenericImageView;

/// Converte os bytes de um PNG para a estrutura `ksni::Icon` em formato ARGB32.
fn load_icon(png_bytes: &[u8]) -> ksni::Icon {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .expect("ícone de bandeja embutido deve ser PNG válido");
    let (width, height) = img.dimensions();
    let mut data = img.into_rgba8().into_vec();
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.rotate_right(1); // RGBA -> ARGB
    }
    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}

/// Retorna os pixmaps ARGB32 correspondentes ao nome do ícone (tamanhos 24x24 e 32x32).
pub fn pixmap_for(name: &str) -> Vec<ksni::Icon> {
    static PIXMAPS: LazyLock<HashMap<&'static str, Vec<ksni::Icon>>> = LazyLock::new(|| {
        let mut map = HashMap::new();
        macro_rules! insert_icon {
            ($name:literal) => {
                map.insert(
                    $name,
                    vec![
                        load_icon(include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/../../assets/icons/hicolor/24x24/status/",
                            $name,
                            ".png"
                        ))),
                        load_icon(include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/../../assets/icons/hicolor/32x32/status/",
                            $name,
                            ".png"
                        ))),
                    ],
                );
            };
        }
        insert_icon!("gdl-synced");
        insert_icon!("gdl-paused");
        insert_icon!("gdl-error");
        insert_icon!("gdl-offline");
        insert_icon!("gdl-signed-out");
        insert_icon!("gdl-syncing-1");
        insert_icon!("gdl-syncing-2");
        insert_icon!("gdl-syncing-3");
        insert_icon!("gdl-syncing-4");
        map
    });

    PIXMAPS.get(name).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{STILL_ICONS, SYNC_ICONS};

    #[test]
    fn every_status_icon_has_valid_pixmaps() {
        for name in STILL_ICONS.iter().chain(&SYNC_ICONS) {
            let icons = pixmap_for(name);
            assert!(!icons.is_empty(), "pixmap faltando para {name}");
            assert_eq!(
                icons.len(),
                2,
                "deve fornecer 2 tamanhos (24 e 32) para {name}"
            );
            for icon in &icons {
                assert!(icon.width == 24 || icon.width == 32);
                assert_eq!(icon.width, icon.height);
                assert_eq!(
                    icon.data.len(),
                    (icon.width * icon.height * 4) as usize,
                    "tamanho dos dados ARGB32 incorreto para {name}"
                );
            }
        }
    }
}
