//! Sistema de arquivos FUSE da montagem do Drive.

mod fs;
mod inodes;
mod layout;
mod trash;
mod write;

pub use fs::{DriveFs, THUMB_WAIT, mount};
pub use layout::Layout;

#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod testutil;
#[cfg(test)]
mod write_tests;
