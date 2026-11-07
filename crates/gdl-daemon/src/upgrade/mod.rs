//! A atualização automática: como o app foi instalado, o download conferido do release e a
//! instalação (`crate::updates` acha a versão nova).

pub mod kind;

pub use kind::{InstallKind, install_kind};
