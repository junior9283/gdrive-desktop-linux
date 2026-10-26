//! Miniaturas no cache do freedesktop, sem baixar os arquivos.

pub mod freedesktop;
pub mod seeder;

/// O `Thumb::MTime` do arquivo: os segundos do `st_mtime` que a montagem mostra.
pub fn mtime_secs(modified_ms: i64) -> i64 {
    modified_ms.div_euclid(1000)
}
