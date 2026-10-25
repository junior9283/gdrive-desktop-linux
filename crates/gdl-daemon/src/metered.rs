//! Conexão limitada (`PauseSyncingOnMeteredConnection`): a propriedade `Metered` do
//! NetworkManager, no barramento do sistema.

pub const NETWORK_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    /// `NMMetered`: 0 desconhecida, 1 sim, 2 não, 3 provavelmente sim, 4 provavelmente não.
    #[zbus(property)]
    fn metered(&self) -> zbus::Result<u32>;
}

/// A conexão é (ou provavelmente é) limitada.
pub fn is_metered(value: u32) -> bool {
    matches!(value, 1 | 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes_and_guess_yes_are_metered() {
        let metered: Vec<u32> = (0..=4).filter(|&v| is_metered(v)).collect();
        assert_eq!(metered, [1, 3]);
    }
}
