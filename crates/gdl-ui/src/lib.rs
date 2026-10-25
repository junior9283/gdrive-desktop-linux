//! A janela (`gdrive-desktop-linux-ui`): a ponte com o serviço, o processo único, as telas e os
//! textos. O Tauri fica no `main.rs`; o resto se testa sem servidor gráfico.

pub mod bridge;
pub mod screens;
pub mod single;
pub mod strings;
