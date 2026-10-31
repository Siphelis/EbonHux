use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Erreur>;

/// Code de sortie affiché dans un message ; `None` quand le processus n'en a pas rendu.
fn afficher_code(code: &Option<i32>) -> String {
    match code {
        Some(code) => code.to_string(),
        None => "aucun".to_string(),
    }
}

#[derive(Debug, Error)]
pub enum Erreur {
    #[error("Échec de l'action « {action} » sur « {} » : {source}", chemin.display())]
    Io {
        action: String,
        chemin: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("La commande « {programme} » a échoué (code de sortie : {}) : {sortie}", afficher_code(code))]
    Commande {
        programme: String,
        code: Option<i32>,
        sortie: String,
    },

    #[error("Élévation de droits impossible : {raison}")]
    Elevation { raison: String },

    #[error("Impossible d'analyser {objet} : {raison}")]
    Analyse { objet: String, raison: String },

    #[error("Échec réseau pour « {url} » : {raison}")]
    Reseau { url: String, raison: String },

    #[error("Chemin refusé « {} » : {raison}", chemin.display())]
    Chemin { chemin: PathBuf, raison: String },

    #[error("Trousseau de secrets inutilisable : {raison}")]
    Coffre { raison: String },

    #[error("Configuration invalide : {raison}")]
    Configuration { raison: String },

    #[error("Archive inutilisable : {raison}")]
    Archive { raison: String },
}

impl Erreur {
    pub fn io(action: &str, chemin: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            action: action.to_string(),
            chemin: chemin.into(),
            source,
        }
    }

    pub fn analyse(objet: &str, raison: impl std::fmt::Display) -> Self {
        Self::Analyse {
            objet: objet.to_string(),
            raison: raison.to_string(),
        }
    }

    pub fn configuration(raison: impl std::fmt::Display) -> Self {
        Self::Configuration {
            raison: raison.to_string(),
        }
    }
}
