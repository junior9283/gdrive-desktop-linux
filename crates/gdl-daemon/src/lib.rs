//! Serviço do usuário: login, montagem da conta, interface D-Bus e notificações.

pub mod daemon;
pub mod dbus;
pub mod mount;
pub mod notify;

pub use daemon::{Api, Config, Daemon};
pub use dbus::{BUS_NAME, OBJECT_PATH, Service, ServiceError, ServiceProxy};

#[cfg(test)]
mod dbus_tests;
#[cfg(test)]
mod testutil;
