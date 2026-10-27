//! A decisão em três vias de um caminho do par: o que está no computador, o que está no Drive
//! e o último estado sincronizado. Sem E/S: quem chama lê os dois lados e aplica a ação.

pub use gdl_store::folders::Direction;

/// Um lado (computador ou Drive) de um caminho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Absent,
    Dir,
    /// No computador, `md5` só precisa vir quando tamanho ou data diferem da base (ou não há
    /// base): sem ele, a diferença de tamanho/data conta como edição.
    File {
        size: u64,
        mtime_ns: i64,
        md5: Option<String>,
    },
}

/// O último estado sincronizado do caminho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ns: i64,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Os dois lados já batem: só grava a base (ou a atualiza, como depois de um `touch`).
    UpdateBase,
    /// Os dois lados sumiram: esquece a base.
    ForgetBase,
    /// O arquivo do computador sobe (novo ou por cima da versão do Drive).
    Upload,
    /// O arquivo do Drive desce para o computador.
    Download,
    CreateRemoteDir,
    CreateLocalDir,
    /// Vai para a lixeira do Drive (nunca exclusão permanente).
    TrashRemote,
    /// Vai para a lixeira do sistema (nunca é apagado).
    TrashLocal,
    /// Os dois mudaram diferente: a versão local é renomeada com o sufixo de conflito e sobe, e
    /// a do Drive desce para o caminho original.
    Conflict,
    /// O conflito no backup: a versão local sobe com o sufixo de conflito e a do Drive fica (o
    /// dispositivo não é tocado).
    UploadAsConflict,
}

/// O que fazer com um caminho.
pub fn decide(local: &Side, remote: &Side, base: Option<&Base>, direction: Direction) -> Action {
    let action = two_way(local, remote, base);
    match direction {
        Direction::TwoWay => action,
        Direction::UploadOnly => backup(action, local, base),
    }
}

/// A decisão nos dois sentidos.
fn two_way(local: &Side, remote: &Side, base: Option<&Base>) -> Action {
    use Action::*;
    use Change::*;
    if kinds_differ(local, remote) {
        return Conflict;
    }
    let Some(base) = base else {
        return match (local, remote) {
            (Side::Absent, Side::Absent) => Nothing,
            (_, Side::Absent) => send(local),
            (Side::Absent, _) => bring(remote),
            _ if same_content(local, remote) => UpdateBase,
            _ => Conflict,
        };
    };
    match (local_change(local, base), remote_change(remote, base)) {
        (Same, Remote::Same) => Nothing,
        (Touched, Remote::Same) => UpdateBase,
        (Same | Touched | Gone, Remote::Changed) => bring(remote),
        (Changed, Remote::Same | Remote::Gone) => send(local),
        (Changed, Remote::Changed) if same_content(local, remote) => UpdateBase,
        (Changed, Remote::Changed) => Conflict,
        (Gone, Remote::Same) => TrashRemote,
        (Same | Touched, Remote::Gone) => TrashLocal,
        (Gone, Remote::Gone) => ForgetBase,
    }
}

/// O backup só sobe: o que viria do Drive não vem, excluir no computador não exclui no Drive, e
/// o que sumiu do Drive sobe de novo.
fn backup(action: Action, local: &Side, base: Option<&Base>) -> Action {
    use Action::*;
    match action {
        Download | CreateLocalDir if base.is_some() && *local == Side::Absent => ForgetBase,
        Download | CreateLocalDir => Nothing,
        TrashRemote => ForgetBase,
        TrashLocal => send(local),
        Conflict => UploadAsConflict,
        other => other,
    }
}

/// Os dois lados são arquivos com o mesmo md5 conhecido.
fn same_content(local: &Side, remote: &Side) -> bool {
    matches!(
        (local, remote),
        (Side::File { md5: Some(l), .. }, Side::File { md5: Some(r), .. }) if l == r
    ) || matches!((local, remote), (Side::Dir, Side::Dir))
}

/// Como o computador ficou em relação à base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Same,
    /// Só a data (ou o tamanho informado) mudou; o conteúdo é o mesmo.
    Touched,
    Changed,
    Gone,
}

fn local_change(local: &Side, base: &Base) -> Change {
    match local {
        Side::Absent => Change::Gone,
        Side::Dir if base.is_dir => Change::Same,
        Side::Dir => Change::Changed,
        Side::File { .. } if base.is_dir => Change::Changed,
        Side::File {
            size,
            mtime_ns,
            md5,
        } => {
            let same_stat = *size == base.size && *mtime_ns == base.mtime_ns;
            match md5 {
                Some(_) if *md5 != base.md5 => Change::Changed,
                Some(_) if !same_stat => Change::Touched,
                None if !same_stat => Change::Changed,
                _ => Change::Same,
            }
        }
    }
}

/// Como o Drive ficou em relação à base (lá só o conteúdo conta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Remote {
    Same,
    Changed,
    Gone,
}

fn remote_change(remote: &Side, base: &Base) -> Remote {
    match remote {
        Side::Absent => Remote::Gone,
        Side::Dir if base.is_dir => Remote::Same,
        Side::File { md5, .. } if !base.is_dir && *md5 == base.md5 => Remote::Same,
        _ => Remote::Changed,
    }
}

/// Os dois lados existem e um é pasta e o outro, arquivo.
fn kinds_differ(local: &Side, remote: &Side) -> bool {
    matches!(
        (local, remote),
        (Side::Dir, Side::File { .. }) | (Side::File { .. }, Side::Dir)
    )
}

/// Leva o lado do computador para o Drive.
fn send(local: &Side) -> Action {
    match local {
        Side::Dir => Action::CreateRemoteDir,
        Side::File { .. } => Action::Upload,
        Side::Absent => Action::Nothing,
    }
}

/// Traz o lado do Drive para o computador.
fn bring(remote: &Side) -> Action {
    match remote {
        Side::Dir => Action::CreateLocalDir,
        Side::File { .. } => Action::Download,
        Side::Absent => Action::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;

    fn file(content: &str) -> Side {
        Side::File {
            size: content.len() as u64,
            mtime_ns: 1,
            md5: Some(content.into()),
        }
    }

    fn base(content: &str) -> Base {
        Base {
            is_dir: false,
            size: content.len() as u64,
            mtime_ns: 1,
            md5: Some(content.into()),
        }
    }

    fn dir_base() -> Base {
        Base {
            is_dir: true,
            size: 0,
            mtime_ns: 0,
            md5: None,
        }
    }

    fn two_way(local: &Side, remote: &Side, base: Option<&Base>) -> Action {
        decide(local, remote, base, Direction::TwoWay)
    }

    #[test]
    fn without_a_base_each_side_brings_what_it_has() {
        let none = &Side::Absent;
        assert_eq!(two_way(none, none, None), Nothing);
        assert_eq!(two_way(&file("a"), none, None), Upload);
        assert_eq!(two_way(none, &file("a"), None), Download);
        assert_eq!(two_way(&file("a"), &file("a"), None), UpdateBase);
        assert_eq!(two_way(&file("a"), &file("b"), None), Conflict);
        assert_eq!(two_way(&Side::Dir, none, None), CreateRemoteDir);
        assert_eq!(two_way(none, &Side::Dir, None), CreateLocalDir);
        assert_eq!(two_way(&Side::Dir, &Side::Dir, None), UpdateBase);
        assert_eq!(two_way(&file("a"), &Side::Dir, None), Conflict);
        assert_eq!(two_way(&Side::Dir, &file("a"), None), Conflict);
    }

    #[test]
    fn with_a_base_the_side_that_changed_wins() {
        let b = &base("a");
        let none = &Side::Absent;
        assert_eq!(two_way(&file("a"), &file("a"), Some(b)), Nothing);
        assert_eq!(two_way(&file("b"), &file("a"), Some(b)), Upload);
        assert_eq!(two_way(&file("a"), &file("b"), Some(b)), Download);
        assert_eq!(two_way(&file("b"), &file("b"), Some(b)), UpdateBase);
        assert_eq!(two_way(&file("b"), &file("c"), Some(b)), Conflict);
        assert_eq!(two_way(none, &file("a"), Some(b)), TrashRemote);
        assert_eq!(two_way(&file("a"), none, Some(b)), TrashLocal);
        assert_eq!(two_way(none, none, Some(b)), ForgetBase);
    }

    #[test]
    fn an_edit_beats_a_deletion_on_the_other_side() {
        let b = &base("a");
        let none = &Side::Absent;
        assert_eq!(two_way(none, &file("b"), Some(b)), Download);
        assert_eq!(two_way(&file("b"), none, Some(b)), Upload);
    }

    #[test]
    fn local_size_or_date_only_counts_when_the_content_is_unknown() {
        let b = &base("a");
        let touched = Side::File {
            size: 1,
            mtime_ns: 99,
            md5: Some("a".into()),
        };
        assert_eq!(two_way(&touched, &file("a"), Some(b)), UpdateBase);
        assert_eq!(two_way(&touched, &file("b"), Some(b)), Download);
        let unknown = Side::File {
            size: 1,
            mtime_ns: 99,
            md5: None,
        };
        assert_eq!(two_way(&unknown, &file("a"), Some(b)), Upload);
        let same_stat = Side::File {
            size: 1,
            mtime_ns: 1,
            md5: None,
        };
        assert_eq!(two_way(&same_stat, &file("a"), Some(b)), Nothing);
    }

    #[test]
    fn folders_follow_the_same_rules() {
        let b = &dir_base();
        let none = &Side::Absent;
        assert_eq!(two_way(&Side::Dir, &Side::Dir, Some(b)), Nothing);
        assert_eq!(two_way(none, &Side::Dir, Some(b)), TrashRemote);
        assert_eq!(two_way(&Side::Dir, none, Some(b)), TrashLocal);
        assert_eq!(two_way(&file("a"), &Side::Dir, Some(b)), Conflict);
    }

    #[test]
    fn backup_only_goes_up_and_never_deletes() {
        let b = &base("a");
        let none = &Side::Absent;
        let up = |l: &Side, r: &Side, base| decide(l, r, base, Direction::UploadOnly);
        assert_eq!(up(&file("a"), none, None), Upload);
        assert_eq!(up(&Side::Dir, none, None), CreateRemoteDir);
        assert_eq!(up(&file("b"), &file("a"), Some(b)), Upload);
        assert_eq!(up(&file("a"), &file("a"), Some(b)), Nothing);
        assert_eq!(
            up(none, &file("a"), Some(b)),
            ForgetBase,
            "excluir no computador não exclui no Drive"
        );
        assert_eq!(
            up(&file("a"), none, Some(b)),
            Upload,
            "apagado no Drive: sobe de novo"
        );
        assert_eq!(
            up(&file("a"), &file("b"), Some(b)),
            Nothing,
            "nada do Drive volta"
        );
        assert_eq!(up(none, &file("a"), None), Nothing);
        assert_eq!(
            up(&file("b"), &file("c"), Some(b)),
            UploadAsConflict,
            "os dois mudaram: a local sobe com outro nome"
        );
    }

    /// O que existiu em algum lado continua existindo em algum lugar depois da ação: nos
    /// lados, na lixeira do Drive ou na do sistema.
    #[test]
    fn no_content_is_ever_lost() {
        let contents = ["a", "b", "c"];
        let mut sides = vec![Side::Absent, Side::Dir];
        sides.extend(contents.map(file));
        let mut bases = vec![None, Some(dir_base())];
        bases.extend(contents.map(|c| Some(base(c))));
        for direction in [Direction::TwoWay, Direction::UploadOnly] {
            for local in &sides {
                for remote in &sides {
                    for b in &bases {
                        let action = decide(local, remote, b.as_ref(), direction);
                        let after = apply(action, local, remote, b.as_ref());
                        for before in [local, remote] {
                            assert!(
                                matches!(before, Side::Absent) || after.contains(before),
                                "{action:?} perdeu {before:?} (local {local:?}, Drive \
                                 {remote:?}, base {b:?}, {direction:?})"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Onde cada conteúdo fica depois da ação (os dois lados e as duas lixeiras juntos).
    /// Sobrescrever um lado só preserva o que estava nele se era a versão da base (que já está
    /// no histórico de versões do Drive).
    fn apply(action: Action, local: &Side, remote: &Side, base: Option<&Base>) -> Vec<Side> {
        let (l, r) = (local.clone(), remote.clone());
        let kept = |side: Side| match (&side, base) {
            (Side::File { md5, .. }, Some(b)) if *md5 == b.md5 => vec![side],
            _ => vec![],
        };
        match action {
            Upload => [vec![l], kept(r)].concat(),
            Download => [vec![r], kept(l)].concat(),
            // Lixeira, conflito (as duas versões ficam) e o resto não sobrescrevem nada.
            _ => vec![l, r],
        }
    }
}
