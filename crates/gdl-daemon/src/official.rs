//! O Client ID do app no Google. No GeraSync ele vem embutido no build (o `build.rs` lê
//! `GERASYNC_CLIENT_ID` e `GERASYNC_CLIENT_SECRET`) e vale sempre; na variante de sempre, cada
//! usuário informa o seu.

use gdl_drive::token_store::ClientCredentials;

/// O Client ID e o segredo embutidos no build do GeraSync.
#[cfg(gerasync)]
pub fn credentials() -> ClientCredentials {
    ClientCredentials {
        client_id: env!("GDL_OFFICIAL_CLIENT_ID").into(),
        client_secret: env!("GDL_OFFICIAL_CLIENT_SECRET").into(),
    }
}

/// As credenciais que valem para entrar ou retomar uma conta: no GeraSync, sempre as embutidas
/// (um Client ID guardado ou informado é ignorado).
#[cfg(gerasync)]
pub fn effective(_given: ClientCredentials) -> ClientCredentials {
    credentials()
}

/// As credenciais que valem para entrar ou retomar uma conta: na variante de sempre, as dadas.
#[cfg(not(gerasync))]
pub fn effective(given: ClientCredentials) -> ClientCredentials {
    given
}

#[cfg(test)]
mod tests {
    use super::*;

    fn someone_elses() -> ClientCredentials {
        ClientCredentials {
            client_id: "outro.apps.googleusercontent.com".into(),
            client_secret: "outro".into(),
        }
    }

    /// Um login guardado com outro Client ID (copiado da variante de sempre, por exemplo) sobe
    /// com o oficial.
    #[cfg(gerasync)]
    #[test]
    fn resume_uses_official_credentials() {
        assert_eq!(effective(someone_elses()), credentials());
        assert_ne!(credentials(), someone_elses());
    }

    #[cfg(not(gerasync))]
    #[test]
    fn default_flavor_keeps_the_users_credentials() {
        assert_eq!(effective(someone_elses()), someone_elses());
    }
}
