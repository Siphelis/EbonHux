//! Coffre des mots de passe de comptes. Un secret n'existe que dans le trousseau du système
//! (ou en mémoire pour les essais) : jamais dans un fichier, un journal ou un message d'erreur.

use std::collections::HashMap;
use std::error::Error as ErreurStd;
use std::sync::{Mutex, MutexGuard};

use crate::comptes::cle_nom_compte;
use crate::erreur::{Erreur, Result};
use crate::secret::Secret;

/// Nom du service sous lequel les secrets sont rangés dans le trousseau.
pub const NOM_SERVICE: &str = "ebonhux";

pub trait CoffreSecrets {
    /// Vérifie que le magasin est utilisable ; sinon `Erreur::Coffre` qui dit pourquoi.
    fn disponible(&self) -> Result<()>;
    fn enregistrer(&self, compte: &str, secret: &Secret) -> Result<()>;
    fn lire(&self, compte: &str) -> Result<Option<Secret>>;
    /// Supprimer un secret absent réussit : le résultat voulu est atteint.
    fn supprimer(&self, compte: &str) -> Result<()>;
}

fn coffre_invalide(raison: String) -> Erreur {
    Erreur::Coffre { raison }
}

/// Nom de compte tel qu'il sert de clé : minuscules, nom non vide et sans caractère de contrôle.
pub fn normaliser_nom_compte(nom: &str) -> Result<String> {
    if nom.trim().is_empty() {
        return Err(coffre_invalide("le nom du compte est vide".to_string()));
    }
    if nom.chars().any(char::is_control) {
        return Err(coffre_invalide(format!(
            "le nom du compte {nom:?} contient un caractère de contrôle"
        )));
    }
    Ok(cle_nom_compte(nom))
}

/// Coffre en mémoire, pour les essais. Le `Debug` dérivé reste sûr : `Secret` se masque.
#[derive(Debug, Default)]
pub struct CoffreMemoire {
    secrets: Mutex<HashMap<String, Secret>>,
}

impl CoffreMemoire {
    pub fn new() -> Self {
        Self::default()
    }

    fn verrou(&self) -> Result<MutexGuard<'_, HashMap<String, Secret>>> {
        self.secrets
            .lock()
            .map_err(|_| coffre_invalide("le coffre en mémoire est corrompu (verrou empoisonné)".to_string()))
    }
}

impl CoffreSecrets for CoffreMemoire {
    fn disponible(&self) -> Result<()> {
        Ok(())
    }

    fn enregistrer(&self, compte: &str, secret: &Secret) -> Result<()> {
        let cle = normaliser_nom_compte(compte)?;
        self.verrou()?.insert(cle, Secret::new(secret.exposer().to_string()));
        Ok(())
    }

    fn lire(&self, compte: &str) -> Result<Option<Secret>> {
        let cle = normaliser_nom_compte(compte)?;
        Ok(self.verrou()?.get(&cle).map(|s| Secret::new(s.exposer().to_string())))
    }

    fn supprimer(&self, compte: &str) -> Result<()> {
        let cle = normaliser_nom_compte(compte)?;
        self.verrou()?.remove(&cle);
        Ok(())
    }
}

/// Trousseau du système (Secret Service sous Linux). Le magasin est initialisé une seule fois
/// par processus par la crate `keyring` : s'il a échoué, l'échec reste le même jusqu'à la fin
/// du processus, même si le service apparaît ensuite.
#[derive(Debug)]
pub struct CoffreSecretService {
    service: String,
}

impl CoffreSecretService {
    /// Vérifie l'état du magasin ; `Erreur::Coffre` s'il est inutilisable.
    pub fn new() -> Result<Self> {
        let coffre = Self { service: NOM_SERVICE.to_string() };
        coffre.disponible()?;
        Ok(coffre)
    }

    fn entree(&self, compte: &str, action: &str) -> Result<keyring::Entry> {
        let cle = normaliser_nom_compte(compte)?;
        keyring::Entry::new(&self.service, &cle).map_err(|e| traduire_erreur_keyring(&e, action))
    }
}

impl CoffreSecrets for CoffreSecretService {
    fn disponible(&self) -> Result<()> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(()),
            Err(e) => Err(traduire_erreur_keyring(e, "ouvrir le trousseau")),
        }
    }

    fn enregistrer(&self, compte: &str, secret: &Secret) -> Result<()> {
        let action = "enregistrer le secret";
        self.entree(compte, action)?
            .set_password(secret.exposer())
            .map_err(|e| traduire_erreur_keyring(&e, action))
    }

    fn lire(&self, compte: &str) -> Result<Option<Secret>> {
        let action = "lire le secret";
        match self.entree(compte, action)?.get_password() {
            Ok(clair) => Ok(Some(Secret::new(clair))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(traduire_erreur_keyring(&e, action)),
        }
    }

    fn supprimer(&self, compte: &str) -> Result<()> {
        let action = "supprimer le secret";
        match self.entree(compte, action)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(traduire_erreur_keyring(&e, action)),
        }
    }
}

/// Texte de l'erreur et de toutes ses causes, de la plus proche à la plus profonde.
fn texte_avec_causes(erreur: &(dyn ErreurStd + 'static)) -> String {
    let mut texte = erreur.to_string();
    let mut source = erreur.source();
    while let Some(cause) = source {
        let cause_texte = cause.to_string();
        if !texte.contains(&cause_texte) {
            texte.push_str(" : ");
            texte.push_str(&cause_texte);
        }
        source = cause.source();
    }
    texte
}

/// Marqueurs (en minuscules) d'un service de secrets absent dans les messages zbus : nom D-Bus
/// sans propriétaire, bus injoignable (observé : « Failed to connect to address … No such file »).
const MARQUEURS_SERVICE_ABSENT: [&str; 5] = [
    "serviceunknown",
    "namehasnoowner",
    "not provided by any .service",
    "failed to connect to address",
    "connection refused",
];

/// Traduit une erreur du magasin en `Erreur::Coffre` : `action` dit ce qui était tenté, la
/// raison dit ce qui s'est passé et rappelle la cause d'origine. Ne contient jamais de secret :
/// seuls les `Display` sont utilisés, jamais le contenu de `BadEncoding` ou `BadDataFormat`.
pub fn traduire_erreur_keyring(erreur: &keyring::Error, action: &str) -> Erreur {
    use keyring::Error as K;
    let cause = texte_avec_causes(erreur);
    let diagnostic = match erreur {
        K::NoStorageAccess(_) => {
            "le trousseau est verrouillé, ou l'accès a été refusé par l'utilisateur".to_string()
        }
        K::PlatformFailure(_) if service_absent(&cause) => {
            "le service de secrets du système (org.freedesktop.secrets) est absent ou injoignable"
                .to_string()
        }
        K::PlatformFailure(_) => "le service de secrets du système a échoué".to_string(),
        K::NoDefaultStore => {
            "le magasin de secrets du système n'a pas pu être initialisé (service de secrets absent ou plateforme non prise en charge)"
                .to_string()
        }
        K::NoEntry => "aucun secret n'est enregistré pour ce compte".to_string(),
        K::BadEncoding(_) => "le secret enregistré n'est pas un texte UTF-8 valide".to_string(),
        K::BadDataFormat(_, _) => "le secret enregistré est mal formé".to_string(),
        K::BadStoreFormat(_) => "le contenu du trousseau est mal formé".to_string(),
        K::TooLong(_, _) => "une valeur dépasse la limite du trousseau".to_string(),
        K::Invalid(_, _) => "le trousseau a refusé un paramètre".to_string(),
        K::Ambiguous(entrees) => {
            format!("{} secrets correspondent à ce compte dans le trousseau", entrees.len())
        }
        K::NotSupportedByStore(_) => "le trousseau ne prend pas en charge cette opération".to_string(),
        _ => "erreur inconnue du trousseau".to_string(),
    };
    let cause = match erreur {
        K::Ambiguous(_) => "plusieurs entrées".to_string(),
        _ => cause,
    };
    Erreur::Coffre { raison: format!("échec de l'action « {action} » : {diagnostic} (cause d'origine : {cause})") }
}

fn service_absent(texte: &str) -> bool {
    let bas = texte.to_lowercase();
    MARQUEURS_SERVICE_ABSENT.iter().any(|m| bas.contains(m))
}
