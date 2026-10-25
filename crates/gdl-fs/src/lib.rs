//! Sistema de arquivos FUSE da montagem do Drive.

mod fs;
mod inodes;
mod layout;

pub use fs::{DriveFs, mount};

#[cfg(test)]
mod read_tests;
