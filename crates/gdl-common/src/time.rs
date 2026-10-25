//! Relógio em milissegundos desde 1970 (UTC), o formato usado no banco e na API.

pub fn now_ms() -> i64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis() as i64
}

/// Data no formato da Drive API (RFC 3339, UTC, com milissegundos).
pub fn rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Data e hora locais no formato `AAAA-MM-DD HHMMSS`, que cabe num nome de arquivo.
pub fn local_stamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H%M%S")
        .to_string()
}

/// Lê uma data RFC 3339 da API, em ms desde 1970.
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|d| d.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_stamp_has_date_and_time_without_colons() {
        let stamp = local_stamp(now_ms());
        assert_eq!(stamp.len(), "2026-10-25 134640".len(), "{stamp}");
        assert!(!stamp.contains(':'));
    }

    #[test]
    fn rfc3339_round_trips_milliseconds() {
        let ms = 1_761_400_000_123;
        assert_eq!(rfc3339(ms), "2025-10-25T13:46:40.123Z");
        assert_eq!(parse_rfc3339(&rfc3339(ms)), Some(ms));
        assert_eq!(parse_rfc3339("ontem"), None);
    }
}
