use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::erreur::{Erreur, Result};

pub const NOM_APPLICATION: &str = "ebonhux";
pub const NOM_FICHIER_CONFIG: &str = "config.toml";
pub const NOM_DOSSIER_JOURNAL: &str = "journal";
pub const NOM_DOSSIER_PREFIXE: &str = "prefixe";
const SUFFIXE_TEMPORAIRE: &str = "tmp";
pub const PREFIXE_SITE: &str = "https://";

const VARIABLE_MAISON: &str = "HOME";
const VARIABLE_CONFIG: &str = "XDG_CONFIG_HOME";
const VARIABLE_DONNEES: &str = "XDG_DATA_HOME";
const VARIABLE_CACHE: &str = "XDG_CACHE_HOME";
const DEFAUT_CONFIG: &str = ".config";
const DEFAUT_DONNEES: &str = ".local/share";
const DEFAUT_CACHE: &str = ".cache";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dossiers {
    pub maison: PathBuf,
    pub config: PathBuf,
    pub donnees: PathBuf,
    pub cache: PathBuf,
}

impl Dossiers {
    pub fn depuis_environnement() -> Result<Self> {
        Self::depuis_valeurs(
            std::env::var_os(VARIABLE_MAISON),
            std::env::var_os(VARIABLE_CONFIG),
            std::env::var_os(VARIABLE_DONNEES),
            std::env::var_os(VARIABLE_CACHE),
        )
    }

    /// Fonction pure derrière `depuis_environnement` ; une variable XDG vide ou relative est
    /// ignorée, comme le demande la spécification XDG.
    pub fn depuis_valeurs(
        maison: Option<OsString>,
        config: Option<OsString>,
        donnees: Option<OsString>,
        cache: Option<OsString>,
    ) -> Result<Self> {
        let maison = match maison.map(PathBuf::from) {
            Some(chemin) if chemin.is_absolute() => chemin,
            Some(chemin) => {
                return Err(Erreur::Chemin {
                    chemin,
                    raison: format!("la variable {VARIABLE_MAISON} doit être un chemin absolu"),
                });
            }
            None => {
                return Err(Erreur::configuration(format!(
                    "la variable {VARIABLE_MAISON} n'est pas définie : dossier personnel inconnu"
                )));
            }
        };
        let choisir = |valeur: Option<OsString>, defaut: &str| -> PathBuf {
            match valeur.map(PathBuf::from) {
                Some(chemin) if chemin.is_absolute() => chemin,
                _ => maison.join(defaut),
            }
        };
        Ok(Self {
            config: choisir(config, DEFAUT_CONFIG),
            donnees: choisir(donnees, DEFAUT_DONNEES),
            cache: choisir(cache, DEFAUT_CACHE),
            maison,
        })
    }

    /// Dossiers d'un arbre factice : tout sous `racine`, selon les défauts XDG.
    pub fn avec_racine(racine: impl Into<PathBuf>) -> Self {
        let maison = racine.into();
        Self {
            config: maison.join(DEFAUT_CONFIG),
            donnees: maison.join(DEFAUT_DONNEES),
            cache: maison.join(DEFAUT_CACHE),
            maison,
        }
    }

    pub fn config_application(&self) -> PathBuf {
        self.config.join(NOM_APPLICATION)
    }

    pub fn donnees_application(&self) -> PathBuf {
        self.donnees.join(NOM_APPLICATION)
    }

    pub fn fichier_config(&self) -> PathBuf {
        self.config_application().join(NOM_FICHIER_CONFIG)
    }

    pub fn dossier_journal(&self) -> PathBuf {
        self.donnees_application().join(NOM_DOSSIER_JOURNAL)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Royaume {
    pub realmlist: String,
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub dossier_jeu: Option<PathBuf>,
    pub dossier_prefixe: Option<PathBuf>,
    pub comptes: Vec<String>,
    pub royaume: Option<Royaume>,
    /// Adresse https du site du projet (globe de l'interface).
    pub site: Option<String>,
}

/// Adresse du site du projet posée par défaut (globe de l'interface).
pub const SITE_PAR_DEFAUT: &str = "https://project-ebonhold.com/";

impl Default for Config {
    fn default() -> Self {
        Self {
            dossier_jeu: None,
            dossier_prefixe: None,
            comptes: Vec::new(),
            royaume: None,
            site: Some(SITE_PAR_DEFAUT.to_string()),
        }
    }
}

fn position(texte: &str, decalage: usize) -> (usize, usize) {
    let avant = &texte[..decalage.min(texte.len())];
    let ligne = avant.matches('\n').count() + 1;
    let colonne = avant.rsplit('\n').next().map_or(0, |fin| fin.chars().count()) + 1;
    (ligne, colonne)
}

impl Config {
    pub fn charger(dossiers: &Dossiers) -> Result<Self> {
        let fichier = dossiers.fichier_config();
        let texte = match fs::read_to_string(&fichier) {
            Ok(texte) => texte,
            Err(erreur) if erreur.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(erreur) => return Err(Erreur::io("lire la configuration", fichier, erreur)),
        };
        let config: Self = toml::from_str(&texte).map_err(|erreur| {
            let lieu = erreur.span().map_or_else(String::new, |etendue| {
                let (ligne, colonne) = position(&texte, etendue.start);
                format!(", ligne {ligne}, colonne {colonne}")
            });
            Erreur::configuration(format!(
                "fichier « {} »{lieu} : {}",
                fichier.display(),
                erreur.message()
            ))
        })?;
        config.valider()?;
        Ok(config)
    }

    /// Écriture atomique : fichier temporaire voisin, puis renommage. Un échec laisse l'ancien
    /// fichier intact ; le temporaire éventuel est réécrit à l'enregistrement suivant.
    pub fn enregistrer(&self, dossiers: &Dossiers) -> Result<()> {
        self.valider()?;
        let texte = toml::to_string_pretty(self).map_err(|erreur| {
            Erreur::configuration(format!("sérialisation impossible : {erreur}"))
        })?;
        let fichier = dossiers.fichier_config();
        let dossier = dossiers.config_application();
        creer_dossier_prive(&dossier, "créer le dossier de configuration")?;
        let temporaire = fichier.with_extension(format!("toml.{SUFFIXE_TEMPORAIRE}"));
        ecrire_et_synchroniser(&temporaire, texte.as_bytes())?;
        fs::rename(&temporaire, &fichier)
            .map_err(|erreur| Erreur::io("remplacer la configuration", &fichier, erreur))
    }

    pub fn dossier_prefixe_effectif(&self, dossiers: &Dossiers) -> PathBuf {
        match &self.dossier_prefixe {
            Some(chemin) => chemin.clone(),
            None => dossiers
                .donnees_application()
                .join(NOM_DOSSIER_PREFIXE),
        }
    }

    pub fn valider(&self) -> Result<()> {
        if let Some(site) = &self.site {
            valider_site(site)?;
        }
        let mut vus: Vec<String> = Vec::new();
        for compte in &self.comptes {
            if compte.trim().is_empty() {
                return Err(Erreur::configuration("un nom de compte est vide"));
            }
            if compte.chars().any(char::is_control) {
                return Err(Erreur::configuration(format!(
                    "le nom de compte « {} » contient un caractère de contrôle",
                    compte.escape_debug()
                )));
            }
            let normalise = compte.to_lowercase();
            if vus.contains(&normalise) {
                return Err(Erreur::configuration(format!(
                    "le compte « {compte} » est en double (la casse ne compte pas)"
                )));
            }
            vus.push(normalise);
        }
        Ok(())
    }
}

/// Adresse du site : `https://` seulement, un hôte, ni espace ni caractère de contrôle.
pub fn valider_site(site: &str) -> Result<()> {
    let hote = site.strip_prefix(PREFIXE_SITE).unwrap_or_default();
    if hote.is_empty() || hote.starts_with('/') {
        return Err(Erreur::configuration(format!(
            "l'adresse du site « {} » doit commencer par {PREFIXE_SITE} et contenir un hôte",
            site.escape_debug()
        )));
    }
    if site.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(Erreur::configuration(format!(
            "l'adresse du site « {} » ne doit contenir ni espace ni caractère de contrôle",
            site.escape_debug()
        )));
    }
    Ok(())
}

const MODE_FICHIER_PRIVE: u32 = 0o600;
const MODE_DOSSIER_PRIVE: u32 = 0o700;

/// Crée le dossier (et ses parents manquants) en 0700 ; un dossier existant plus ouvert est resserré,
/// car `mode` ne joue qu'à la création. Toute erreur est rendue, jamais avalée.
pub(crate) fn creer_dossier_prive(dossier: &Path, action: &str) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(MODE_DOSSIER_PRIVE)
        .create(dossier)
        .map_err(|erreur| Erreur::io(action, dossier, erreur))?;
    fs::set_permissions(dossier, fs::Permissions::from_mode(MODE_DOSSIER_PRIVE))
        .map_err(|erreur| Erreur::io("restreindre les droits du dossier", dossier, erreur))
}

/// Ouvre (ou crée) le fichier en 0600 ; un fichier existant plus ouvert est resserré.
pub(crate) fn ouvrir_fichier_prive(chemin: &Path, options: &mut OpenOptions, action: &str) -> Result<File> {
    let fichier = options
        .mode(MODE_FICHIER_PRIVE)
        .open(chemin)
        .map_err(|erreur| Erreur::io(action, chemin, erreur))?;
    fichier
        .set_permissions(fs::Permissions::from_mode(MODE_FICHIER_PRIVE))
        .map_err(|erreur| Erreur::io("restreindre les droits du fichier", chemin, erreur))?;
    Ok(fichier)
}

fn ecrire_et_synchroniser(chemin: &Path, contenu: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut fichier = ouvrir_fichier_prive(chemin, &mut options, "créer le fichier temporaire")?;
    fichier
        .write_all(contenu)
        .map_err(|erreur| Erreur::io("écrire le fichier temporaire", chemin, erreur))?;
    fichier
        .sync_all()
        .map_err(|erreur| Erreur::io("synchroniser le fichier temporaire", chemin, erreur))
}
