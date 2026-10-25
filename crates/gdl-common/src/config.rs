//! Preferências em três camadas: política do administrador > usuário > padrão da máquina.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::fs::write_atomic;
use crate::paths::Paths;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("a preferência {0} é definida pelo administrador")]
    Managed(String),
    #[error("preferência desconhecida: {0}")]
    UnknownKey(String),
    #[error("valor inválido para {key}: {source}")]
    InvalidValue {
        key: String,
        source: toml::de::Error,
    },
    #[error("erro ao ler {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("erro de E/S em {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Tipo do campo em `Preferences`: `Option` quando não há padrão.
macro_rules! resolved_ty {
    ($ty:ty) => { Option<$ty> };
    ($ty:ty, $default:expr) => { $ty };
}

/// Valor efetivo: o da camada mesclada, ou o padrão quando houver.
macro_rules! resolve {
    ($value:expr) => {
        $value
    };
    ($value:expr, $default:expr) => {
        $value.unwrap_or($default)
    };
}

/// Fonte única das preferências: gera `KNOWN_KEYS`, a camada TOML (`Layer`) e as
/// preferências efetivas (`Preferences`).
macro_rules! preferences {
    ($($key:literal => $field:ident: $ty:ty $(= $default:expr)?;)*) => {
        /// Todas as chaves aceitas, com os mesmos nomes do app original (spec §6.16).
        const KNOWN_KEYS: &[&str] = &[$($key),*];

        /// Uma camada (um arquivo TOML); tudo opcional.
        #[derive(Debug, Default, Deserialize)]
        #[serde(default)]
        struct Layer {
            $(#[serde(rename = $key)] $field: Option<$ty>,)*
        }

        impl Layer {
            /// Campo a campo, o valor de `self` vence o de `lower`.
            fn over(self, lower: Layer) -> Layer {
                Layer { $($field: self.$field.or(lower.$field),)* }
            }

            fn resolve(self) -> Preferences {
                Preferences { $($field: resolve!(self.$field $(, $default)?),)* }
            }
        }

        /// Preferências efetivas, já com as três camadas mescladas e os padrões aplicados.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Preferences {
            $(pub $field: resolved_ty!($ty $(, $default)?),)*
        }
    };
}

preferences! {
    "AllowedAccountsPattern" => allowed_accounts_pattern: String;
    "AutoStartOnLogin" => auto_start_on_login: bool = true;
    "AutomaticErrorReporting" => automatic_error_reporting: bool = false;
    "BandwidthRxKBPS" => bandwidth_rx_kbps: u64;
    "BandwidthTxKBPS" => bandwidth_tx_kbps: u64;
    "ContentCacheMaxKbytes" => content_cache_max_kbytes: u64;
    "ContentCachePath" => content_cache_path: PathBuf;
    "DefaultMountPoint" => default_mount_point: PathBuf;
    "DefaultWebBrowser" => default_web_browser: PathBuf;
    "DirectConnection" => direct_connection: bool = false;
    "DisableAddAccount" => disable_add_account: bool = false;
    "DisableCRLCheck" => disable_crl_check: bool = false;
    "DisableExternalMediaSync" => disable_external_media_sync: bool = false;
    "DisableLocalizedVirtualFolders" => disable_localized_virtual_folders: bool = false;
    "DisableMirroredFolders" => disable_mirrored_folders: bool = false;
    "DisableMirroredMyDrive" => disable_mirrored_my_drive: bool = false;
    "DisableOnboardingDialog" => disable_onboarding_dialog: bool = false;
    "DisableSSLValidation" => disable_ssl_validation: bool = false;
    "DoNotShowNotifications" => do_not_show_notifications: Vec<String> = Vec::new();
    "MinFreeDiskSpaceKBytes" => min_free_disk_space_kbytes: u64 = 512_000;
    "OpenOfficeFilesInDocs" => open_office_files_in_docs: bool = true;
    "PauseSyncingOnMeteredConnection" => pause_syncing_on_metered_connection: bool = false;
    "PromptToBackupDevices" => prompt_to_backup_devices: bool = true;
    "SearchHotKey" => search_hot_key: String;
    "ShowMountPointInSidebar" => show_mount_point_in_sidebar: bool = true;
    "TrustSystemRootCerts" => trust_system_root_certs: bool = true;
    "TrustedRootCertsFile" => trusted_root_certs_file: PathBuf;
    "UITheme" => ui_theme: String;
}

fn policies_path(paths: &Paths) -> PathBuf {
    paths.etc_dir.join("policies.toml")
}

fn user_path(paths: &Paths) -> PathBuf {
    paths.config_dir.join("preferences.toml")
}

fn defaults_path(paths: &Paths) -> PathBuf {
    paths.etc_dir.join("defaults.toml")
}

/// Lê um arquivo TOML como tabela; arquivo ausente é uma tabela vazia.
fn read_table(path: &Path) -> Result<toml::Table, ConfigError> {
    match fs::read_to_string(path) {
        Ok(body) => body.parse().map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(source) => Err(ConfigError::Io {
            path: path.into(),
            source,
        }),
    }
}

/// Lê uma camada e devolve também as chaves conhecidas que ela define.
fn read_layer(path: &Path) -> Result<(Layer, HashSet<&'static str>), ConfigError> {
    let table = read_table(path)?;
    for key in table.keys().filter(|k| !KNOWN_KEYS.contains(&k.as_str())) {
        tracing::warn!(arquivo = %path.display(), chave = %key, "preferência desconhecida ignorada");
    }
    let defined = KNOWN_KEYS
        .iter()
        .copied()
        .filter(|k| table.contains_key(*k))
        .collect();
    let layer = table.try_into().map_err(|source| ConfigError::Parse {
        path: path.into(),
        source,
    })?;
    Ok((layer, defined))
}

/// Carrega as preferências efetivas e o conjunto de chaves definidas pela política.
pub fn load_preferences(
    paths: &Paths,
) -> Result<(Preferences, HashSet<&'static str>), ConfigError> {
    let (policy, managed) = read_layer(&policies_path(paths))?;
    let (user, _) = read_layer(&user_path(paths))?;
    let (defaults, _) = read_layer(&defaults_path(paths))?;
    Ok((policy.over(user).over(defaults).resolve(), managed))
}

/// Grava uma preferência do usuário, recusando chaves desconhecidas, definidas pela política
/// ou com valor de tipo errado. A gravação é atômica: o arquivo nunca fica pela metade.
pub fn save_user_preference(
    paths: &Paths,
    key: &str,
    value: toml::Value,
) -> Result<(), ConfigError> {
    if !KNOWN_KEYS.contains(&key) {
        return Err(ConfigError::UnknownKey(key.into()));
    }
    if read_table(&policies_path(paths))?.contains_key(key) {
        return Err(ConfigError::Managed(key.into()));
    }
    let probe = toml::Table::from_iter([(key.to_string(), value.clone())]);
    probe
        .try_into::<Layer>()
        .map_err(|source| ConfigError::InvalidValue {
            key: key.into(),
            source,
        })?;

    let path = user_path(paths);
    let mut table = read_table(&path)?;
    table.insert(key.into(), value);
    let io = |source| ConfigError::Io {
        path: path.clone(),
        source,
    };
    write_atomic(&path, table.to_string().as_bytes()).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &std::path::Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    #[test]
    fn policy_overrides_user_and_user_overrides_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        write(
            &paths.etc_dir.join("defaults.toml"),
            "ContentCacheMaxKbytes = 3\nDefaultMountPoint = \"/a\"\n",
        );
        write(
            &paths.config_dir.join("preferences.toml"),
            "ContentCacheMaxKbytes = 2\nDefaultMountPoint = \"/b\"\n",
        );
        write(
            &paths.etc_dir.join("policies.toml"),
            "ContentCacheMaxKbytes = 1\n",
        );
        let (prefs, managed) = load_preferences(&paths).unwrap();
        assert_eq!(prefs.content_cache_max_kbytes, Some(1));
        assert_eq!(prefs.default_mount_point, Some("/b".into()));
        assert!(managed.contains("ContentCacheMaxKbytes"));
        assert!(!managed.contains("DefaultMountPoint"));
    }

    #[test]
    fn saving_a_managed_key_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        write(
            &paths.etc_dir.join("policies.toml"),
            "BandwidthRxKBPS = 100\n",
        );
        let err =
            save_user_preference(&paths, "BandwidthRxKBPS", toml::Value::Integer(5)).unwrap_err();
        assert!(matches!(err, ConfigError::Managed(k) if k == "BandwidthRxKBPS"));
    }

    #[test]
    fn saved_user_preference_is_loaded_back() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        save_user_preference(
            &paths,
            "PauseSyncingOnMeteredConnection",
            toml::Value::Boolean(true),
        )
        .unwrap();
        let (prefs, _) = load_preferences(&paths).unwrap();
        assert!(prefs.pause_syncing_on_metered_connection);
    }

    #[test]
    fn saving_unknown_key_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let err =
            save_user_preference(&paths, "Inexistente", toml::Value::Boolean(true)).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownKey(_)));
    }

    #[test]
    fn saving_a_value_of_the_wrong_type_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let err = save_user_preference(
            &paths,
            "AutoStartOnLogin",
            toml::Value::String("sim".into()),
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { key, .. } if key == "AutoStartOnLogin"));
        assert!(load_preferences(&paths).is_ok());
    }

    #[test]
    fn min_free_disk_space_defaults_to_512000() {
        let tmp = tempfile::tempdir().unwrap();
        let (prefs, managed) = load_preferences(&Paths::for_test(tmp.path())).unwrap();
        assert_eq!(prefs.min_free_disk_space_kbytes, 512_000);
        assert!(managed.is_empty());
    }

    #[test]
    fn unknown_keys_are_ignored_with_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        write(
            &paths.config_dir.join("preferences.toml"),
            "ChaveDoFuturo = 1\nAutoStartOnLogin = false\n",
        );
        let (prefs, _) = load_preferences(&paths).unwrap();
        assert!(!prefs.auto_start_on_login);
    }
}
