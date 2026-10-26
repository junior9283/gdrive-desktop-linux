//! O menu "Google Drive" do botão direito, o mesmo em todos os gerenciadores de arquivos (as
//! extensões só desenham o que o serviço devolve): quais itens a seleção mostra e com qual marca.

/// O id de D-Bus dos separadores entre grupos de itens.
pub const SEPARATOR: &str = "-";

/// As ações do menu. O id é o protocolo com as extensões; a tradução vem de `label_key`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    OpenWeb,
    CopyLink,
    Offline,
    OnlineOnly,
    NewDocument,
    NewSpreadsheet,
    NewPresentation,
    Refresh,
    Trash,
}

/// Um documento Google vazio de "Novo …": o tipo e a chave da tradução do nome.
pub struct NewFile {
    pub mime: &'static str,
    pub name_key: &'static str,
}

impl Action {
    pub const ALL: [Action; 9] = [
        Action::OpenWeb,
        Action::CopyLink,
        Action::Offline,
        Action::OnlineOnly,
        Action::NewDocument,
        Action::NewSpreadsheet,
        Action::NewPresentation,
        Action::Refresh,
        Action::Trash,
    ];

    /// `(id, chave do rótulo)`.
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Action::OpenWeb => ("open-web", "menu-open-web"),
            Action::CopyLink => ("copy-link", "menu-copy-link"),
            Action::Offline => ("offline", "menu-offline"),
            Action::OnlineOnly => ("online-only", "menu-online-only"),
            Action::NewDocument => ("new-document", "menu-new-document"),
            Action::NewSpreadsheet => ("new-spreadsheet", "menu-new-spreadsheet"),
            Action::NewPresentation => ("new-presentation", "menu-new-presentation"),
            Action::Refresh => ("refresh", "menu-refresh"),
            Action::Trash => ("trash", "menu-trash"),
        }
    }

    pub fn id(self) -> &'static str {
        self.names().0
    }

    pub fn label_key(self) -> &'static str {
        self.names().1
    }

    pub fn parse(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn new_file(self) -> Option<NewFile> {
        let (mime, name_key) = match self {
            Action::NewDocument => ("application/vnd.google-apps.document", "new-document-name"),
            Action::NewSpreadsheet => (
                "application/vnd.google-apps.spreadsheet",
                "new-spreadsheet-name",
            ),
            Action::NewPresentation => (
                "application/vnd.google-apps.presentation",
                "new-presentation-name",
            ),
            _ => return None,
        };
        Some(NewFile { mime, name_key })
    }
}

/// O que o menu precisa saber de cada item selecionado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picked {
    pub is_folder: bool,
    /// Tem endereço na web (itens ainda não enviados não têm).
    pub has_link: bool,
    pub offline: Offline,
}

/// O acesso off-line de um item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Offline {
    On,
    Off,
    /// Pasta com parte do conteúdo off-line.
    Mixed,
}

impl Offline {
    fn mark(self) -> &'static str {
        match self {
            Offline::On => "on",
            Offline::Off => "off",
            Offline::Mixed => "mixed",
        }
    }

    fn inverse(self) -> Offline {
        match self {
            Offline::On => Offline::Off,
            Offline::Off => Offline::On,
            Offline::Mixed => Offline::Mixed,
        }
    }
}

/// Os itens (ação, marca) para a seleção, em ordem; `None` é um separador. A marca é `on`, `off`
/// ou `mixed` nas duas escolhas de acesso off-line e vazia nos outros. Sem seleção, nenhum.
pub fn entries(picked: &[Picked]) -> Vec<(Option<Action>, &'static str)> {
    let Some(first) = picked.first() else {
        return Vec::new();
    };
    let single = (picked.len() == 1).then_some(first);
    let offline = match picked.iter().all(|p| p.offline == first.offline) {
        true => first.offline,
        false => Offline::Mixed,
    };
    let mut list = Vec::new();
    if single.is_some_and(|p| p.has_link) {
        list.extend([
            (Some(Action::OpenWeb), ""),
            (Some(Action::CopyLink), ""),
            (None, ""),
        ]);
    }
    list.extend([
        (Some(Action::Offline), offline.mark()),
        (Some(Action::OnlineOnly), offline.inverse().mark()),
        (None, ""),
    ]);
    if single.is_some_and(|p| p.is_folder) {
        list.extend([
            (Some(Action::NewDocument), ""),
            (Some(Action::NewSpreadsheet), ""),
            (Some(Action::NewPresentation), ""),
            (Some(Action::Refresh), ""),
            (None, ""),
        ]);
    }
    list.push((Some(Action::Trash), ""));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: Picked = Picked {
        is_folder: false,
        has_link: true,
        offline: Offline::Off,
    };
    const FOLDER: Picked = Picked {
        is_folder: true,
        ..FILE
    };

    fn ids(list: &[(Option<Action>, &str)]) -> Vec<&'static str> {
        list.iter()
            .map(|(a, _)| a.map_or(SEPARATOR, Action::id))
            .collect()
    }

    fn mark<'a>(list: &[(Option<Action>, &'a str)], action: Action) -> &'a str {
        list.iter().find(|(a, _)| *a == Some(action)).unwrap().1
    }

    #[test]
    fn a_file_gets_links_offline_and_trash() {
        let list = entries(&[FILE]);
        assert_eq!(
            ids(&list),
            [
                "open-web",
                "copy-link",
                "-",
                "offline",
                "online-only",
                "-",
                "trash"
            ]
        );
        assert_eq!(mark(&list, Action::Offline), "off");
        assert_eq!(mark(&list, Action::OnlineOnly), "on");
    }

    #[test]
    fn a_folder_also_creates_and_refreshes() {
        let list = entries(&[FOLDER]);
        for id in [
            "new-document",
            "new-spreadsheet",
            "new-presentation",
            "refresh",
        ] {
            assert!(ids(&list).contains(&id), "{id}");
        }
    }

    #[test]
    fn several_items_have_no_links_nor_creation_and_mix_the_marks() {
        let on = Picked {
            offline: Offline::On,
            ..FILE
        };
        let list = entries(&[FOLDER, on]);
        assert_eq!(ids(&list), ["offline", "online-only", "-", "trash"]);
        assert_eq!(mark(&list, Action::Offline), "mixed");
        assert_eq!(mark(&list, Action::OnlineOnly), "mixed");
        assert_eq!(mark(&entries(&[on, on]), Action::Offline), "on");
    }

    #[test]
    fn without_a_link_there_is_nothing_to_open_or_copy() {
        let list = entries(&[Picked {
            has_link: false,
            ..FOLDER
        }]);
        assert!(!ids(&list).contains(&"open-web"));
        assert!(!ids(&list).contains(&"copy-link"));
        assert!(entries(&[]).is_empty());
    }

    #[test]
    fn ids_round_trip_and_only_new_items_create_files() {
        for action in Action::ALL {
            assert_eq!(Action::parse(action.id()), Some(action));
        }
        assert_eq!(Action::parse("apagar-tudo"), None);
        let sheet = Action::NewSpreadsheet.new_file().unwrap();
        assert_eq!(sheet.mime, "application/vnd.google-apps.spreadsheet");
        assert!(Action::Trash.new_file().is_none());
    }

    #[test]
    fn every_label_and_new_name_is_translated() {
        for locale in [
            include_str!("../../../locales/pt-BR/main.ftl"),
            include_str!("../../../locales/en/main.ftl"),
        ] {
            for action in Action::ALL {
                let keys = [
                    Some(action.label_key()),
                    action.new_file().map(|f| f.name_key),
                ];
                for key in keys.into_iter().flatten() {
                    assert!(locale.contains(&format!("\n{key} = ")), "{key}");
                }
            }
        }
    }
}
