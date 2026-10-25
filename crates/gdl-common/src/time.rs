//! Relógio em milissegundos desde 1970 (UTC), o formato usado no banco e na API.

pub fn now_ms() -> i64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis() as i64
}
