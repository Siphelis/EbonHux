//! Commandes `langue` et `reverifier_langue` (docs/interface.md, décision 10) : lecture seule de
//! l'état de la version française. Le launcher ne modifie jamais le jeu : « protégé » veut dire
//! « présent », ou, pour la langue du jeu, « égale à `frFR` ».

use std::path::Path;

use ebonhux_coeur::lancement::{LectureWtf, lire_config_wtf};
use serde::Serialize;

use crate::comptes::Service;
use crate::erreur::{CODE_JEU_NON_CONFIGURE, ErreurService, Resultat};

pub const CODE_LANGUE: &str = "frFR";
pub const NOM_LANGUE: &str = "Français";
const ETAPE_LANGUE: &str = "langue";
const DOSSIER_DATA: &str = "Data";
const FICHIER_CORRECTIF: &str = "patch-Z.MPQ";

pub const ID_DONNEES: &str = "donnees";
pub const ID_CORRECTIF: &str = "correctif";
pub const ID_LOCALE: &str = "locale";
const TITRE_DONNEES: &str = "Données françaises";
const TITRE_CORRECTIF: &str = "Correctif français";
const TITRE_LOCALE: &str = "Langue du jeu";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EtatLangue {
    Protege,
    Incomplet,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElementLangue {
    pub id: &'static str,
    pub titre: &'static str,
    pub detail: String,
    pub present: bool,
    pub protege: bool,
    pub valeur: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseLangue {
    pub code: &'static str,
    pub nom: &'static str,
    pub etat: EtatLangue,
    pub elements: Vec<ElementLangue>,
}

impl EtatLangue {
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Protege => "protégée",
            Self::Incomplet => "incomplète",
            Self::Absent => "absente",
        }
    }
}

/// `protege` si tout est protégé, `absent` si rien n'est présent, `incomplet` sinon (calcul pur).
pub fn etat_global(elements: &[ElementLangue]) -> EtatLangue {
    if elements.iter().all(|e| e.protege) {
        EtatLangue::Protege
    } else if elements.iter().all(|e| !e.present) {
        EtatLangue::Absent
    } else {
        EtatLangue::Incomplet
    }
}

fn libelle_presence(chemin: &str, present: bool) -> String {
    format!("{chemin} : {}", if present { "présent" } else { "absent" })
}

/// Éléments à partir de ce que le disque contient (calcul pur).
pub fn elements_depuis(donnees_presentes: bool, correctif_present: bool, locale: Option<&str>) -> Vec<ElementLangue> {
    let chemin_donnees = format!("{DOSSIER_DATA}/{CODE_LANGUE}");
    let locale_ok = locale == Some(CODE_LANGUE);
    vec![
        ElementLangue {
            id: ID_DONNEES,
            titre: TITRE_DONNEES,
            detail: libelle_presence(&chemin_donnees, donnees_presentes),
            present: donnees_presentes,
            protege: donnees_presentes,
            valeur: None,
        },
        ElementLangue {
            id: ID_CORRECTIF,
            titre: TITRE_CORRECTIF,
            detail: libelle_presence(&format!("{DOSSIER_DATA}/{FICHIER_CORRECTIF}"), correctif_present),
            present: correctif_present,
            protege: correctif_present,
            valeur: None,
        },
        ElementLangue {
            id: ID_LOCALE,
            titre: TITRE_LOCALE,
            detail: match locale {
                Some(valeur) => format!("Langue réglée sur {valeur}"),
                None => "Langue non réglée dans Config.wtf".to_string(),
            },
            present: locale.is_some(),
            protege: locale_ok,
            valeur: locale.map(str::to_string),
        },
    ]
}

fn lire_locale(service: &Service, jeu: &Path) -> Resultat<Option<String>> {
    let lecture = service.lecture_silencieuse(|journal| lire_config_wtf(jeu, journal))?;
    Ok(match lecture {
        LectureWtf::Presente(config) => config.locale().map(str::to_string),
        LectureWtf::Absent => None,
    })
}

impl Service {
    fn verifier_langue(&self) -> Resultat<ReponseLangue> {
        let Some(jeu) = self.charger_config()?.dossier_jeu else {
            return Err(ErreurService::nouvelle(CODE_JEU_NON_CONFIGURE, "Le dossier du jeu n'est pas configuré : indiquez-le d'abord."));
        };
        let locale = lire_locale(self, &jeu)?;
        let donnees = jeu.join(DOSSIER_DATA).join(CODE_LANGUE).is_dir();
        let correctif = jeu.join(DOSSIER_DATA).join(FICHIER_CORRECTIF).is_file();
        let elements = elements_depuis(donnees, correctif, locale.as_deref());
        Ok(ReponseLangue { code: CODE_LANGUE, nom: NOM_LANGUE, etat: etat_global(&elements), elements })
    }

    /// Commande `langue`.
    pub fn langue(&self) -> Resultat<ReponseLangue> {
        self.verifier_langue()
    }

    /// Commande `reverifier_langue` : même lecture, mais l'action de l'utilisateur est journalisée.
    pub fn reverifier_langue(&self) -> Resultat<ReponseLangue> {
        let reponse = self.verifier_langue()?;
        self.journal.info(ETAPE_LANGUE, &format!("Langue revérifiée à la demande : {}", reponse.etat.libelle()))?;
        Ok(reponse)
    }
}
