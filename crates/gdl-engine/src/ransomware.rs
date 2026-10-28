//! Atividade suspeita: muitos arquivos regravados com cara de cifrados em pouco tempo. Um arquivo
//! conta quando o começo do conteúdo novo tem entropia alta e a extensão não é de formato que já
//! é comprimido, ou quando é renomeado para uma extensão de arquivo cifrado (`.locked`…); 20 em
//! 60 s pausam os envios da conta (as versões anteriores ficam no Drive).

use gdl_common::names::split_ext;
use std::collections::VecDeque;

/// Quanto do começo do arquivo é medido.
pub const SAMPLE_BYTES: u64 = 64 * 1024;
/// Entropia (bits por byte) acima da qual o conteúdo parece cifrado.
pub const MIN_ENTROPY: f64 = 7.6;
/// Quantos arquivos suspeitos na janela disparam o alerta.
pub const THRESHOLD: usize = 20;
pub const WINDOW_MS: i64 = 60_000;
/// "Eu reconheço esta atividade": sem alerta por este tempo.
pub const QUIET_MS: i64 = 60 * 60_000;

/// Formatos já comprimidos: a entropia deles é alta por natureza.
const COMPRESSED: &[&str] = &[
    "zip", "gz", "7z", "rar", "xz", "zst", "jpg", "jpeg", "png", "gif", "webp", "heic", "mp3",
    "mp4", "mkv", "mov", "avi", "ogg", "opus", "flac", "pdf", "docx", "xlsx", "pptx", "odt", "ods",
    "odp", "apk", "jar", "iso", "dmg",
];

/// Extensões que programas de sequestro de dados põem nos arquivos que cifram.
const ENCRYPTED: &[&str] = &[
    "locked",
    "encrypted",
    "enc",
    "crypt",
    "crypted",
    "crypto",
    "cry",
    "lock",
    "ransom",
    "pay",
    "aes",
    "rsa",
];

/// A extensão de `name` é de arquivo cifrado por sequestro de dados.
pub fn is_encrypted_name(name: &str) -> bool {
    let (_, ext) = split_ext(name);
    ENCRYPTED.contains(&ext.trim_start_matches('.').to_ascii_lowercase().as_str())
}

/// Entropia de Shannon de `bytes`, em bits por byte (0 a 8).
pub fn entropy(bytes: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let total = bytes.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / total;
            -p * p.log2()
        })
        .sum()
}

/// A extensão de `name` é de formato que já é comprimido (nunca conta, nem é lido).
pub fn is_compressed(name: &str) -> bool {
    let (_, ext) = split_ext(name);
    COMPRESSED.contains(&ext.trim_start_matches('.').to_ascii_lowercase().as_str())
}

/// O arquivo `name`, que começa com `head`, parece cifrado.
pub fn is_suspicious(name: &str, head: &[u8]) -> bool {
    !is_compressed(name) && entropy(head) > MIN_ENTROPY
}

/// Conta os arquivos suspeitos dos últimos `WINDOW_MS`.
#[derive(Debug, Default)]
pub struct Detector {
    hits: VecDeque<i64>,
    quiet_until: i64,
}

impl Detector {
    /// Um arquivo suspeito gravado em `now`; verdadeiro quando ele completa `THRESHOLD` na
    /// janela (a contagem recomeça).
    pub fn observe(&mut self, now: i64) -> bool {
        while self.hits.front().is_some_and(|&t| now - t >= WINDOW_MS) {
            self.hits.pop_front();
        }
        self.hits.push_back(now);
        if self.is_quiet(now) || self.hits.len() < THRESHOLD {
            return false;
        }
        self.hits.clear();
        true
    }

    /// Dentro da hora de "Eu reconheço esta atividade".
    pub fn is_quiet(&self, now: i64) -> bool {
        now < self.quiet_until
    }

    /// Sem alerta até `now + QUIET_MS`.
    pub fn quiet(&mut self, now: i64) {
        self.quiet_until = now + QUIET_MS;
        self.hits.clear();
    }
}

#[cfg(test)]
pub(crate) fn random_bytes(len: usize, seed: u64) -> Vec<u8> {
    // xorshift: pseudoaleatório o bastante para ter entropia perto de 8.
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_of_text_and_random() {
        let text = "O relatório do trimestre está na pasta compartilhada. ".repeat(500);
        assert!(entropy(text.as_bytes()) < 5.0);
        assert!(entropy(&random_bytes(64 * 1024, 7)) > 7.9);
        assert_eq!(entropy(&[]), 0.0);
        assert_eq!(entropy(&[42; 100]), 0.0);
    }

    #[test]
    fn compressed_formats_do_not_count() {
        let random = random_bytes(64 * 1024, 3);
        for name in [
            "foto.JPG",
            "musica.mp3",
            "pacote.zip",
            "planilha.xlsx",
            "f.tar.gz",
        ] {
            assert!(!is_suspicious(name, &random), "{name}");
        }
        assert!(is_suspicious("notas.txt", &random));
        assert!(is_suspicious("notas.txt.locked", &random));
        assert!(!is_suspicious("notas.txt", b"texto comum"));
        // Uma pasta inteira de fotos descompactada não chega perto.
        let mut detector = Detector::default();
        let photos = (0..200).filter(|n| is_suspicious(&format!("IMG_{n}.jpg"), &random));
        assert!(photos.map(|_| detector.observe(0)).all(|alert| !alert));
    }

    #[test]
    fn twenty_in_a_minute_alert_once() {
        let mut detector = Detector::default();
        let alerts: Vec<bool> = (0..19).map(|n| detector.observe(n * 1000)).collect();
        assert!(alerts.iter().all(|a| !a));
        assert!(detector.observe(19_000), "o vigésimo dispara");
        assert!(!detector.observe(19_500), "a contagem recomeça");

        let mut slow = Detector::default();
        assert!(
            (0..40).all(|n| !slow.observe(n * 4_000)),
            "espalhados em mais de um minuto, não"
        );
    }

    /// Gravações fechadas na montagem: 20 arquivos com cara de cifrados pausam os envios e
    /// avisam uma vez; "Eu reconheço" retoma e silencia; retomar volta a vigiar.
    #[tokio::test]
    async fn twenty_encrypted_files_pause() {
        use gdl_common::time::now_ms;
        use gdl_store::ItemId;

        use crate::events::EngineEvent;
        use crate::ops::Upload;

        let fake = gdl_fake_drive::FakeDrive::start().await;
        let (engine, _tmp) = crate::testutil::indexed_engine(&fake).await;
        let mut events = engine.subscribe();
        let root = ItemId(fake.root_id());
        let encrypt = |n: u64| {
            let engine = &engine;
            let root = root.clone();
            async move {
                let item = engine.create_file(&root, &format!("doc{n}.txt")).unwrap();
                let bytes = random_bytes(8192, n + 1);
                std::fs::write(engine.cache().local_path(&item.id), &bytes).unwrap();
                engine
                    .finish_write(&item.id, bytes.len() as u64, now_ms(), Upload::Soon)
                    .await
                    .unwrap();
            }
        };
        let suspicious = |events: &mut tokio::sync::broadcast::Receiver<EngineEvent>| {
            std::iter::from_fn(|| events.try_recv().ok())
                .filter(|e| *e == EngineEvent::SuspiciousActivity)
                .count()
        };

        for n in 0..19 {
            encrypt(n).await;
        }
        assert!(!engine.is_paused());
        encrypt(19).await;
        assert!(engine.is_paused(), "o vigésimo pausa os envios");
        assert_eq!(suspicious(&mut events), 1);

        engine.recognize_activity(now_ms());
        assert!(!engine.is_paused());
        for n in 20..60 {
            encrypt(n).await;
        }
        assert!(!engine.is_paused(), "reconhecida, fica quieta por 1 h");
        assert_eq!(suspicious(&mut events), 0);
    }

    /// Renomear em massa para extensão de arquivo cifrado conta no mesmo detector: o vigésimo
    /// pausa. Renomear um que já tinha a extensão, ou para outro nome qualquer, não conta.
    #[tokio::test]
    async fn mass_rename_to_encrypted_pauses() {
        use gdl_store::ItemId;

        let fake = gdl_fake_drive::FakeDrive::start().await;
        let root = fake.root_id();
        let ids: Vec<_> = (0..THRESHOLD)
            .map(|n| fake.add_file(&root, &format!("doc{n}.txt"), b"texto".to_vec()))
            .collect();
        fake.add_file(&root, "antigo.locked", b"x".to_vec());
        let (engine, _tmp) = crate::testutil::indexed_engine(&fake).await;
        let root = ItemId(root);
        let old = engine
            .store()
            .lookup(&root, "antigo.locked")
            .unwrap()
            .unwrap();
        engine
            .move_item(&old.id, &root, "antigo2.locked")
            .await
            .unwrap();
        for (n, id) in ids.iter().take(5).enumerate() {
            let id = ItemId(id.clone());
            engine
                .move_item(&id, &root, &format!("novo{n}.txt"))
                .await
                .unwrap();
        }

        for (n, id) in ids.iter().enumerate() {
            assert!(!engine.is_paused(), "pausou antes da hora, no {n}");
            let name = format!("doc{n}.txt.locked");
            engine
                .move_item(&ItemId(id.clone()), &root, &name)
                .await
                .unwrap();
        }
        assert!(engine.is_paused(), "o vigésimo pausa os envios");
    }

    #[test]
    fn encrypted_extensions() {
        assert!(is_encrypted_name("relatorio.docx.locked"));
        assert!(is_encrypted_name("FOTO.JPG.Encrypted"));
        assert!(!is_encrypted_name("relatorio.docx"));
        assert!(!is_encrypted_name("locked"));
    }

    /// A pausa por atividade suspeita continua depois de reabrir a conta, e "Eu reconheço" não
    /// desfaz a pausa do usuário.
    #[tokio::test]
    async fn suspicious_pause_survives_restart() {
        use gdl_common::config::load_preferences;
        use gdl_common::paths::Paths;
        use gdl_common::time::now_ms;

        use crate::engine::Engine;
        use crate::testutil::fast_client;

        let fake = gdl_fake_drive::FakeDrive::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let (prefs, _) = load_preferences(&paths).unwrap();
        let open = || Engine::open(&paths, &prefs, "teste@example.com", fast_client(&fake, 3));
        let engine = open().unwrap();
        let file = tmp.path().join("cifrado");
        std::fs::write(&file, random_bytes(8192, 7)).unwrap();
        let now = now_ms();
        for n in 0..THRESHOLD as i64 {
            engine.screen_write(&format!("doc{n}.txt"), &file, now + n);
        }
        assert!(engine.is_paused());
        drop(engine);

        let engine = open().unwrap();
        assert!(
            engine.is_paused(),
            "reaberta, a pausa continua até o usuário decidir"
        );
        assert!(!engine.screen_write("outro.txt", &file, now + 100));

        engine.pause();
        engine.recognize_activity(now + 200);
        assert!(engine.is_paused(), "a pausa do usuário continua");
        engine.resume();
        drop(engine);
        assert!(
            !open().unwrap().is_paused(),
            "reconhecida, não volta a pausar"
        );
    }

    #[test]
    fn recognized_activity_is_quiet_for_an_hour() {
        let mut detector = Detector::default();
        detector.quiet(0);
        assert!(
            (0..100).all(|n| !detector.observe(n * 100)),
            "na hora quieta"
        );
        let later = QUIET_MS;
        assert!((0..19).all(|n| !detector.observe(later + n)));
        assert!(
            detector.observe(later + 19),
            "depois de 1 h, volta a disparar"
        );
    }
}
