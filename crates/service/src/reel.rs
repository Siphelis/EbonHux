//! Assemblage du service avec les ports réels (machine du propriétaire). Aucun essai automatique
//! ne le construit avec le vrai trousseau : voir `docs/essais-reels.md` (module `ebonhux_hote_reel`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ebonhux_coeur::coffre::{CoffreSecretService, CoffreSecrets};
use ebonhux_coeur::config::Dossiers;
use ebonhux_coeur::erreur::Result as ResultatCoeur;
use ebonhux_coeur::executeur::ExecuteurReel;
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::lancement::{LanceurReel, PROGRAMME_WINE_DEFAUT};
use ebonhux_coeur::reseau::TelechargeurReel;
use ebonhux_coeur::secret::Secret;

use crate::comptes::Service;
use crate::erreur::Resultat;
use crate::sessions::{HorlogeReelle, SondeProc};
use crate::site::OuvreurReel;

const RACINE_SYSTEME: &str = "/";

/// Trousseau du système ouvert au premier besoin : le launcher démarre même sans service de
/// secrets, `comptes` rapporte alors `trousseau_disponible = false` au lieu de ne pas s'ouvrir.
#[derive(Debug, Default, Clone, Copy)]
pub struct CoffreSysteme;

impl CoffreSecrets for CoffreSysteme {
    fn disponible(&self) -> ResultatCoeur<()> {
        CoffreSecretService::new().map(|_| ())
    }

    fn enregistrer(&self, compte: &str, secret: &Secret) -> ResultatCoeur<()> {
        CoffreSecretService::new()?.enregistrer(compte, secret)
    }

    fn lire(&self, compte: &str) -> ResultatCoeur<Option<Secret>> {
        CoffreSecretService::new()?.lire(compte)
    }

    fn supprimer(&self, compte: &str) -> ResultatCoeur<()> {
        CoffreSecretService::new()?.supprimer(compte)
    }
}

/// Variables d'environnement visibles des sondes (fonction pure sur une liste de paires).
pub fn environnement_depuis(variables: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> BTreeMap<String, String> {
    variables
        .into_iter()
        .map(|(nom, valeur)| (nom.to_string_lossy().into_owned(), valeur.to_string_lossy().into_owned()))
        .collect()
}

impl Service {
    /// Service branché sur la machine : trousseau du système, lancement réel (Wine du cœur), horloge,
    /// `/proc`, commandes du système, téléchargement https, ouverture d'adresse, journal en fichier.
    pub fn reel() -> Resultat<Self> {
        Ok(Self::assembler(Dossiers::depuis_environnement()?, environnement_depuis(std::env::vars_os())))
    }

    /// Assemblage pur des ports réels pour des dossiers donnés (aucun accès au trousseau ici).
    pub fn assembler(dossiers: Dossiers, environnement: BTreeMap<String, String>) -> Self {
        let journal = Arc::new(Journal::vers_fichier(&dossiers));
        Self::new(
            Arc::new(CoffreSysteme),
            Arc::new(LanceurReel::pour_production(&dossiers, Arc::clone(&journal))),
            Arc::new(HorlogeReelle),
            Arc::new(SondeProc::new(RACINE_SYSTEME)),
            dossiers,
            journal,
        )
        .avec_executeur(Arc::new(ExecuteurReel))
        .avec_telechargeur(Arc::new(TelechargeurReel::new()))
        .avec_ouvreur(Arc::new(OuvreurReel))
        .avec_systeme(PathBuf::from(RACINE_SYSTEME), environnement)
        .avec_programme_wine(PROGRAMME_WINE_DEFAUT)
    }
}
