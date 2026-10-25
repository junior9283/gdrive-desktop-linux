//! Serviço do usuário: login, montagem da conta, interface D-Bus e notificações.

pub mod daemon;
pub mod dbus;
pub mod mount;
pub mod notify;

pub use daemon::{Api, Config, Daemon};
pub use dbus::Service;

#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod no_data_loss_tests;
#[cfg(test)]
mod testutil;
