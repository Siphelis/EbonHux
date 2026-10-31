//! Coffre réel du binaire : le trousseau du système n'est ouvert qu'au premier besoin, pour que
//! `diag`, `config` ou `prefixe` ne dépendent pas de la présence d'un service de secrets.

use ebonhux_coeur::coffre::{CoffreSecretService, CoffreSecrets};
use ebonhux_coeur::erreur::Result;
use ebonhux_coeur::secret::Secret;

#[derive(Debug, Default)]
pub struct CoffreSysteme;

impl CoffreSecrets for CoffreSysteme {
    fn disponible(&self) -> Result<()> {
        CoffreSecretService::new().map(|_| ())
    }

    fn enregistrer(&self, compte: &str, secret: &Secret) -> Result<()> {
        CoffreSecretService::new()?.enregistrer(compte, secret)
    }

    fn lire(&self, compte: &str) -> Result<Option<Secret>> {
        CoffreSecretService::new()?.lire(compte)
    }

    fn supprimer(&self, compte: &str) -> Result<()> {
        CoffreSecretService::new()?.supprimer(compte)
    }
}
