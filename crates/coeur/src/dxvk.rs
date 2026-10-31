//! Source des DLL DXVK : runtime de Lutris s'il existe, sinon archive officielle épinglée.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use crate::erreur::{Erreur, Result};
use crate::journal::Journal;
use crate::reseau::Telechargeur;

/// Valeur par défaut du runtime de Lutris, testée sur la machine du propriétaire.
pub const VERSION_EPINGLEE: &str = "2.6.2";
/// Archive officielle de la release GitHub `doitsujin/dxvk` correspondant à `VERSION_EPINGLEE`.
pub const URL_ARCHIVE_EPINGLEE: &str =
    "https://github.com/doitsujin/dxvk/releases/download/v2.6.2/dxvk-2.6.2.tar.gz";
/// SHA256 de l'archive (identique au champ `digest` de l'API GitHub des releases).
pub const SOMME_ARCHIVE_EPINGLEE: &str =
    "17761876556afd55736cb895d184f5a1c55d43350f1b1e3b129f8d28706d7992";
/// Préfixe des dossiers de version du runtime de Lutris (« v2.6.2 »).
const PREFIXE_VERSION: char = 'v';
pub const DLL_REQUISES: [&str; 5] = ["d3d8", "d3d9", "d3d10core", "d3d11", "dxgi"];
pub const SOUS_DOSSIERS_ARCHITECTURE: [&str; 2] = ["x32", "x64"];
pub const LIMITE_ARCHIVE_OCTETS: u64 = 64 * 1024 * 1024;
pub const LIMITE_ENTREE_OCTETS: u64 = 32 * 1024 * 1024;

const ETAPE: &str = "dxvk";
const EXTENSION_DLL: &str = "dll";
const NOM_ARCHIVE_TEMPORAIRE: &str = "archive.tar.gz";
const NOM_EXTRAIT_TEMPORAIRE: &str = "extrait";
const NOM_ANCIEN_TEMPORAIRE: &str = "ancien";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origine {
    RuntimeLutris,
    Telechargee,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDxvk {
    /// Contient `x32/` et `x64/` avec toutes les DLL requises.
    pub dossier: PathBuf,
    pub version: String,
    pub origine: Origine,
}

/// Archive attendue : adresse, somme et version (qui fixe le préfixe `dxvk-<version>/`).
#[derive(Debug, Clone, Copy)]
pub struct ArchiveAttendue<'a> {
    pub url: &'a str,
    pub sha256: &'a str,
    pub version: &'a str,
}

pub const ARCHIVE_EPINGLEE: ArchiveAttendue<'static> = ArchiveAttendue {
    url: URL_ARCHIVE_EPINGLEE,
    sha256: SOMME_ARCHIVE_EPINGLEE,
    version: VERSION_EPINGLEE,
};

/// Version telle qu'affichée : sans le « v » du nom de dossier de Lutris (le chemin garde le nom réel).
fn normaliser_version(nom: &str) -> &str {
    nom.strip_prefix(PREFIXE_VERSION).unwrap_or(nom)
}

/// Version numérique comparable ; les zéros finaux sont ignorés (`2.6` = `2.6.0`).
/// Lutris nomme ses dossiers « v2.6.2 » : un seul « v » minuscule est toléré devant les chiffres.
fn analyser_version(nom: &str) -> Option<Vec<u64>> {
    let nom = normaliser_version(nom);
    let mut parties = nom
        .split('.')
        .map(|p| {
            if p.is_empty() || !p.bytes().all(|o| o.is_ascii_digit()) {
                None
            } else {
                p.parse::<u64>().ok()
            }
        })
        .collect::<Option<Vec<u64>>>()?;
    while parties.len() > 1 && parties.last() == Some(&0) {
        parties.pop();
    }
    Some(parties)
}

fn dll_manquantes(dossier: &Path) -> Vec<String> {
    let mut manquantes = Vec::new();
    for architecture in SOUS_DOSSIERS_ARCHITECTURE {
        for dll in DLL_REQUISES {
            let relatif = format!("{architecture}/{dll}.{EXTENSION_DLL}");
            if !dossier.join(&relatif).is_file() {
                manquantes.push(relatif);
            }
        }
    }
    manquantes
}

fn dossier_runtime(donnees: &Path) -> PathBuf {
    donnees.join("lutris").join("runtime").join("dxvk")
}

fn dossier_telecharge(donnees: &Path, version: &str) -> PathBuf {
    donnees.join("ebonhux").join("dxvk").join(version)
}

pub fn chercher_runtime_lutris(donnees: &Path, journal: &Journal) -> Result<Option<SourceDxvk>> {
    let racine = dossier_runtime(donnees);
    let entrees = match fs::read_dir(&racine) {
        Ok(entrees) => entrees,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            journal.info(ETAPE, &format!("pas de runtime DXVK de Lutris (« {} » absent)", racine.display()))?;
            return Ok(None);
        }
        Err(e) => return Err(Erreur::io("lire le runtime DXVK de Lutris", &racine, e)),
    };
    let mut meilleur: Option<(Vec<u64>, String, PathBuf)> = None;
    for entree in entrees {
        let entree = entree.map_err(|e| Erreur::io("lire le runtime DXVK de Lutris", &racine, e))?;
        let chemin = entree.path();
        let Some(nom) = entree.file_name().to_str().map(str::to_string) else {
            journal.avertir(ETAPE, &format!("entrée au nom illisible ignorée dans « {} »", racine.display()))?;
            continue;
        };
        let Some(version) = analyser_version(&nom) else {
            journal.avertir(ETAPE, &format!("version DXVK non numérique ignorée : « {nom} »"))?;
            continue;
        };
        if !chemin.is_dir() {
            journal.avertir(ETAPE, &format!("« {nom} » n'est pas un dossier de version, ignoré"))?;
            continue;
        }
        let manquantes = dll_manquantes(&chemin);
        if !manquantes.is_empty() {
            journal.avertir(
                ETAPE,
                &format!("version DXVK {nom} écartée : DLL manquante(s) {}", manquantes.join(", ")),
            )?;
            continue;
        }
        if meilleur.as_ref().is_none_or(|(haute, _, _)| version > *haute) {
            meilleur = Some((version, nom, chemin));
        }
    }
    match meilleur {
        Some((_, nom, dossier)) => {
            let version = normaliser_version(&nom).to_string();
            journal.info(ETAPE, &format!("runtime DXVK de Lutris retenu : version {version}"))?;
            Ok(Some(SourceDxvk { dossier, version, origine: Origine::RuntimeLutris }))
        }
        None => {
            journal.info(ETAPE, "aucune version complète de DXVK dans le runtime de Lutris")?;
            Ok(None)
        }
    }
}

pub fn obtenir(donnees: &Path, telechargeur: &dyn Telechargeur, journal: &Journal) -> Result<SourceDxvk> {
    if let Some(source) = chercher_runtime_lutris(donnees, journal)? {
        return Ok(source);
    }
    let cible = dossier_telecharge(donnees, VERSION_EPINGLEE);
    if cible.exists() {
        let manquantes = dll_manquantes(&cible);
        if manquantes.is_empty() {
            journal.info(
                ETAPE,
                &format!("DXVK {VERSION_EPINGLEE} déjà téléchargée, réutilisée sans nouveau téléchargement"),
            )?;
            return Ok(SourceDxvk {
                dossier: cible,
                version: VERSION_EPINGLEE.to_string(),
                origine: Origine::Telechargee,
            });
        }
        journal.avertir(
            ETAPE,
            &format!("DXVK déjà téléchargée mais incomplète ({}) : nouveau téléchargement", manquantes.join(", ")),
        )?;
    }
    journal.info(ETAPE, "pas de runtime Lutris utilisable : téléchargement de la version épinglée")?;
    telecharger_epinglee(&cible, telechargeur, journal)
}

/// `destination` est le dossier final de la version (contiendra `x32/` et `x64/`).
pub fn telecharger_epinglee(
    destination: &Path,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<SourceDxvk> {
    telecharger_archive_verifiee(destination, &ARCHIVE_EPINGLEE, telechargeur, journal)
}

/// Variante qui prend l'archive attendue en paramètre (essais : somme connue de l'archive fabriquée).
pub fn telecharger_archive_verifiee(
    destination: &Path,
    archive: &ArchiveAttendue<'_>,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<SourceDxvk> {
    let parent = destination.parent().ok_or_else(|| Erreur::Chemin {
        chemin: destination.to_path_buf(),
        raison: "aucun dossier parent pour la destination".to_string(),
    })?;
    fs::create_dir_all(parent).map_err(|e| Erreur::io("créer le dossier de destination", parent, e))?;
    let travail = creer_dossier_temporaire(parent, ETAPE)?;
    let resultat = telecharger_dans(&travail, destination, archive, telechargeur, journal);
    match fs::remove_dir_all(&travail) {
        Ok(()) => resultat,
        Err(e) if e.kind() == ErrorKind::NotFound => resultat,
        Err(e) => match resultat {
            Ok(source) => {
                journal.avertir(
                    ETAPE,
                    &format!("dossier temporaire « {} » non retiré : {e}", travail.display()),
                )?;
                Ok(source)
            }
            Err(erreur) => Err(Erreur::Archive {
                raison: format!(
                    "{erreur} ; en plus, le dossier temporaire « {} » n'a pas pu être retiré : {e}",
                    travail.display()
                ),
            }),
        },
    }
}

/// Dossier temporaire neuf `.<etape>-temp-<pid>-<nanos>` sous `parent` (réutilisé par les autres téléchargements vérifiés).
pub(crate) fn creer_dossier_temporaire(parent: &Path, etape: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let chemin = parent.join(format!(".{etape}-temp-{}-{nanos}", process::id()));
    fs::create_dir(&chemin).map_err(|e| Erreur::io("créer le dossier temporaire", &chemin, e))?;
    Ok(chemin)
}

fn echouer<T>(journal: &Journal, erreur: Erreur) -> Result<T> {
    journal.journaliser_erreur(ETAPE, &erreur)?;
    Err(erreur)
}

fn telecharger_dans(
    travail: &Path,
    destination: &Path,
    archive: &ArchiveAttendue<'_>,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<SourceDxvk> {
    let fichier = travail.join(NOM_ARCHIVE_TEMPORAIRE);
    journal.info(ETAPE, &format!("téléchargement de DXVK {} depuis {}", archive.version, archive.url))?;
    let bilan = match telechargeur.telecharger(archive.url, &fichier, LIMITE_ARCHIVE_OCTETS) {
        Ok(bilan) => bilan,
        Err(erreur) => return echouer(journal, erreur),
    };
    if !bilan.sha256.eq_ignore_ascii_case(archive.sha256) {
        return echouer(
            journal,
            Erreur::Reseau {
                url: archive.url.to_string(),
                raison: format!(
                    "somme SHA256 inattendue : attendue {}, obtenue {}",
                    archive.sha256, bilan.sha256
                ),
            },
        );
    }
    journal.info(ETAPE, &format!("somme SHA256 vérifiée ({} octets)", bilan.octets))?;
    let extrait = travail.join(NOM_EXTRAIT_TEMPORAIRE);
    if let Err(erreur) = extraire_dll(&fichier, &extrait, archive.version) {
        return echouer(journal, erreur);
    }
    installer_dossier(&extrait, destination, &travail.join(NOM_ANCIEN_TEMPORAIRE))?;
    journal.info(ETAPE, &format!("DXVK {} installée dans « {} »", archive.version, destination.display()))?;
    Ok(SourceDxvk {
        dossier: destination.to_path_buf(),
        version: archive.version.to_string(),
        origine: Origine::Telechargee,
    })
}

/// Renomme `extrait` en `destination` ; un ancien dossier incomplet est mis de côté puis restauré en cas d'échec.
fn installer_dossier(extrait: &Path, destination: &Path, ancien: &Path) -> Result<()> {
    let existait = destination.exists();
    if existait {
        fs::rename(destination, ancien)
            .map_err(|e| Erreur::io("mettre de côté l'ancien dossier DXVK", destination, e))?;
    }
    if let Err(e) = fs::rename(extrait, destination) {
        let erreur = Erreur::io("installer le dossier DXVK", destination, e);
        if existait && let Err(r) = fs::rename(ancien, destination) {
            return Err(Erreur::Archive {
                raison: format!("{erreur} ; l'ancien dossier n'a pas pu être restauré : {r}"),
            });
        }
        return Err(erreur);
    }
    Ok(())
}

fn refus(raison: impl std::fmt::Display) -> Erreur {
    Erreur::Archive { raison: raison.to_string() }
}

fn erreur_lecture_archive(e: &io::Error) -> Erreur {
    refus(format!("archive tronquée ou corrompue : {e}"))
}

/// Chemin d'entrée validé : composantes normales seulement, sous `dxvk-<version>/`.
fn composantes_validees(chemin: &Path, prefixe: &str) -> Result<Vec<String>> {
    let mut composantes = Vec::new();
    for composante in chemin.components() {
        match composante {
            Component::Normal(nom) => {
                let nom = nom
                    .to_str()
                    .ok_or_else(|| refus(format!("chemin non UTF-8 refusé : {}", chemin.display())))?;
                composantes.push(nom.to_string());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(refus(format!("chemin absolu refusé : {}", chemin.display())));
            }
            Component::ParentDir => {
                return Err(refus(format!("chemin avec « .. » refusé : {}", chemin.display())));
            }
            Component::CurDir => {
                return Err(refus(format!("chemin avec « . » refusé : {}", chemin.display())));
            }
        }
    }
    if composantes.first().map(String::as_str) != Some(prefixe) {
        return Err(refus(format!(
            "entrée hors du préfixe attendu « {prefixe}/ » : {}",
            chemin.display()
        )));
    }
    Ok(composantes)
}

/// Extrait UNIQUEMENT `x32|x64/<dll>.dll` des DLL requises vers `sortie` (dossier neuf).
fn extraire_dll(archive: &Path, sortie: &Path, version: &str) -> Result<()> {
    let prefixe = format!("dxvk-{version}");
    let fichier = File::open(archive).map_err(|e| Erreur::io("ouvrir l'archive", archive, e))?;
    let mut tar = Archive::new(GzDecoder::new(fichier));
    for architecture in SOUS_DOSSIERS_ARCHITECTURE {
        let dossier = sortie.join(architecture);
        fs::create_dir_all(&dossier).map_err(|e| Erreur::io("créer le dossier d'extraction", &dossier, e))?;
    }
    let mut extraits: BTreeSet<String> = BTreeSet::new();
    let entrees = tar.entries().map_err(|e| erreur_lecture_archive(&e))?;
    for entree in entrees {
        let mut entree = entree.map_err(|e| erreur_lecture_archive(&e))?;
        let chemin = entree.path().map_err(|e| erreur_lecture_archive(&e))?.into_owned();
        let type_entree = entree.header().entry_type();
        match type_entree {
            EntryType::Regular | EntryType::Directory | EntryType::XGlobalHeader => {}
            autre => {
                return Err(refus(format!(
                    "entrée de type non autorisé ({autre:?}, liens refusés) : {}",
                    chemin.display()
                )));
            }
        }
        let composantes = composantes_validees(&chemin, &prefixe)?;
        let taille = entree.header().size().map_err(|e| erreur_lecture_archive(&e))?;
        if taille > LIMITE_ENTREE_OCTETS {
            return Err(refus(format!(
                "entrée trop grande ({taille} octets, limite {LIMITE_ENTREE_OCTETS}) : {}",
                chemin.display()
            )));
        }
        let [_, architecture, nom] = composantes.as_slice() else { continue };
        if type_entree != EntryType::Regular || !SOUS_DOSSIERS_ARCHITECTURE.contains(&architecture.as_str()) {
            continue;
        }
        let Some(dll) = nom.strip_suffix(&format!(".{EXTENSION_DLL}")) else { continue };
        if !DLL_REQUISES.contains(&dll) {
            continue;
        }
        let relatif = format!("{architecture}/{nom}");
        if !extraits.insert(relatif.clone()) {
            return Err(refus(format!("entrée en double dans l'archive : {relatif}")));
        }
        let cible = sortie.join(architecture).join(nom);
        let mut ecrit = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&cible)
            .map_err(|e| Erreur::io("créer la DLL extraite", &cible, e))?;
        let copies = io::copy(&mut entree.by_ref().take(LIMITE_ENTREE_OCTETS), &mut ecrit)
            .map_err(|e| erreur_lecture_archive(&e))?;
        if copies != taille {
            return Err(refus(format!("entrée tronquée : {relatif} ({copies} octets sur {taille})")));
        }
    }
    let manquantes: Vec<String> = SOUS_DOSSIERS_ARCHITECTURE
        .iter()
        .flat_map(|a| DLL_REQUISES.iter().map(move |d| format!("{a}/{d}.{EXTENSION_DLL}")))
        .filter(|relatif| !extraits.contains(relatif))
        .collect();
    if manquantes.is_empty() {
        Ok(())
    } else {
        Err(refus(format!("DLL absente(s) de l'archive : {}", manquantes.join(", "))))
    }
}
