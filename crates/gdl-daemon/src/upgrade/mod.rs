//! A atualização automática: como o app foi instalado, o download conferido do release e a
//! instalação (`crate::updates` acha a versão nova).

pub mod kind;
pub mod offer;

pub use kind::{InstallKind, install_kind};
pub use offer::{Offer, ReleaseFiles, offer};
