//! Conversão entre nomes do Google Drive e nomes válidos no Linux.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Limite de bytes de um nome de arquivo no Linux (`NAME_MAX`).
const NAME_MAX: usize = 255;
/// Extensões maiores que isso não são preservadas ao cortar nomes longos.
const MAX_EXT_BYTES: usize = 16;
/// `/` não pode aparecer num nome no Linux; no lugar dele usamos "∕" (U+2215).
const DIVISION_SLASH: char = '\u{2215}';
/// Substitutos de `.` e `..`, que são nomes reservados no Linux.
const ONE_DOT_LEADER: &str = "\u{2024}";
const TWO_DOT_LEADER: &str = "\u{2025}";

/// Um item de uma pasta, usado para resolver nomes repetidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sibling {
    pub id: String,
    pub name: String,
    pub created: i64,
}

/// Converte um nome vindo do Drive num nome válido no Linux.
pub fn to_local(remote: &str) -> String {
    let name = match remote {
        "" => "_".to_string(),
        "." => ONE_DOT_LEADER.to_string(),
        ".." => TWO_DOT_LEADER.to_string(),
        _ => remote.replace('/', &DIVISION_SLASH.to_string()),
    };
    if name.len() <= NAME_MAX {
        name
    } else {
        let (stem, ext) = split_ext(&name);
        fit(stem, &format!("~{}{ext}", short_hash(remote)))
    }
}

/// Converte um nome criado localmente no nome a enviar ao Drive.
pub fn to_remote(local: &str) -> String {
    match local {
        ONE_DOT_LEADER => ".".to_string(),
        TWO_DOT_LEADER => "..".to_string(),
        _ => local.replace(DIVISION_SLASH, "/"),
    }
}

/// Dá nomes locais únicos a itens com o mesmo nome na mesma pasta.
/// O mais antigo fica com o nome original; os demais recebem ` (1)`, ` (2)`…
/// O resultado vem em ordem de criação.
pub fn dedupe(siblings: &[Sibling]) -> Vec<(String, String)> {
    let mut ordered: Vec<&Sibling> = siblings.iter().collect();
    ordered.sort_by_key(|s| (s.created, &s.id));

    let originals: HashSet<&str> = siblings.iter().map(|s| s.name.as_str()).collect();
    let mut generated: HashSet<String> = HashSet::new();
    let mut next_suffix: HashMap<&str, u32> = HashMap::new();
    let mut out = Vec::with_capacity(siblings.len());
    for s in ordered {
        let Some(n) = next_suffix.get_mut(s.name.as_str()) else {
            next_suffix.insert(&s.name, 1);
            out.push((s.id.clone(), s.name.clone()));
            continue;
        };
        let (stem, ext) = split_ext(&s.name);
        let name = loop {
            let candidate = fit(stem, &format!(" ({n}){ext}"));
            *n += 1;
            if !originals.contains(candidate.as_str()) && generated.insert(candidate.clone()) {
                break candidate;
            }
        };
        out.push((s.id.clone(), name));
    }
    out
}

/// Separa a extensão (com o ponto) do resto do nome. Nomes como `.bashrc` não têm extensão.
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= MAX_EXT_BYTES => name.split_at(i),
        _ => (name, ""),
    }
}

/// Junta `stem` e `suffix`, cortando `stem` (em limite de caractere) para caber em `NAME_MAX`.
pub fn fit(stem: &str, suffix: &str) -> String {
    let mut cut = stem.len().min(NAME_MAX - suffix.len());
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{suffix}", &stem[..cut])
}

/// Cria em `dir` o arquivo `stem + suffix(n)` com o primeiro `n` (0, 1, 2…) cujo nome está
/// livre, sem sobrescrever nada. O nome é cortado com `fit` para caber no limite do Linux.
pub fn create_unique(
    dir: &Path,
    stem: &str,
    suffix: impl Fn(u32) -> String,
) -> std::io::Result<(PathBuf, File)> {
    let mut n = 0;
    loop {
        let path = dir.join(fit(stem, &suffix(n)));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// 8 dígitos hexadecimais do SHA-256 do nome original, para nomes cortados não colidirem.
fn short_hash(original: &str) -> String {
    let digest = Sha256::digest(original.as_bytes());
    format!(
        "{:08x}",
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    )
}

/// Forma de comparar nomes na pesquisa: minúsculas e sem os acentos do português e do espanhol
/// (`Relatório` e `RELATORIO` dão `relatorio`).
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("Relatório AÇÃO Ñandú"), "relatorio acao nandu");
        assert_eq!(fold("日本 Straße"), "日本 straße");
    }

    fn sib(id: &str, name: &str, created: i64) -> Sibling {
        Sibling {
            id: id.into(),
            name: name.into(),
            created,
        }
    }

    #[test]
    fn slash_becomes_division_slash() {
        assert_eq!(to_local("a/b"), "a∕b");
        assert_eq!(to_remote("a∕b"), "a/b");
    }

    #[test]
    fn dot_names_are_mapped() {
        assert_eq!(to_local("."), "․");
        assert_eq!(to_local(".."), "‥");
    }

    #[test]
    fn long_names_are_cut_to_255_bytes_keeping_extension() {
        let n = to_local(&format!("{}.pdf", "á".repeat(200)));
        assert!(n.len() <= 255, "tem {} bytes", n.len());
        assert!(n.ends_with(".pdf"));
        assert!(n.contains('~'));
        // o corte é determinístico
        assert_eq!(n, to_local(&format!("{}.pdf", "á".repeat(200))));
    }

    #[test]
    fn short_names_are_untouched() {
        assert_eq!(to_local("relatório final.docx"), "relatório final.docx");
    }

    #[test]
    fn duplicates_get_suffix_before_extension_by_creation_order() {
        let s = [
            sib("b", "foto.jpg", 20),
            sib("a", "foto.jpg", 10),
            sib("c", "foto.jpg", 30),
        ];
        assert_eq!(
            dedupe(&s),
            vec![
                ("a".into(), "foto.jpg".into()),
                ("b".into(), "foto (1).jpg".into()),
                ("c".into(), "foto (2).jpg".into()),
            ]
        );
    }

    #[test]
    fn dedupe_is_case_sensitive_like_linux() {
        let s = [sib("a", "A.txt", 1), sib("b", "a.txt", 2)];
        assert_eq!(
            dedupe(&s),
            vec![("a".into(), "A.txt".into()), ("b".into(), "a.txt".into())]
        );
    }

    #[test]
    fn dedupe_skips_suffix_already_taken() {
        let s = [
            sib("a", "x.txt", 1),
            sib("b", "x.txt", 2),
            sib("c", "x (1).txt", 3),
        ];
        assert_eq!(
            dedupe(&s),
            vec![
                ("a".into(), "x.txt".into()),
                ("b".into(), "x (2).txt".into()),
                ("c".into(), "x (1).txt".into()),
            ]
        );
    }

    #[test]
    fn deduped_long_names_stay_within_255_bytes() {
        let long = to_local(&format!("{}.pdf", "a".repeat(300)));
        let s = [sib("a", &long, 1), sib("b", &long, 2)];
        let out = dedupe(&s);
        assert!(out.iter().all(|(_, n)| n.len() <= 255), "{out:?}");
        assert!(out[1].1.ends_with(" (1).pdf"));
        assert_ne!(out[0].1, out[1].1);
    }

    #[test]
    fn empty_name_gets_placeholder() {
        assert_eq!(to_local(""), "_");
    }

    #[test]
    fn dedupe_without_extension_appends_suffix() {
        let s = [sib("a", "Notas", 1), sib("b", "Notas", 2)];
        assert_eq!(
            dedupe(&s),
            vec![
                ("a".into(), "Notas".into()),
                ("b".into(), "Notas (1)".into())
            ]
        );
    }
}
