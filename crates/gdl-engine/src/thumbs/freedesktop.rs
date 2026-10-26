//! Formato do cache de miniaturas do freedesktop (`~/.cache/thumbnails/{normal,large,x-large}`).

use std::fs::File;
use std::io::BufReader;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_encode};

#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    #[error("imagem inválida: {0}")]
    Decode(String),
    #[error("falha ao gravar o PNG: {0}")]
    Encode(String),
    #[error("erro de arquivo local: {0}")]
    Io(#[from] std::io::Error),
}

/// Tamanhos gravados. `xx-large` não é gravado de propósito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbSize {
    Normal,
    Large,
    XLarge,
}

impl ThumbSize {
    pub const ALL: [ThumbSize; 3] = [ThumbSize::Normal, ThumbSize::Large, ThumbSize::XLarge];

    pub fn dir_name(self) -> &'static str {
        match self {
            ThumbSize::Normal => "normal",
            ThumbSize::Large => "large",
            ThumbSize::XLarge => "x-large",
        }
    }

    /// Lado maior da miniatura.
    pub fn pixels(self) -> u32 {
        match self {
            ThumbSize::Normal => 128,
            ThumbSize::Large => 256,
            ThumbSize::XLarge => 512,
        }
    }
}

/// Tudo é escapado, exceto o que o GLib/KIO deixa passar num caminho de `file://`.
const URI_ESCAPE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@');

/// A URI `file://` do caminho absoluto, no mesmo formato que o Dolphin usa para achar a miniatura.
pub fn uri_of(path: &Path) -> String {
    let bytes = path.as_os_str().as_bytes();
    format!("file://{}", percent_encode(bytes, URI_ESCAPE))
}

/// O PNG da miniatura: `<thumbs_dir>/<tamanho>/<md5 da URI>.png`.
pub fn file_for(thumbs_dir: &Path, size: ThumbSize, uri: &str) -> PathBuf {
    let hash: String = Md5::digest(uri.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    thumbs_dir.join(size.dir_name()).join(format!("{hash}.png"))
}

/// O `Thumb::MTime` e o `Thumb::Size` de uma miniatura existente (cada um `None` se faltar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: Option<i64>,
    pub size: Option<u64>,
}

/// O `Stamp` de uma miniatura existente, lendo só o cabeçalho do PNG.
pub fn stored_stamp(file: &Path) -> Option<Stamp> {
    let reader = png::Decoder::new(BufReader::new(File::open(file).ok()?))
        .read_info()
        .ok()?;
    let info = reader.info();
    Some(Stamp {
        mtime: text_of(info, "Thumb::MTime"),
        size: text_of(info, "Thumb::Size"),
    })
}

/// O `Thumb::MTime` de uma miniatura existente.
pub fn stored_mtime(file: &Path) -> Option<i64> {
    stored_stamp(file)?.mtime
}

fn text_of<T: std::str::FromStr>(info: &png::Info, key: &str) -> Option<T> {
    info.uncompressed_latin1_text
        .iter()
        .find(|t| t.keyword == key)?
        .text
        .trim()
        .parse()
        .ok()
}

/// Decodifica a imagem uma vez e grava os três tamanhos (nunca amplia). Cada tamanho sai da
/// imagem original: reduzir em cadeia (512 → 256 → 128) acumula arredondamento e muda as
/// dimensões (600x400 daria 128x86 em vez de 128x85).
pub fn write_all(
    thumbs_dir: &Path,
    uri: &str,
    mtime_s: i64,
    file_size: u64,
    image: &[u8],
) -> Result<(), ThumbError> {
    let img = image::load_from_memory(image).map_err(|e| ThumbError::Decode(e.to_string()))?;
    for size in ThumbSize::ALL {
        let target = size.pixels();
        let scaled = if img.width().max(img.height()) > target {
            img.thumbnail(target, target).into_rgba8()
        } else {
            img.to_rgba8()
        };
        let png = encode_png(&scaled, uri, mtime_s, file_size)?;
        write_atomic(&file_for(thumbs_dir, size, uri), &png)?;
    }
    Ok(())
}

/// Apaga os três tamanhos; o que não existe é ignorado.
pub fn remove_all(thumbs_dir: &Path, uri: &str) {
    for size in ThumbSize::ALL {
        let file = file_for(thumbs_dir, size, uri);
        if let Err(e) = gdl_common::fs::remove_file_if_exists(&file) {
            tracing::debug!(erro = %e, arquivo = %file.display(), "falha ao apagar a miniatura");
        }
    }
}

fn encode_png(
    img: &image::RgbaImage,
    uri: &str,
    mtime_s: i64,
    file_size: u64,
) -> Result<Vec<u8>, ThumbError> {
    let to_err = |e: png::EncodingError| ThumbError::Encode(e.to_string());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.add_text_chunk("Thumb::URI".into(), uri.into())
        .map_err(to_err)?;
    enc.add_text_chunk("Thumb::MTime".into(), mtime_s.to_string())
        .map_err(to_err)?;
    enc.add_text_chunk("Thumb::Size".into(), file_size.to_string())
        .map_err(to_err)?;
    let mut writer = enc.write_header().map_err(to_err)?;
    writer.write_image_data(img.as_raw()).map_err(to_err)?;
    writer.finish().map_err(to_err)?;
    Ok(out)
}

/// Grava na pasta `0700` (criada se faltar) por troca atômica; o temporário já nasce `0600`.
fn write_atomic(file: &Path, data: &[u8]) -> Result<(), ThumbError> {
    gdl_common::paths::create_private_dir(file.parent().expect("o PNG sempre tem pasta"))?;
    Ok(gdl_common::fs::write_atomic(file, data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const URI: &str = "file:///a/b.jpg";

    /// JPEG `w`x`h` gerado no teste.
    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    /// Dimensões e textos de um PNG gravado.
    fn read_png(file: &Path) -> (u32, u32, Vec<(String, String)>) {
        let reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).unwrap()))
            .read_info()
            .unwrap();
        let info = reader.info();
        let texts = info
            .uncompressed_latin1_text
            .iter()
            .map(|t| (t.keyword.clone(), t.text.clone()))
            .collect();
        (info.width, info.height, texts)
    }

    fn text<'a>(texts: &'a [(String, String)], key: &str) -> &'a str {
        &texts.iter().find(|(k, _)| k == key).unwrap().1
    }

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn uri_matches_kio_encoding() {
        assert_eq!(
            uri_of(Path::new(
                "/home/u/Google Drive/Meu Drive/Fotos de Março #1 (100%).jpg"
            )),
            "file:///home/u/Google%20Drive/Meu%20Drive/Fotos%20de%20Mar%C3%A7o%20%231%20(100%25).jpg"
        );
    }

    #[test]
    fn uri_keeps_reserved_chars_and_escapes_question_mark() {
        assert_eq!(
            uri_of(Path::new("/a/b-c_d.e~f!$&'()*+,;=:@/x?y")),
            "file:///a/b-c_d.e~f!$&'()*+,;=:@/x%3Fy"
        );
    }

    #[test]
    fn file_name_is_md5_of_uri() {
        assert_eq!(
            file_for(Path::new("/c/thumbnails"), ThumbSize::Large, URI),
            PathBuf::from("/c/thumbnails/large/aaab0ed2bb45ccfb97782022c367076f.png")
        );
        assert_eq!(ThumbSize::XLarge.dir_name(), "x-large");
        assert_eq!(ThumbSize::ALL.map(ThumbSize::pixels), [128, 256, 512]);
    }

    #[test]
    fn write_all_creates_three_sizes_with_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("thumbnails");
        write_all(&dir, URI, 1_700_000_000, 12345, &jpeg(600, 400)).unwrap();
        for (size, w, h) in [
            (ThumbSize::Normal, 128, 85),
            (ThumbSize::Large, 256, 171),
            (ThumbSize::XLarge, 512, 341),
        ] {
            let f = file_for(&dir, size, URI);
            let (pw, ph, texts) = read_png(&f);
            assert_eq!((pw, ph), (w, h), "{size:?}");
            assert_eq!(text(&texts, "Thumb::URI"), URI);
            assert_eq!(text(&texts, "Thumb::MTime"), "1700000000");
            assert_eq!(text(&texts, "Thumb::Size"), "12345");
            assert_eq!(mode(&f), 0o600);
            assert_eq!(mode(f.parent().unwrap()), 0o700);
        }
        assert!(!dir.join("xx-large").exists());
        // Nenhum temporário sobra.
        let sobras = std::fs::read_dir(dir.join("normal")).unwrap().count();
        assert_eq!(sobras, 1);
    }

    #[test]
    fn small_image_is_not_upscaled() {
        let tmp = tempfile::tempdir().unwrap();
        write_all(tmp.path(), URI, 1, 1, &jpeg(100, 80)).unwrap();
        for size in ThumbSize::ALL {
            let (w, h, _) = read_png(&file_for(tmp.path(), size, URI));
            assert_eq!((w, h), (100, 80), "{size:?}");
        }
    }

    #[test]
    fn stored_mtime_reads_back_and_ignores_garbage() {
        let tmp = tempfile::tempdir().unwrap();
        write_all(tmp.path(), URI, 1_700_000_000, 1, &jpeg(50, 50)).unwrap();
        let f = file_for(tmp.path(), ThumbSize::Normal, URI);
        assert_eq!(stored_mtime(&f), Some(1_700_000_000));
        assert_eq!(
            stored_stamp(&f),
            Some(Stamp {
                mtime: Some(1_700_000_000),
                size: Some(1)
            })
        );
        assert_eq!(stored_mtime(&tmp.path().join("nao-existe.png")), None);
        let lixo = tmp.path().join("lixo.png");
        std::fs::write(&lixo, [7u8, 1, 200, 33, 9, 250, 0, 4, 5, 6, 77, 88]).unwrap();
        assert_eq!(stored_mtime(&lixo), None);
    }

    #[test]
    fn rewriting_replaces_with_new_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        write_all(tmp.path(), URI, 10, 1, &jpeg(50, 50)).unwrap();
        write_all(tmp.path(), URI, 20, 1, &jpeg(50, 50)).unwrap();
        let f = file_for(tmp.path(), ThumbSize::Large, URI);
        assert_eq!(stored_mtime(&f), Some(20));
    }

    #[test]
    fn undecodable_image_is_an_error_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("thumbnails");
        let err = write_all(&dir, URI, 1, 1, b"nao e imagem").unwrap_err();
        assert!(matches!(err, ThumbError::Decode(_)), "{err:?}");
        let arquivos = walk(&dir);
        assert!(arquivos.is_empty(), "{arquivos:?}");
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return vec![];
        };
        rd.flatten()
            .flat_map(|e| {
                let p = e.path();
                if p.is_dir() { walk(&p) } else { vec![p] }
            })
            .collect()
    }

    #[test]
    fn remove_all_deletes_every_size() {
        let tmp = tempfile::tempdir().unwrap();
        write_all(tmp.path(), URI, 1, 1, &jpeg(50, 50)).unwrap();
        remove_all(tmp.path(), URI);
        assert!(walk(tmp.path()).is_empty());
        // Sem nada para apagar, não dá erro nem pânico.
        remove_all(tmp.path(), URI);
    }
}
