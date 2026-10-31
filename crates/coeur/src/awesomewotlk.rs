//! DLL AwesomeWotlk v37 : téléchargement vérifié (zip puis DLL) et installation atomique.

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::dxvk::{ArchiveAttendue, creer_dossier_temporaire};
use crate::erreur::{Erreur, Result};
use crate::journal::Journal;
use crate::reseau::{Telechargeur, somme_sha256};

/// Release officielle `noname08662/awesome_wotlk`.
pub const VERSION_EPINGLEE: &str = "v37";
pub const URL_ARCHIVE_EPINGLEE: &str =
    "https://github.com/noname08662/awesome_wotlk/releases/download/v37/AwesomeWotLK_v37.zip";
/// SHA256 du zip (identique au champ `digest` de l'API GitHub des releases).
pub const SOMME_ARCHIVE_EPINGLEE: &str = "df049d0bdcefd24408f7e8ea407d42fefb4213bd71fefc76a4522cec6eb930a2";
pub const NOM_DLL: &str = "AwesomeWotlkLib.dll";
pub const SOMME_DLL_EPINGLEE: &str = "c8f06bf34a6dac15d614616ab0114b77fbca9ee2b5bd8ea70c9fe845adfba2fa";
pub const LIMITE_ARCHIVE_OCTETS: u64 = 8 * 1024 * 1024;
pub const LIMITE_DLL_OCTETS: u64 = 4 * 1024 * 1024;

const ETAPE: &str = "awesomewotlk";
const NOM_ARCHIVE_TEMPORAIRE: &str = "archive.zip";
const NOM_DLL_TEMPORAIRE: &str = "dll.tmp";

pub const ARCHIVE_EPINGLEE: ArchiveAttendue<'static> = ArchiveAttendue {
    url: URL_ARCHIVE_EPINGLEE,
    sha256: SOMME_ARCHIVE_EPINGLEE,
    version: VERSION_EPINGLEE,
};

/// DLL attendue : nom exact à la racine du zip, somme et taille maximale.
#[derive(Debug, Clone, Copy)]
pub struct DllAttendue<'a> {
    pub nom: &'a str,
    pub sha256: &'a str,
    pub limite_octets: u64,
}

pub const DLL_EPINGLEE: DllAttendue<'static> = DllAttendue {
    nom: NOM_DLL,
    sha256: SOMME_DLL_EPINGLEE,
    limite_octets: LIMITE_DLL_OCTETS,
};

fn refus(raison: impl std::fmt::Display) -> Erreur {
    Erreur::Archive { raison: raison.to_string() }
}

fn dossier_racine(donnees: &Path) -> PathBuf {
    donnees.join("ebonhux")
}

/// Chemin final de la DLL d'une version : `<donnees>/ebonhux/awesomewotlk/<version>/<nom>`.
pub fn chemin_dll(donnees: &Path, version: &str, dll: &DllAttendue<'_>) -> PathBuf {
    dossier_racine(donnees).join(ETAPE).join(version).join(dll.nom)
}

/// Sonde sans réseau : la DLL v37 est-elle déjà là avec la bonne somme ?
pub fn est_prete(donnees: &Path) -> Result<bool> {
    dll_prete(&chemin_dll(donnees, VERSION_EPINGLEE, &DLL_EPINGLEE), &DLL_EPINGLEE)
}

/// Présence et somme de la DLL à `chemin` ; absente ou somme fausse → `false`.
pub fn dll_prete(chemin: &Path, dll: &DllAttendue<'_>) -> Result<bool> {
    let metadonnees = match fs::symlink_metadata(chemin) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(Erreur::io("lire la DLL AwesomeWotlk", chemin, e)),
    };
    if !metadonnees.is_file() || metadonnees.len() > dll.limite_octets {
        return Ok(false);
    }
    let octets = fs::read(chemin).map_err(|e| Erreur::io("lire la DLL AwesomeWotlk", chemin, e))?;
    Ok(somme_sha256(&octets).eq_ignore_ascii_case(dll.sha256))
}

/// Chemin de la DLL v37 prête, téléchargée et vérifiée si elle manque ou est abîmée.
pub fn obtenir(donnees: &Path, telechargeur: &dyn Telechargeur, journal: &Journal) -> Result<PathBuf> {
    obtenir_verifiee(donnees, &ARCHIVE_EPINGLEE, &DLL_EPINGLEE, telechargeur, journal)
}

/// Variante qui prend l'archive et la DLL attendues en paramètres (essais : zips et sommes fabriqués).
pub fn obtenir_verifiee(
    donnees: &Path,
    archive: &ArchiveAttendue<'_>,
    dll: &DllAttendue<'_>,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<PathBuf> {
    let destination = chemin_dll(donnees, archive.version, dll);
    if dll_prete(&destination, dll)? {
        journal.info(ETAPE, &format!("AwesomeWotlk {} déjà prête, aucun téléchargement", archive.version))?;
        return Ok(destination);
    }
    if destination.exists() {
        journal.avertir(
            ETAPE,
            &format!("« {} » est abîmée ou différente de celle attendue : nouveau téléchargement", destination.display()),
        )?;
    }
    let racine = dossier_racine(donnees);
    fs::create_dir_all(&racine).map_err(|e| Erreur::io("créer le dossier d'EbonHux", &racine, e))?;
    let travail = creer_dossier_temporaire(&racine, ETAPE)?;
    let resultat = telecharger_dans(&travail, &destination, archive, dll, telechargeur, journal);
    match fs::remove_dir_all(&travail) {
        Ok(()) => resultat,
        Err(e) if e.kind() == ErrorKind::NotFound => resultat,
        Err(e) => match resultat {
            Ok(chemin) => {
                journal.avertir(ETAPE, &format!("dossier temporaire « {} » non retiré : {e}", travail.display()))?;
                Ok(chemin)
            }
            Err(erreur) => Err(refus(format!(
                "{erreur} ; en plus, le dossier temporaire « {} » n'a pas pu être retiré : {e}",
                travail.display()
            ))),
        },
    }
}

fn echouer<T>(journal: &Journal, erreur: Erreur) -> Result<T> {
    journal.journaliser_erreur(ETAPE, &erreur)?;
    Err(erreur)
}

fn telecharger_dans(
    travail: &Path,
    destination: &Path,
    archive: &ArchiveAttendue<'_>,
    dll: &DllAttendue<'_>,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<PathBuf> {
    let fichier = travail.join(NOM_ARCHIVE_TEMPORAIRE);
    journal.info(ETAPE, &format!("téléchargement d'AwesomeWotlk {} depuis {}", archive.version, archive.url))?;
    let bilan = match telechargeur.telecharger(archive.url, &fichier, LIMITE_ARCHIVE_OCTETS) {
        Ok(bilan) => bilan,
        Err(erreur) => return echouer(journal, erreur),
    };
    if !bilan.sha256.eq_ignore_ascii_case(archive.sha256) {
        return echouer(
            journal,
            Erreur::Reseau {
                url: archive.url.to_string(),
                raison: format!("somme SHA256 inattendue : attendue {}, obtenue {}", archive.sha256, bilan.sha256),
            },
        );
    }
    journal.info(ETAPE, &format!("somme SHA256 du zip vérifiée ({} octets)", bilan.octets))?;
    let octets = match extraire_dll(&fichier, dll) {
        Ok(octets) => octets,
        Err(erreur) => return echouer(journal, erreur),
    };
    if let Err(erreur) = installer(&octets, travail, destination) {
        return echouer(journal, erreur);
    }
    journal.info(ETAPE, &format!("{} installée dans « {} »", dll.nom, destination.display()))?;
    Ok(destination.to_path_buf())
}

/// Lit UNIQUEMENT l'entrée nommée exactement `dll.nom` à la racine du zip et vérifie sa somme.
fn extraire_dll(archive: &Path, dll: &DllAttendue<'_>) -> Result<Vec<u8>> {
    let fichier = File::open(archive).map_err(|e| Erreur::io("ouvrir le zip téléchargé", archive, e))?;
    let mut zip = ZipArchive::new(fichier).map_err(|e| refus(format!("zip illisible : {e}")))?;
    let mut trouvee: Option<usize> = None;
    for indice in 0..zip.len() {
        let entree = zip
            .by_index_raw(indice)
            .map_err(|e| refus(format!("entrée {indice} du zip illisible : {e}")))?;
        let nom = entree.name().to_string();
        let dernier = nom.rsplit(['/', '\\']).next().unwrap_or("");
        if nom == dll.nom {
            if entree.is_symlink() {
                return Err(refus(format!("« {nom} » est un lien dans le zip : refusé")));
            }
            trouvee = Some(indice);
        } else if dernier == dll.nom {
            return Err(refus(format!(
                "« {nom} » n'est pas à la racine du zip (dossier ou « .. ») : refusé"
            )));
        }
    }
    let indice = trouvee.ok_or_else(|| refus(format!("« {} » est absente du zip", dll.nom)))?;
    let entree = zip
        .by_index(indice)
        .map_err(|e| refus(format!("« {} » illisible dans le zip : {e}", dll.nom)))?;
    if entree.size() > dll.limite_octets {
        return Err(refus(format!(
            "« {} » dépasse la limite de {} octets ({} annoncés)",
            dll.nom,
            dll.limite_octets,
            entree.size()
        )));
    }
    let mut octets = Vec::new();
    entree
        .take(dll.limite_octets + 1)
        .read_to_end(&mut octets)
        .map_err(|e| refus(format!("« {} » tronquée ou corrompue : {e}", dll.nom)))?;
    if octets.len() as u64 > dll.limite_octets {
        return Err(refus(format!("« {} » dépasse la limite de {} octets", dll.nom, dll.limite_octets)));
    }
    let somme = somme_sha256(&octets);
    if !somme.eq_ignore_ascii_case(dll.sha256) {
        return Err(refus(format!(
            "somme SHA256 de « {} » inattendue : attendue {}, obtenue {somme}",
            dll.nom, dll.sha256
        )));
    }
    Ok(octets)
}

/// Écrit dans le dossier temporaire (même système de fichiers) puis renomme : jamais de DLL à moitié écrite.
fn installer(octets: &[u8], travail: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| Erreur::Chemin {
        chemin: destination.to_path_buf(),
        raison: "aucun dossier parent pour la destination".to_string(),
    })?;
    fs::create_dir_all(parent).map_err(|e| Erreur::io("créer le dossier de la DLL", parent, e))?;
    let temporaire = travail.join(NOM_DLL_TEMPORAIRE);
    fs::write(&temporaire, octets).map_err(|e| Erreur::io("écrire la DLL temporaire", &temporaire, e))?;
    fs::rename(&temporaire, destination).map_err(|e| Erreur::io("installer la DLL", destination, e))
}
