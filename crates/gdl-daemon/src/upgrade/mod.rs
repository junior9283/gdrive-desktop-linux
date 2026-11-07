//! A atualização automática: como o app foi instalado, o download conferido do release e a
//! instalação (`crate::updates` acha a versão nova).

pub mod install;
pub mod kind;
pub mod offer;
pub mod verify;

pub use kind::{InstallKind, install_kind};
pub use offer::{Offer, ReleaseFiles, offer};
pub use verify::Downloaded;

/// Por que a atualização não aconteceu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    /// Sem rede ou arquivo do release fora do ar.
    Network,
    Verify(verify::VerifyError),
    /// Não deu para instalar: gravar o pacote baixado ou rodar o instalador (o motivo).
    Install(String),
    /// Já há uma atualização em andamento.
    Busy,
}
