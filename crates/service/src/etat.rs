//! Données non secrètes du service : `<config>/ebonhux/etat-interface.json` (docs/interface.md,
//! section 2). Absent = valeurs par défaut ; malformé = erreur claire, jamais ignorée. Aucun mot
//! de passe n'y figure.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use ebonhux_coeur::comptes::cle_nom_compte;
use ebonhux_coeur::config::Dossiers;
use ebonhux_coeur::erreur::Erreur;
use serde::{Deserialize, Serialize};

use crate::erreur::{CODE_ETAT_INVALIDE, ErreurService, Resultat};

pub const NOM_FICHIER_ETAT: &str = "etat-interface.json";
const SUFFIXE_TEMPORAIRE: &str = "tmp";
const MODE_FICHIER: u32 = 0o600;

fn vrai() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EtatCompteFichier {
    #[serde(default = "vrai")]
    pub enregistrer_mot_de_passe: bool,
    #[serde(default)]
    pub derniere_connexion: Option<String>,
}

impl Default for EtatCompteFichier {
    fn default() -> Self {
        Self { enregistrer_mot_de_passe: true, derniere_connexion: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfoAddonInstalle {
    /// Casse réelle du dossier installé.
    pub dossier: String,
    /// Adresse de l'entrée de catalogue qui a servi à l'installation.
    #[serde(default)]
    pub addon_url: Option<String>,
    pub installe_le: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EtatInterface {
    pub comptes: BTreeMap<String, EtatCompteFichier>,
    pub addons_suivis: Vec<String>,
    /// Clé = nom du dossier en minuscules.
    #[serde(deserialize_with = "deserialiser_addons_installes")]
    pub addons_installes: BTreeMap<String, InfoAddonInstalle>,
    /// Compte pré-sélectionné pour « Tout lancer » (clé `cle_nom_compte`). Absent = le premier compte de la liste.
    pub compte_par_defaut: Option<String>,
}

/// Avant ce champ, `addons_installes` était une simple liste (jamais remplie en pratique, aucune
/// installation ne passait encore par le catalogue) : un fichier d'état écrit par une version
/// antérieure la porte encore comme un tableau JSON. Le lire comme une liste vide plutôt que de
/// rejeter tout le fichier (il ne contient aucune information récupérable dans l'ancienne forme).
fn deserialiser_addons_installes<'de, D>(deserialiseur: D) -> Result<BTreeMap<String, InfoAddonInstalle>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let valeur = serde_json::Value::deserialize(deserialiseur)?;
    if valeur.is_array() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_value(valeur).map_err(serde::de::Error::custom)
}

impl EtatInterface {
    /// État d'un compte (valeurs par défaut s'il n'est pas encore connu), clé sans tenir compte de la casse.
    pub fn compte(&self, nom: &str) -> EtatCompteFichier {
        self.comptes.get(&cle_nom_compte(nom)).cloned().unwrap_or_default()
    }

    pub fn compte_mut(&mut self, nom: &str) -> &mut EtatCompteFichier {
        self.comptes.entry(cle_nom_compte(nom)).or_default()
    }

    /// Clés en minuscules ; deux clés qui ne diffèrent que par la casse sont une erreur (jamais fusionnées en silence).
    fn normaliser(self, chemin: &std::path::Path) -> Resultat<Self> {
        let mut comptes = BTreeMap::new();
        for (nom, etat) in self.comptes {
            let cle = cle_nom_compte(&nom);
            if comptes.insert(cle, etat).is_some() {
                return Err(ErreurService::nouvelle(
                    CODE_ETAT_INVALIDE,
                    format!(
                        "Fichier d'état « {} » invalide : le compte « {nom} » apparaît deux fois (la casse ne compte pas).",
                        chemin.display()
                    ),
                ));
            }
        }
        // Clé normalisée comme celles de `comptes` : un fichier écrit à la main avec une autre casse reste lisible.
        let compte_par_defaut = self.compte_par_defaut.as_deref().map(cle_nom_compte);
        Ok(Self { comptes, compte_par_defaut, ..self })
    }
}

pub fn chemin_etat(dossiers: &Dossiers) -> PathBuf {
    dossiers.config_application().join(NOM_FICHIER_ETAT)
}

pub fn lire(dossiers: &Dossiers) -> Resultat<EtatInterface> {
    let chemin = chemin_etat(dossiers);
    let texte = match fs::read_to_string(&chemin) {
        Ok(texte) => texte,
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => return Ok(EtatInterface::default()),
        Err(erreur) => return Err(Erreur::io("lire le fichier d'état de l'interface", &chemin, erreur).into()),
    };
    let etat: EtatInterface = serde_json::from_str(&texte).map_err(|erreur| {
        ErreurService::nouvelle(
            CODE_ETAT_INVALIDE,
            format!(
                "Fichier d'état « {} » invalide (ligne {}, colonne {}) : {erreur}",
                chemin.display(),
                erreur.line(),
                erreur.column()
            ),
        )
    })?;
    etat.normaliser(&chemin)
}

/// Écriture atomique : fichier temporaire voisin (droits 0600), synchronisé, puis renommage.
pub fn ecrire(dossiers: &Dossiers, etat: &EtatInterface) -> Resultat<()> {
    let chemin = chemin_etat(dossiers);
    let dossier = dossiers.config_application();
    fs::create_dir_all(&dossier).map_err(|e| Erreur::io("créer le dossier de configuration", &dossier, e))?;
    let normalise = etat.clone().normaliser(&chemin)?;
    let texte = serde_json::to_string_pretty(&normalise).map_err(|erreur| {
        ErreurService::nouvelle(CODE_ETAT_INVALIDE, format!("Sérialisation du fichier d'état impossible : {erreur}"))
    })?;
    let temporaire = chemin.with_extension(format!("json.{SUFFIXE_TEMPORAIRE}"));
    let mut fichier = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(MODE_FICHIER)
        .open(&temporaire)
        .map_err(|e| Erreur::io("créer le fichier d'état temporaire", &temporaire, e))?;
    // `mode` ne joue qu'à la création : un temporaire resté d'un échec précédent est resserré.
    fichier
        .set_permissions(fs::Permissions::from_mode(MODE_FICHIER))
        .map_err(|e| Erreur::io("restreindre les droits du fichier d'état", &temporaire, e))?;
    fichier
        .write_all(texte.as_bytes())
        .map_err(|e| Erreur::io("écrire le fichier d'état temporaire", &temporaire, e))?;
    fichier
        .sync_all()
        .map_err(|e| Erreur::io("synchroniser le fichier d'état temporaire", &temporaire, e))?;
    fs::rename(&temporaire, &chemin).map_err(|e| Erreur::io("remplacer le fichier d'état", &chemin, e))?;
    Ok(())
}
