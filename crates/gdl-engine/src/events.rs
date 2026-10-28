//! Eventos que o motor publica para a bandeja, as notificações e a CLI.

use std::path::PathBuf;

/// Resumo do estado da sincronização, mostrado na bandeja e na CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub pending_uploads: u32,
    pub paused: bool,
    pub offline: bool,
    /// O Google recusou o login da conta.
    pub needs_sign_in: bool,
    pub last_sync_ms: Option<i64>,
    pub errors: u32,
    /// O último envio esbarrou na cota cheia (até um envio dar certo).
    pub quota_full: bool,
    /// Itens subindo ou descendo: os envios na fila e os fixados que faltam baixar.
    pub syncing: u32,
    /// Estimativa pelo ritmo dos envios recentes; `None` sem amostras.
    pub remaining_secs: Option<u64>,
}

/// O estado que a bandeja e a janela principal mostram, do que menos ao que mais pede atenção
/// (`Status` sem conta mostra o maior entre as contas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncState {
    /// Sem conta conectada (o serviço responde isto; o motor só existe com conta).
    SignedOut,
    /// O serviço acabou de subir e ainda está reconectando as contas guardadas.
    Starting,
    Synced,
    Syncing,
    Paused,
    Offline,
    QuotaFull,
    Error,
    /// O login da conta venceu: nada sincroniza até ela entrar de novo.
    NeedsSignIn,
}

impl SyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncState::SignedOut => "signed-out",
            SyncState::Starting => "starting",
            SyncState::Synced => "synced",
            SyncState::Syncing => "syncing",
            SyncState::Paused => "paused",
            SyncState::Offline => "offline",
            SyncState::QuotaFull => "quota-full",
            SyncState::Error => "error",
            SyncState::NeedsSignIn => "needs-sign-in",
        }
    }
}

impl SyncSummary {
    /// Vale o primeiro que se aplicar: login vencido, pausado, sem internet, cota cheia,
    /// sincronizando, erro.
    pub fn state(&self) -> SyncState {
        if self.needs_sign_in {
            SyncState::NeedsSignIn
        } else if self.paused {
            SyncState::Paused
        } else if self.offline {
            SyncState::Offline
        } else if self.quota_full {
            SyncState::QuotaFull
        } else if self.syncing > 0 {
            SyncState::Syncing
        } else if self.errors > 0 {
            SyncState::Error
        } else {
            SyncState::Synced
        }
    }
}

/// Por que uma edição local foi guardada em `Lost and Found` em vez de subir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostReason {
    RemoteChanged,
    RemoteDeleted,
    NoPermission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    SyncStateChanged(SyncSummary),
    LostAndFound {
        path: PathBuf,
        reason: LostReason,
    },
    Offline(bool),
    /// O Google recusou (`true`) ou voltou a aceitar (`false`) o login da conta.
    NeedsSignIn(bool),
    DiskLow {
        free_kb: u64,
    },
    /// O armazenamento da conta está cheio: nada sobe até liberar espaço.
    QuotaFull,
    UploadFailed {
        name: String,
        error: String,
    },
    /// Um ponteiro de Documento Google copiado para uma pasta virou atalho (cópia de Documento
    /// Google não é suportada).
    CopiedAsShortcut {
        name: String,
    },
    /// Uma gravação foi registrada com a sincronização pausada (no máximo um por pausa): ela só
    /// sobe quando o usuário retomar.
    SavedWhilePaused {
        name: String,
    },
    /// O usuário cancelou o envio; a versão local foi para `Uploads cancelados`.
    UploadCanceled {
        name: String,
    },
    /// Um arquivo foi movido para um Drive compartilhado: quem pode vê-lo muda.
    MovedToSharedDrive {
        name: String,
    },
    /// Um arquivo saiu de um Drive compartilhado: os membros do drive deixam de vê-lo.
    MovedOutOfSharedDrive {
        name: String,
    },
    /// Um arquivo do usuário foi movido para uma pasta do Meu Drive que é de outra pessoa: quem
    /// tem acesso a ela passa a ver o arquivo.
    MovedToSharedFolder {
        name: String,
        folder: String,
    },
    /// Um Drive compartilhado teve mais de 1 000 mudanças numa consulta: a sincronização dele
    /// pode demorar (no máximo um aviso por drive a cada 24 h).
    BusySharedDrive {
        name: String,
    },
    /// Itens do banco mudaram pelo Drive (consulta de mudanças): as pastas sincronizadas que os
    /// contêm comparam de novo.
    ItemsChanged {
        ids: Vec<gdl_store::ItemId>,
    },
    /// Uma pasta não pôde mudar de Meu Drive para um Drive compartilhado (ou o contrário): a API
    /// não move pastas assim, só a web.
    FolderMoveUnsupported {
        name: String,
    },
    /// O Drive recusou mexer num item da montagem por falta de permissão.
    NoPermission {
        action: DeniedAction,
        name: String,
    },
    /// O armazenamento da conta subiu para a faixa de `percent` (90 ou 95).
    QuotaAlmostFull {
        percent: u8,
    },
    /// Envios na fila há mais de 3 dias que ainda não tinham sido avisados.
    UploadsStuck {
        count: usize,
    },
    /// Uma gravação na montagem falhou com o disco local cheio.
    LocalDiskFull,
    /// A organização bloqueou a conta para o Drive (`403 domainPolicy`).
    AccountDisabled,
    /// Muitos arquivos regravados com cara de cifrados: os envios da conta foram pausados.
    SuspiciousActivity,
    /// O banco da conta estava estragado: `count` edições que não subiram foram para o
    /// `Lost and Found` e a conta se reindexa.
    ItemsRecovered {
        count: usize,
    },
}

/// O que o Drive recusou fazer (`EngineEvent::NoPermission`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniedAction {
    Create,
    Move,
    Rename,
    Delete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_state_follows_the_precedence() {
        let base = SyncSummary::default();
        let with = |f: fn(&mut SyncSummary)| {
            let mut s = base;
            f(&mut s);
            s.state()
        };
        assert_eq!(
            with(|s| {
                s.needs_sign_in = true;
                s.paused = true;
                s.errors = 1;
            }),
            SyncState::NeedsSignIn
        );
        assert_eq!(
            with(|s| {
                s.paused = true;
                s.offline = true;
            }),
            SyncState::Paused
        );
        assert_eq!(
            with(|s| {
                s.offline = true;
                s.pending_uploads = 2;
                s.syncing = 2;
            }),
            SyncState::Offline
        );
        assert_eq!(
            with(|s| {
                s.quota_full = true;
                s.syncing = 1;
            }),
            SyncState::QuotaFull
        );
        assert_eq!(with(|s| s.syncing = 3), SyncState::Syncing);
        assert_eq!(with(|s| s.errors = 1), SyncState::Error);
        assert_eq!(base.state(), SyncState::Synced);
        assert_eq!(SyncState::QuotaFull.as_str(), "quota-full");
        assert!(SyncState::NeedsSignIn > SyncState::Error);
        assert_eq!(SyncState::NeedsSignIn.as_str(), "needs-sign-in");
    }
}
