//! Miniaturas no cache do freedesktop, sem baixar os arquivos.

pub mod freedesktop;
pub mod seeder;

/// O `Thumb::MTime` do arquivo: os segundos do `st_mtime` que a montagem mostra (datas antes de
/// 1970 aparecem como a época, como em `time_of` da montagem).
pub fn mtime_secs(modified_ms: i64) -> i64 {
    modified_ms.max(0) / 1000
}

#[cfg(test)]
mod tests {
    use super::mtime_secs;

    #[test]
    fn mtime_is_the_mount_second_and_never_negative() {
        assert_eq!(mtime_secs(1_999), 1);
        assert_eq!(mtime_secs(2_000), 2);
        // A montagem mostra datas antes de 1970 como a época (`time_of`).
        assert_eq!(mtime_secs(-1), 0);
        assert_eq!(mtime_secs(-5_500), 0);
    }
}
