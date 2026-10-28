//! Serviço do usuário: login, montagem da conta, interface D-Bus e notificações.

pub mod cosmic;
pub mod daemon;
pub mod dbus;
pub mod desktop;
pub mod devices;
pub mod diagnostics;
#[cfg(any(test, feature = "testing"))]
pub mod fake_udisks;
pub mod history;
pub mod logging;
pub mod menu;
pub mod metered;
pub mod mount;
pub mod notify;

pub use daemon::{Api, Config, Daemon};

/// O último componente do caminho, como texto.
pub(crate) fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Trava que não fica inutilizável se uma thread entrou em pânico com ela.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
pub use dbus::Service;

#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod devices_tests;
#[cfg(test)]
mod no_data_loss_tests;
#[cfg(test)]
mod testutil;
