//! Erreur du service : `{ code, message }` du contrat. Le code est stable (ASCII) et sert à
//! l'interface ; le message est en français et s'affiche tel quel dans le bandeau d'erreur.

use std::fmt;

use ebonhux_coeur::erreur::Erreur;
use serde::Serialize;

pub const CODE_COMPTE_INCONNU: &str = "compte_inconnu";
pub const CODE_COMPTE_EXISTANT: &str = "compte_existant";
pub const CODE_NOM_INVALIDE: &str = "nom_invalide";
pub const CODE_DEJA_EN_COURS: &str = "deja_en_cours";
pub const CODE_MOT_DE_PASSE_REQUIS: &str = "mot_de_passe_requis";
pub const CODE_ENREGISTREMENT_DESACTIVE: &str = "enregistrement_desactive";
pub const CODE_TROUSSEAU_INDISPONIBLE: &str = "trousseau_indisponible";
pub const CODE_JEU_NON_CONFIGURE: &str = "jeu_non_configure";
pub const CODE_AUCUN_COMPTE: &str = "aucun_compte";
pub const CODE_CONFIGURATION: &str = "configuration_invalide";
pub const CODE_ETAT_INVALIDE: &str = "etat_interface_invalide";
pub const CODE_FICHIER: &str = "fichier_inaccessible";
pub const CODE_CHEMIN_REFUSE: &str = "chemin_refuse";
pub const CODE_LANCEMENT_ECHOUE: &str = "lancement_impossible";
pub const CODE_ENREGISTREMENT_ECHOUE: &str = "enregistrement_mot_de_passe_echoue";
pub const CODE_DONNEES_INVALIDES: &str = "donnees_invalides";
pub const CODE_ELEVATION: &str = "elevation_impossible";
pub const CODE_RESEAU: &str = "reseau_indisponible";
pub const CODE_ARCHIVE: &str = "archive_inutilisable";
pub const CODE_ADDON_INCONNU: &str = "addon_inconnu";
pub const CODE_ENTREE_INCONNUE: &str = "entree_catalogue_inconnue";
pub const CODE_SITE_NON_CONFIGURE: &str = "site_non_configure";
pub const CODE_PORT_ABSENT: &str = "port_indisponible";
pub const CODE_ARGUMENTS_INVALIDES: &str = "arguments_invalides";
pub const CODE_DOSSIER_JEU_INVALIDE: &str = "dossier_jeu_invalide";
pub const CODE_DISTRIBUTION_NON_PRISE_EN_CHARGE: &str = "distribution_non_prise_en_charge";
pub const CODE_PREPARATION_IMPOSSIBLE: &str = "preparation_impossible";
pub const CODE_COMMANDE_INCONNUE: &str = "commande_inconnue";

pub type Resultat<T> = std::result::Result<T, ErreurService>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurService {
    pub code: String,
    pub message: String,
}

impl ErreurService {
    pub fn nouvelle(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }
}

impl fmt::Display for ErreurService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ErreurService {}

impl From<Erreur> for ErreurService {
    fn from(erreur: Erreur) -> Self {
        let code = match &erreur {
            Erreur::Io { .. } => CODE_FICHIER,
            Erreur::Commande { .. } => CODE_LANCEMENT_ECHOUE,
            Erreur::Elevation { .. } => CODE_ELEVATION,
            Erreur::Analyse { .. } => CODE_DONNEES_INVALIDES,
            Erreur::Reseau { .. } => CODE_RESEAU,
            Erreur::Chemin { .. } => CODE_CHEMIN_REFUSE,
            Erreur::Coffre { .. } => CODE_TROUSSEAU_INDISPONIBLE,
            Erreur::Configuration { .. } => CODE_CONFIGURATION,
            Erreur::Archive { .. } => CODE_ARCHIVE,
        };
        Self::nouvelle(code, erreur.to_string())
    }
}
