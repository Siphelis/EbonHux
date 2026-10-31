//! Dossier de lancement Linux : liens vers le jeu et copie de la DLL AwesomeWotlk.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::awesomewotlk::NOM_DLL;
use crate::erreur::{Erreur, Result};
use crate::journal::Journal;

const ETAPE: &str = "dossier_lancement";
const NOM_CACHE: &str = "Cache_AwesomeWotLK";
const NOM_DLL_TEMPORAIRE: &str = ".AwesomeWotlkLib.dll.tmp";

/// Ce que `synchroniser` a changé.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bilan {
    pub liens_crees: Vec<OsString>,
    pub liens_remplaces: Vec<OsString>,
    pub liens_retires: Vec<OsString>,
    pub dll_copiee: bool,
    pub cache_cree: bool,
    pub conflits: Vec<OsString>,
}

impl Bilan {
    /// Vrai si rien n'a changé.
    pub fn est_vide(&self) -> bool {
        *self == Self::default()
    }
}

/// Dossier de lancement par défaut : `<donnees>/ebonhux/lancement`.
pub fn dossier_par_defaut(donnees: &Path) -> PathBuf {
    donnees.join("ebonhux").join("lancement")
}

fn refus(chemin: &Path, raison: &str) -> Erreur {
    Erreur::Chemin { chemin: chemin.to_path_buf(), raison: raison.to_string() }
}

fn est_exclu(nom: &OsStr) -> bool {
    let minuscules = nom.to_string_lossy().to_lowercase();
    minuscules == NOM_DLL.to_lowercase() || minuscules == NOM_CACHE.to_lowercase()
}

/// État d'un chemin sans suivre les liens ; `None` s'il n'existe pas.
fn etat(chemin: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(chemin) {
        Ok(meta) => Ok(Some(meta)),
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Erreur::io("lire l'état de", chemin, source)),
    }
}

#[cfg(unix)]
fn lier(cible: &Path, lien: &Path) -> Result<()> {
    std::os::unix::fs::symlink(cible, lien).map_err(|source| Erreur::io("créer le lien", lien, source))
}

#[cfg(not(unix))]
fn lier(_cible: &Path, lien: &Path) -> Result<()> {
    Err(refus(lien, "les liens symboliques ne sont gérés que sous Linux"))
}

fn retirer_fichier(chemin: &Path) -> Result<()> {
    fs::remove_file(chemin).map_err(|source| Erreur::io("retirer le lien", chemin, source))
}

fn verifier_dossiers(dossier_jeu: &Path, dossier_lancement: &Path) -> Result<bool> {
    match etat(dossier_jeu)? {
        Some(meta) if meta.is_dir() => {}
        _ => return Err(refus(dossier_jeu, "le dossier du jeu doit être un vrai dossier")),
    }
    match etat(dossier_lancement)? {
        None => Ok(true),
        Some(meta) if meta.is_dir() => Ok(false),
        Some(_) => Err(refus(dossier_lancement, "le dossier de lancement doit être un vrai dossier, pas un lien ni un fichier")),
    }
}

fn noms_du_jeu(dossier_jeu: &Path) -> Result<BTreeSet<OsString>> {
    let mut noms = BTreeSet::new();
    let lecture = fs::read_dir(dossier_jeu).map_err(|source| Erreur::io("lister", dossier_jeu, source))?;
    for entree in lecture {
        let entree = entree.map_err(|source| Erreur::io("lister", dossier_jeu, source))?;
        noms.insert(entree.file_name());
    }
    Ok(noms)
}

fn lier_entrees(jeu: &Path, lancement: &Path, noms: &BTreeSet<OsString>, journal: &Journal, bilan: &mut Bilan) -> Result<()> {
    for nom in noms.iter().filter(|nom| !est_exclu(nom)) {
        let cible = jeu.join(nom);
        let lien = lancement.join(nom);
        match etat(&lien)? {
            None => {
                lier(&cible, &lien)?;
                bilan.liens_crees.push(nom.clone());
            }
            Some(meta) if meta.file_type().is_symlink() => {
                let actuelle = fs::read_link(&lien).map_err(|source| Erreur::io("lire le lien", &lien, source))?;
                if actuelle != cible {
                    retirer_fichier(&lien)?;
                    lier(&cible, &lien)?;
                    bilan.liens_remplaces.push(nom.clone());
                }
            }
            Some(_) => {
                journal.avertir(
                    ETAPE,
                    &format!("« {} » existe déjà dans le dossier de lancement et n'est pas un lien : laissé tel quel", nom.to_string_lossy()),
                )?;
                bilan.conflits.push(nom.clone());
            }
        }
    }
    Ok(())
}

fn retirer_liens_perimes(lancement: &Path, noms: &BTreeSet<OsString>, bilan: &mut Bilan) -> Result<()> {
    let mut perimes = BTreeSet::new();
    let lecture = fs::read_dir(lancement).map_err(|source| Erreur::io("lister", lancement, source))?;
    for entree in lecture {
        let entree = entree.map_err(|source| Erreur::io("lister", lancement, source))?;
        let nom = entree.file_name();
        if est_exclu(&nom) || noms.contains(&nom) {
            continue;
        }
        let meta = fs::symlink_metadata(entree.path()).map_err(|source| Erreur::io("lire l'état de", entree.path(), source))?;
        if meta.file_type().is_symlink() {
            perimes.insert(nom);
        }
    }
    for nom in perimes {
        retirer_fichier(&lancement.join(&nom))?;
        bilan.liens_retires.push(nom);
    }
    Ok(())
}

fn lire_dll(chemin: &Path) -> Result<Vec<u8>> {
    fs::read(chemin).map_err(|source| Erreur::io("lire", chemin, source))
}

fn copier_dll(source: &Path, lancement: &Path, bilan: &mut Bilan) -> Result<()> {
    let destination = lancement.join(NOM_DLL);
    let a_jour = match etat(&destination)? {
        None => false,
        Some(meta) if meta.file_type().is_symlink() => false,
        Some(meta) if meta.is_file() => lire_dll(&destination)? == lire_dll(source)?,
        Some(_) => return Err(refus(&destination, "la DLL du dossier de lancement doit être un fichier")),
    };
    if a_jour {
        return Ok(());
    }
    let temporaire = lancement.join(NOM_DLL_TEMPORAIRE);
    if etat(&temporaire)?.is_some() {
        retirer_fichier(&temporaire)?;
    }
    fs::copy(source, &temporaire).map_err(|erreur| Erreur::io("copier la DLL vers", &temporaire, erreur))?;
    fs::rename(&temporaire, &destination).map_err(|erreur| Erreur::io("installer la DLL", &destination, erreur))?;
    bilan.dll_copiee = true;
    Ok(())
}

fn preparer_cache(lancement: &Path, bilan: &mut Bilan) -> Result<()> {
    let cache = lancement.join(NOM_CACHE);
    match etat(&cache)? {
        Some(meta) if meta.is_dir() => return Ok(()),
        Some(meta) if meta.file_type().is_symlink() => retirer_fichier(&cache)?,
        Some(_) => return Err(refus(&cache, "le cache du dossier de lancement doit être un dossier")),
        None => {}
    }
    fs::create_dir(&cache).map_err(|source| Erreur::io("créer le dossier", &cache, source))?;
    bilan.cache_cree = true;
    Ok(())
}

/// Met le dossier de lancement en accord avec le jeu, sans rien écrire dans le jeu.
pub fn synchroniser(dossier_jeu: &Path, dossier_lancement: &Path, dll_awesomewotlk: &Path, journal: &Journal) -> Result<Bilan> {
    let resultat = synchroniser_sans_journal(dossier_jeu, dossier_lancement, dll_awesomewotlk, journal);
    if let Err(erreur) = &resultat {
        journal.journaliser_erreur(ETAPE, erreur)?;
    }
    resultat
}

fn synchroniser_sans_journal(dossier_jeu: &Path, dossier_lancement: &Path, dll_awesomewotlk: &Path, journal: &Journal) -> Result<Bilan> {
    let jeu = std::path::absolute(dossier_jeu).map_err(|source| Erreur::io("résoudre", dossier_jeu, source))?;
    let a_creer = verifier_dossiers(&jeu, dossier_lancement)?;
    let meta_dll = fs::metadata(dll_awesomewotlk).map_err(|source| Erreur::io("lire la DLL", dll_awesomewotlk, source))?;
    if !meta_dll.is_file() {
        return Err(refus(dll_awesomewotlk, "la DLL AwesomeWotlk fournie doit être un fichier"));
    }
    if a_creer {
        fs::create_dir_all(dossier_lancement).map_err(|source| Erreur::io("créer le dossier", dossier_lancement, source))?;
    }
    let noms = noms_du_jeu(&jeu)?;
    let mut bilan = Bilan::default();
    lier_entrees(&jeu, dossier_lancement, &noms, journal, &mut bilan)?;
    retirer_liens_perimes(dossier_lancement, &noms, &mut bilan)?;
    copier_dll(dll_awesomewotlk, dossier_lancement, &mut bilan)?;
    preparer_cache(dossier_lancement, &mut bilan)?;
    if !bilan.est_vide() {
        journal.info(
            ETAPE,
            &format!(
                "dossier de lancement à jour : {} liens créés, {} remplacés, {} retirés, DLL copiée : {}, conflits : {}",
                bilan.liens_crees.len(),
                bilan.liens_remplaces.len(),
                bilan.liens_retires.len(),
                if bilan.dll_copiee { "oui" } else { "non" },
                bilan.conflits.len()
            ),
        )?;
    }
    Ok(bilan)
}
