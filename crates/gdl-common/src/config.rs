//! Preferências em três camadas: política do administrador > usuário > padrão da máquina.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::fs::{remove_file_if_exists, write_atomic};
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

/// Padrão como valor TOML, para `pref_values`.
macro_rules! default_value {
    ($ty:ty) => {
        None
    };
    ($ty:ty, $default:expr) => {
        toml::Value::try_from::<$ty>($default).ok()
    };
}

/// Fonte única das preferências: gera `KNOWN_KEYS`, a camada TOML (`Layer`), as preferências
/// efetivas (`Preferences`) e os padrões de cada chave (`default_value`).
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

        /// Valor padrão de `key`, para as chaves que têm um.
        fn default_value(key: &str) -> Option<toml::Value> {
            match key {
                $($key => default_value!($ty $(, $default)?),)*
                _ => None,
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
    // Própria deste app: o proxy manual (endereço), que o original não tem como chave.
    "ProxyServer" => proxy_server: String;
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

/// Preferências que só valem depois de reiniciar o serviço (cache, montagem e rede); as
/// outras valem na hora.
const RESTART_KEYS: &[&str] = &[
    "ContentCacheMaxKbytes",
    "ContentCachePath",
    "DefaultMountPoint",
    "DirectConnection",
    "ProxyServer",
    "TrustSystemRootCerts",
    "TrustedRootCertsFile",
];

pub fn needs_restart(key: &str) -> bool {
    RESTART_KEYS.contains(&key)
}

/// Uma preferência como a janela de Preferências mostra: o valor efetivo (ou o padrão; `None`
/// sem nenhum dos dois) e se a política do administrador a define.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefValue {
    pub key: &'static str,
    pub value: Option<toml::Value>,
    pub managed: bool,
}

/// Todas as preferências conhecidas, com o valor efetivo de cada uma.
pub fn pref_values(paths: &Paths) -> Result<Vec<PrefValue>, ConfigError> {
    let policy = read_table(&policies_path(paths))?;
    let layers = [
        &policy,
        &read_table(&user_path(paths))?,
        &read_table(&defaults_path(paths))?,
    ];
    Ok(KNOWN_KEYS
        .iter()
        .map(|&key| PrefValue {
            key,
            value: layers
                .iter()
                .find_map(|t| t.get(key).cloned())
                .or_else(|| default_value(key)),
            managed: policy.contains_key(key),
        })
        .collect())
}

/// Grava uma preferência do usuário (ver `set_user_preferences`).
pub fn save_user_preference(
    paths: &Paths,
    key: &str,
    value: toml::Value,
) -> Result<(), ConfigError> {
    set_user_preferences(paths, &[(key.into(), value)])
}

/// Esquemas aceitos em `ProxyServer`.
pub const PROXY_SCHEMES: [&str; 4] = ["http", "https", "socks5", "socks5h"];

/// O proxy manual de `ProxyServer`, sem espaços; vazio é nenhum.
fn manual_proxy(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|p| !p.is_empty())
}

impl Preferences {
    /// O proxy manual (`ProxyServer`), se houver.
    pub fn manual_proxy(&self) -> Option<&str> {
        manual_proxy(self.proxy_server.as_deref())
    }
}

impl Layer {
    /// O que o tipo do campo não verifica. Só na gravação: um valor ruim já no arquivo não
    /// impede a partida.
    fn check(&self) -> Result<(), toml::de::Error> {
        if let Some(proxy) = manual_proxy(self.proxy_server.as_deref()) {
            let valid = url::Url::parse(proxy).is_ok_and(|url| {
                PROXY_SCHEMES.contains(&url.scheme())
                    && url.host_str().is_some_and(|h| !h.is_empty())
            });
            if !valid {
                return Err(serde::de::Error::custom(format!(
                    "endereço de proxy esperado ({}://)",
                    PROXY_SCHEMES.join("://, ")
                )));
            }
        }
        Ok(())
    }
}

/// Grava várias preferências do usuário de uma vez. Recusa tudo, sem gravar nada, se alguma
/// chave for desconhecida, definida pela política ou tiver valor de tipo errado: um valor
/// inválido nunca chega ao arquivo (e nunca impede a próxima partida). A gravação é atômica.
pub fn set_user_preferences(
    paths: &Paths,
    values: &[(String, toml::Value)],
) -> Result<(), ConfigError> {
    let policy = read_table(&policies_path(paths))?;
    for (key, value) in values {
        if !KNOWN_KEYS.contains(&key.as_str()) {
            return Err(ConfigError::UnknownKey(key.clone()));
        }
        if policy.contains_key(key) {
            return Err(ConfigError::Managed(key.clone()));
        }
        let probe = toml::Table::from_iter([(key.clone(), value.clone())]);
        let invalid = |source| ConfigError::InvalidValue {
            key: key.clone(),
            source,
        };
        let layer = probe.try_into::<Layer>().map_err(invalid)?;
        layer.check().map_err(invalid)?;
    }

    let path = user_path(paths);
    let mut table = read_table(&path)?;
    table.extend(values.iter().cloned());
    write_atomic(&path, table.to_string().as_bytes()).map_err(|source| ConfigError::Io {
        path: path.clone(),
        source,
    })
}

/// "Restaurar padrões": apaga as preferências do usuário. As da política continuam valendo.
pub fn reset_user_preferences(paths: &Paths) -> Result<(), ConfigError> {
    let path = user_path(paths);
    remove_file_if_exists(&path).map_err(|source| ConfigError::Io { path, source })
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
    fn set_preferences_rejects_bad_values_before_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let user = paths.config_dir.join("preferences.toml");
        write(&user, "UITheme = \"light\"\n");

        let err = set_user_preferences(
            &paths,
            &[
                ("UITheme".into(), toml::Value::String("dark".into())),
                (
                    "BandwidthRxKBPS".into(),
                    toml::Value::String("rápido".into()),
                ),
            ],
        )
        .unwrap_err();

        assert!(matches!(err, ConfigError::InvalidValue { key, .. } if key == "BandwidthRxKBPS"));
        assert_eq!(fs::read_to_string(&user).unwrap(), "UITheme = \"light\"\n");
        set_user_preferences(
            &paths,
            &[
                ("UITheme".into(), toml::Value::String("dark".into())),
                ("BandwidthRxKBPS".into(), toml::Value::Integer(300)),
            ],
        )
        .unwrap();
        let (prefs, _) = load_preferences(&paths).unwrap();
        assert_eq!(prefs.ui_theme.as_deref(), Some("dark"));
        assert_eq!(prefs.bandwidth_rx_kbps, Some(300));
    }

    #[test]
    fn reset_preferences_keeps_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        write(
            &paths.etc_dir.join("policies.toml"),
            "AutoStartOnLogin = false\n",
        );
        save_user_preference(&paths, "UITheme", toml::Value::String("dark".into())).unwrap();

        reset_user_preferences(&paths).unwrap();

        let (prefs, managed) = load_preferences(&paths).unwrap();
        assert_eq!(prefs.ui_theme, None);
        assert!(!prefs.auto_start_on_login);
        assert!(managed.contains("AutoStartOnLogin"));
        reset_user_preferences(&paths).unwrap();
    }

    #[test]
    fn pref_values_show_effective_value_default_and_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        write(
            &paths.etc_dir.join("policies.toml"),
            "AutoStartOnLogin = false\n",
        );
        save_user_preference(&paths, "UITheme", toml::Value::String("dark".into())).unwrap();

        let values = pref_values(&paths).unwrap();
        let get = |key: &str| values.iter().find(|v| v.key == key).unwrap();

        assert_eq!(values.len(), KNOWN_KEYS.len());
        assert_eq!(
            get("UITheme").value,
            Some(toml::Value::String("dark".into()))
        );
        assert!(!get("UITheme").managed);
        assert_eq!(
            get("AutoStartOnLogin").value,
            Some(toml::Value::Boolean(false))
        );
        assert!(get("AutoStartOnLogin").managed);
        assert_eq!(
            get("MinFreeDiskSpaceKBytes").value,
            Some(toml::Value::Integer(512_000)),
            "padrão"
        );
        assert_eq!(get("ProxyServer").value, None, "sem padrão");
        assert!(needs_restart("ContentCacheMaxKbytes") && !needs_restart("UITheme"));
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

    #[test]
    fn proxy_server_must_be_a_proxy_url() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        for bad in ["proxy.empresa:3128", "ftp://proxy:21", "http://", "nada"] {
            let err = save_user_preference(&paths, "ProxyServer", bad.into()).unwrap_err();
            assert!(
                matches!(&err, ConfigError::InvalidValue { key, .. } if key == "ProxyServer"),
                "{bad}: {err}"
            );
        }
        for good in [
            "",
            "http://proxy:3128",
            "socks5://usuario:senha@127.0.0.1:1080",
        ] {
            save_user_preference(&paths, "ProxyServer", good.into()).unwrap();
        }
    }
}
