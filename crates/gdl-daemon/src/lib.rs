//! Serviço do usuário: login, montagem da conta, interface D-Bus e notificações.

pub mod daemon;
pub mod dbus;
pub mod desktop;
pub mod history;
pub mod menu;
pub mod metered;
pub mod mount;
pub mod notify;

pub use daemon::{Api, Config, Daemon};

/// Trava que não fica inutilizável se uma thread entrou em pânico com ela.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
pub use dbus::Service;

#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod no_data_loss_tests;
#[cfg(test)]
mod testutil;
