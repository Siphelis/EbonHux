//! Réglages du jeu que plusieurs commandes exigent de la configuration.

use std::path::PathBuf;

use ebonhux_coeur::config::{Config, Royaume};
use ebonhux_coeur::erreur::{Erreur, Result};

use crate::commandes::config::{CLEF_DOSSIER_JEU, CLEF_REALMLIST, CLEF_ROYAUME_NOM};

/// Dossier du jeu, ou une erreur qui dit quelle clef définir.
pub fn dossier_jeu(config: &Config) -> Result<PathBuf> {
    config.dossier_jeu.clone().ok_or_else(|| {
        Erreur::configuration(format!(
            "le dossier du jeu n'est pas défini ; définissez-le avec « ebonhux config definir {CLEF_DOSSIER_JEU} <chemin absolu> »"
        ))
    })
}

/// Royaume de la configuration : facultatif, mais s'il est commencé il doit être complet.
pub fn royaume(config: &Config) -> Result<Option<&Royaume>> {
    let Some(royaume) = &config.royaume else {
        return Ok(None);
    };
    for (vide, clef) in [(royaume.realmlist.trim().is_empty(), CLEF_REALMLIST), (royaume.nom.trim().is_empty(), CLEF_ROYAUME_NOM)] {
        if vide {
            return Err(Erreur::configuration(format!(
                "le royaume est incomplet ; définissez-le avec « ebonhux config definir {clef} <valeur> »"
            )));
        }
    }
    Ok(Some(royaume))
}
