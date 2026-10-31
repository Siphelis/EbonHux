//! Détection de ce que possède le PC du joueur, sans rien modifier. Tous les chemins système
//! se lisent sous une `racine` (`/` en production, un arbre factice dans les essais) ; les
//! chemins rendus sont ceux que voit le système (`/usr/bin/wine`), sans la racine.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::erreur::Result;
use crate::executeur::{Commande, Executeur, Sortie};
use crate::journal::Journal;

const ETAPE: &str = "systeme";
const RACINE_SYSTEME: &str = "/";
const DELAI_SONDE: Duration = Duration::from_secs(30);

const FICHIER_OS_RELEASE: &str = "etc/os-release";
const FICHIER_OS_RELEASE_REPLI: &str = "usr/lib/os-release";
const FICHIER_NVIDIA: &str = "proc/driver/nvidia/version";
const FICHIER_LIMITES: &str = "proc/self/limits";
const FICHIER_MAX_MAP_COUNT: &str = "proc/sys/vm/max_map_count";
const FICHIER_NTSYNC: &str = "dev/ntsync";
const DOSSIER_DRM: &str = "sys/class/drm";
const PREFIXE_CARTE_DRM: &str = "card";
const FICHIER_VENDEUR_DRM: &str = "device/vendor";

const DOSSIERS_MODULES_WINE: [&str; 5] = [
    "usr/lib/wine",
    "usr/lib64/wine",
    "usr/lib/x86_64-linux-gnu/wine",
    "usr/lib64/wine-wow64",
    "usr/lib/wine-wow64",
];
const MODULES_WINE_ANCIEN: &str = "i386-unix";
const MODULES_WINE_WOW64: &str = "i386-windows";

const DOSSIERS_ICD: [&str; 3] = [
    "usr/share/vulkan/icd.d",
    "etc/vulkan/icd.d",
    "usr/local/share/vulkan/icd.d",
];
const DOSSIERS_BIBLIOTHEQUES_32: [&str; 3] =
    ["usr/lib32", "usr/lib/i386-linux-gnu", "usr/lib/i686-linux-gnu"];
const MARQUES_NOM_32: [&str; 2] = ["i686", "i386"];
const EXTENSION_MANIFESTE: &str = "json";
const ARCHITECTURE_32: &str = "32";

const PROGRAMME_WINE: &str = "wine";
const PROGRAMME_LUTRIS: &str = "lutris";
const PROGRAMME_VULKANINFO: &str = "vulkaninfo";
const ARGUMENT_VERSION: &str = "--version";
const ARGUMENT_RESUME: &str = "--summary";
const PREFIXE_VERSION_WINE: &str = "wine";
const PREFIXE_VERSION_LUTRIS: &str = "lutris";
const CLE_PATH: &str = "PATH";
const SEPARATEUR_PATH: char = ':';

const CLE_PERIPHERIQUE: &str = "deviceName";
const CLE_PILOTE: &str = "driverName";
const LIGNE_VERSION_NVIDIA: &str = "NVRM version";
const LIGNE_LIMITE_FICHIERS: &str = "Max open files";
const VALEUR_ILLIMITEE: &str = "unlimited";

const VARIABLE_TYPE_SESSION: &str = "XDG_SESSION_TYPE";
const VARIABLE_WAYLAND: &str = "WAYLAND_DISPLAY";
const VARIABLE_X11: &str = "DISPLAY";
const SESSION_WAYLAND: &str = "wayland";
const SESSION_X11: &str = "x11";

const VENDEUR_NVIDIA: u32 = 0x10de;
const VENDEUR_AMD: u32 = 0x1002;
const VENDEUR_INTEL: u32 = 0x8086;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FamillePaquets {
    Eopkg,
    Apt,
    Dnf,
    Pacman,
    Inconnue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Distribution {
    pub id: String,
    pub id_like: Vec<String>,
    pub nom: String,
    pub version_id: Option<String>,
    pub famille: FamillePaquets,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ModeWine {
    Wow64,
    Ancien,
    Inconnu,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfoWine {
    pub chemin: PathBuf,
    pub version: Option<String>,
    pub mode: ModeWine,
    pub dossiers_modules: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfoLutris {
    pub chemin: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FamillePilote {
    Nvidia { version: Option<String> },
    Amd,
    Intel,
    Logiciel,
    Inconnue,
}

/// Décrit la machine ; ne dit pas si le 32 bits est nécessaire (un Wine WoW64 n'en a pas
/// besoin : constaté sur Solus avec NVIDIA, sans aucune bibliothèque Vulkan 32 bits).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfoVulkan {
    pub outil_present: bool,
    pub peripheriques: Vec<String>,
    pub manifestes_64: Vec<PathBuf>,
    pub manifestes_32: Vec<PathBuf>,
    pub pilote_32_present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Session {
    Wayland,
    X11,
    Inconnue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EtatNtsync {
    Absent,
    PresentInaccessible,
    Utilisable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EchecSonde {
    pub sonde: String,
    pub raison: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Systeme {
    pub distribution: Option<Distribution>,
    pub wine: Option<InfoWine>,
    pub lutris: Option<InfoLutris>,
    pub vulkan: InfoVulkan,
    pub pilote: FamillePilote,
    pub session: Session,
    pub limite_fichiers_souple: Option<u64>,
    pub limite_fichiers_dure: Option<u64>,
    pub max_map_count: Option<u64>,
    pub ntsync: EtatNtsync,
    pub sondes_en_echec: Vec<EchecSonde>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyseOsRelease {
    pub valeurs: BTreeMap<String, String>,
    pub lignes_invalides: Vec<usize>,
}

fn analyser_valeur(brut: &str) -> Option<String> {
    let brut = brut.trim();
    let guillemet = match brut.chars().next() {
        None => return Some(String::new()),
        Some(c @ ('"' | '\'')) => c,
        Some(_) => {
            let fin = brut.find(" #").unwrap_or(brut.len());
            return Some(brut[..fin].trim_end().to_string());
        }
    };
    let reste = &brut[1..];
    let mut valeur = String::new();
    let mut caracteres = reste.char_indices();
    while let Some((position, c)) = caracteres.next() {
        if c == guillemet {
            let apres = reste[position + 1..].trim();
            return (apres.is_empty() || apres.starts_with('#')).then_some(valeur);
        }
        if c == '\\' && guillemet == '"' {
            match caracteres.next()? {
                (_, echappe @ ('"' | '\\' | '$' | '`')) => valeur.push(echappe),
                (_, autre) => {
                    valeur.push('\\');
                    valeur.push(autre);
                }
            }
            continue;
        }
        valeur.push(c);
    }
    None
}

fn cle_valide(cle: &str) -> bool {
    let mut caracteres = cle.chars();
    matches!(caracteres.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && caracteres.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Analyse un os-release : BOM et CRLF tolérés ; une ligne mal formée (clé invalide, guillemet
/// non fermé, texte après le guillemet fermant) est écartée mais son numéro est rendu.
pub fn analyser_os_release(texte: &str) -> AnalyseOsRelease {
    let texte = texte.strip_prefix('\u{feff}').unwrap_or(texte);
    let mut valeurs = BTreeMap::new();
    let mut lignes_invalides = Vec::new();
    for (indice, ligne) in texte.lines().enumerate() {
        let ligne = ligne.trim();
        if ligne.is_empty() || ligne.starts_with('#') {
            continue;
        }
        let analysee = ligne
            .split_once('=')
            .filter(|(cle, _)| cle_valide(cle.trim()))
            .and_then(|(cle, brut)| Some((cle.trim().to_string(), analyser_valeur(brut)?)));
        match analysee {
            Some((cle, valeur)) => {
                valeurs.insert(cle, valeur);
            }
            None => lignes_invalides.push(indice + 1),
        }
    }
    AnalyseOsRelease {
        valeurs,
        lignes_invalides,
    }
}

fn parmi(candidat: &str, liste: &[&str]) -> bool {
    liste.iter().any(|connu| candidat.eq_ignore_ascii_case(connu))
}

pub fn famille_depuis(id: &str, id_like: &[String]) -> FamillePaquets {
    let dans_like = |liste: &[&str]| id_like.iter().any(|parent| parmi(parent, liste));
    if id.eq_ignore_ascii_case("solus") {
        FamillePaquets::Eopkg
    } else if parmi(id, &["debian", "ubuntu"]) || dans_like(&["debian", "ubuntu"]) {
        FamillePaquets::Apt
    } else if parmi(id, &["fedora"]) || dans_like(&["fedora", "rhel"]) {
        FamillePaquets::Dnf
    } else if parmi(id, &["arch"]) || dans_like(&["arch"]) {
        FamillePaquets::Pacman
    } else {
        FamillePaquets::Inconnue
    }
}

fn version_numerique(mot: &str) -> bool {
    let segments: Vec<&str> = mot.split('.').collect();
    segments.len() >= 2
        && segments
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
}

/// Lit « <programme>-<version> » ou « <programme> <version> » sur la première ligne non vide ;
/// tout autre format rend `None` (jamais de version inventée).
fn version_apres_programme(sortie: &str, programme: &str) -> Option<String> {
    let ligne = sortie.lines().map(str::trim).find(|l| !l.is_empty())?;
    let reste = ligne.strip_prefix(programme)?;
    let apres = reste.strip_prefix('-').or_else(|| {
        reste
            .starts_with(char::is_whitespace)
            .then(|| reste.trim_start())
    })?;
    let mot = apres.split_whitespace().next()?;
    version_numerique(mot).then(|| mot.to_string())
}

pub fn version_wine_depuis(sortie: &str) -> Option<String> {
    version_apres_programme(sortie, PREFIXE_VERSION_WINE)
}

pub fn version_lutris_depuis(sortie: &str) -> Option<String> {
    version_apres_programme(sortie, PREFIXE_VERSION_LUTRIS)
}

fn valeur_cle<'a>(ligne: &'a str, cle: &str) -> Option<&'a str> {
    let valeur = ligne
        .trim_start()
        .strip_prefix(cle)?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    (!valeur.is_empty()).then_some(valeur)
}

pub fn peripheriques_depuis(sortie: &str) -> Vec<String> {
    sortie
        .lines()
        .filter_map(|l| valeur_cle(l, CLE_PERIPHERIQUE))
        .map(str::to_string)
        .collect()
}

fn famille_depuis_nom_pilote(nom: &str) -> Option<FamillePilote> {
    let nom = nom.to_lowercase();
    if nom.contains("llvmpipe") {
        Some(FamillePilote::Logiciel)
    } else if nom.contains("nvidia") {
        Some(FamillePilote::Nvidia { version: None })
    } else if nom.contains("radv") || nom.contains("amd") {
        Some(FamillePilote::Amd)
    } else if nom.contains("anv") || nom.contains("intel") {
        Some(FamillePilote::Intel)
    } else {
        None
    }
}

/// Famille d'après les `driverName` : le matériel passe avant le rendu logiciel.
pub fn pilote_depuis_vulkaninfo(sortie: &str) -> Option<FamillePilote> {
    let familles: Vec<FamillePilote> = sortie
        .lines()
        .filter_map(|l| valeur_cle(l, CLE_PILOTE))
        .filter_map(famille_depuis_nom_pilote)
        .collect();
    familles
        .iter()
        .find(|f| **f != FamillePilote::Logiciel)
        .or_else(|| familles.first())
        .cloned()
}

pub fn version_nvidia_depuis(texte: &str) -> Option<String> {
    let ligne = texte.lines().find(|l| l.contains(LIGNE_VERSION_NVIDIA))?;
    let (_, apres) = ligne.split_once(LIGNE_VERSION_NVIDIA)?;
    apres
        .split_whitespace()
        .map(|mot| mot.trim_start_matches(':'))
        .find(|mot| version_numerique(mot) && mot.split('.').count() <= 3)
        .map(str::to_string)
}

fn limite_depuis_mot(mot: &str) -> std::result::Result<u64, String> {
    if mot == VALEUR_ILLIMITEE {
        return Ok(u64::MAX);
    }
    mot.parse::<u64>()
        .map_err(|_| format!("valeur « {mot} » illisible"))
}

/// « unlimited » se représente par `u64::MAX`.
pub fn limites_fichiers_depuis(texte: &str) -> std::result::Result<(u64, u64), String> {
    let ligne = texte
        .lines()
        .find_map(|l| l.strip_prefix(LIGNE_LIMITE_FICHIERS))
        .ok_or_else(|| format!("ligne « {LIGNE_LIMITE_FICHIERS} » absente"))?;
    let mut mots = ligne.split_whitespace();
    let souple = mots
        .next()
        .ok_or_else(|| "limite souple absente".to_string())?;
    let dure = mots
        .next()
        .ok_or_else(|| "limite dure absente".to_string())?;
    Ok((limite_depuis_mot(souple)?, limite_depuis_mot(dure)?))
}

pub fn session_depuis(environnement: &BTreeMap<String, String>) -> Session {
    let present = |cle: &str| environnement.get(cle).is_some_and(|v| !v.is_empty());
    match environnement
        .get(VARIABLE_TYPE_SESSION)
        .map(|v| v.to_lowercase())
        .as_deref()
    {
        Some(SESSION_WAYLAND) => Session::Wayland,
        Some(SESSION_X11) => Session::X11,
        _ if present(VARIABLE_WAYLAND) => Session::Wayland,
        _ if present(VARIABLE_X11) => Session::X11,
        _ => Session::Inconnue,
    }
}

fn sous_racine(racine: &Path, chemin: &Path) -> PathBuf {
    racine.join(chemin.strip_prefix(RACINE_SYSTEME).unwrap_or(chemin))
}

fn chemin_systeme(relatif: &str) -> PathBuf {
    Path::new(RACINE_SYSTEME).join(relatif)
}

fn raison_io(action: &str, chemin: &Path, source: &std::io::Error) -> String {
    format!("{action} « {} » impossible : {source}", chemin.display())
}

enum Lecture {
    Absent,
    Texte(String),
    Illisible(String),
}

fn lire(racine: &Path, relatif: &str) -> Lecture {
    let chemin = racine.join(relatif);
    match fs::read_to_string(&chemin) {
        Ok(texte) => Lecture::Texte(texte),
        Err(e) if e.kind() == ErrorKind::NotFound => Lecture::Absent,
        Err(e) => Lecture::Illisible(raison_io("lecture de", &chemin, &e)),
    }
}

struct Rapporteur<'a> {
    journal: &'a Journal,
    echecs: Vec<EchecSonde>,
}

impl Rapporteur<'_> {
    fn echec(&mut self, sonde: &str, raison: &str) -> Result<()> {
        self.journal
            .avertir(&format!("{ETAPE}.{sonde}"), raison)?;
        self.echecs.push(EchecSonde {
            sonde: sonde.to_string(),
            raison: raison.to_string(),
        });
        Ok(())
    }

    fn info(&self, sonde: &str, message: &str) -> Result<()> {
        self.journal.info(&format!("{ETAPE}.{sonde}"), message)
    }
}

fn sonder_distribution(racine: &Path, rapport: &mut Rapporteur) -> Result<Option<Distribution>> {
    let texte = match lire(racine, FICHIER_OS_RELEASE) {
        Lecture::Texte(texte) => texte,
        Lecture::Illisible(raison) => {
            rapport.echec("distribution", &raison)?;
            return Ok(None);
        }
        Lecture::Absent => match lire(racine, FICHIER_OS_RELEASE_REPLI) {
            Lecture::Texte(texte) => texte,
            Lecture::Illisible(raison) => {
                rapport.echec("distribution", &raison)?;
                return Ok(None);
            }
            Lecture::Absent => {
                let raison = format!(
                    "ni « {FICHIER_OS_RELEASE} » ni « {FICHIER_OS_RELEASE_REPLI} » sous « {} »",
                    racine.display()
                );
                rapport.echec("distribution", &raison)?;
                return Ok(None);
            }
        },
    };
    let analyse = analyser_os_release(&texte);
    if !analyse.lignes_invalides.is_empty() {
        let lignes: Vec<String> = analyse.lignes_invalides.iter().map(usize::to_string).collect();
        rapport.echec(
            "distribution",
            &format!(
                "{} ligne(s) mal formée(s) écartée(s) de l'os-release (lignes {})",
                lignes.len(),
                lignes.join(", ")
            ),
        )?;
    }
    let Some(id) = analyse.valeurs.get("ID").filter(|id| !id.is_empty()) else {
        rapport.echec("distribution", "os-release sans identifiant ID exploitable")?;
        return Ok(None);
    };
    let id_like: Vec<String> = analyse
        .valeurs
        .get("ID_LIKE")
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let famille = famille_depuis(id, &id_like);
    let nom = analyse
        .valeurs
        .get("NAME")
        .filter(|n| !n.is_empty())
        .unwrap_or(id)
        .clone();
    let distribution = Distribution {
        id: id.clone(),
        id_like,
        nom,
        version_id: analyse.valeurs.get("VERSION_ID").filter(|v| !v.is_empty()).cloned(),
        famille,
    };
    rapport.info(
        "distribution",
        &format!(
            "distribution lue dans os-release : {} (famille de paquets {:?})",
            distribution.id, distribution.famille
        ),
    )?;
    Ok(Some(distribution))
}

fn chercher_executable(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    nom: &str,
) -> (Option<PathBuf>, Vec<String>) {
    let mut problemes = Vec::new();
    let Some(path) = environnement.get(CLE_PATH) else {
        problemes.push(format!("variable {CLE_PATH} absente : « {nom} » introuvable"));
        return (None, problemes);
    };
    for dossier in path.split(SEPARATEUR_PATH).filter(|d| !d.is_empty()) {
        let candidat = Path::new(dossier).join(nom);
        let reel = sous_racine(racine, &candidat);
        match fs::metadata(&reel) {
            Ok(meta) if meta.is_file() && std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o111 != 0 => {
                return (Some(candidat), problemes);
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => problemes.push(raison_io("examen de", &reel, &e)),
        }
    }
    (None, problemes)
}

fn lancer_version(
    executeur: &dyn Executeur,
    programme: &str,
) -> std::result::Result<Sortie, String> {
    let commande = Commande::new(programme)
        .arg(ARGUMENT_VERSION)
        .avec_delai(DELAI_SONDE);
    let sortie = executeur
        .lancer(&commande)
        .map_err(|e| format!("« {programme} {ARGUMENT_VERSION} » n'a pas pu être lancé : {e}"))?;
    if sortie.reussie() {
        Ok(sortie)
    } else {
        Err(format!(
            "« {programme} {ARGUMENT_VERSION} » a rendu le code {} : {}",
            sortie.code,
            sortie.stderr.trim()
        ))
    }
}

fn version_programme(
    executeur: &dyn Executeur,
    programme: &str,
    sonde: &str,
    extraire: fn(&str) -> Option<String>,
    rapport: &mut Rapporteur,
) -> Result<Option<String>> {
    let sortie = match lancer_version(executeur, programme) {
        Ok(sortie) => sortie,
        Err(raison) => {
            rapport.echec(sonde, &raison)?;
            return Ok(None);
        }
    };
    let version = extraire(&sortie.stdout);
    if version.is_none() {
        rapport.echec(
            sonde,
            &format!(
                "version de « {programme} » illisible dans la sortie : « {} »",
                sortie.stdout.trim()
            ),
        )?;
    }
    Ok(version)
}

fn sonder_executable(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    nom: &str,
    rapport: &mut Rapporteur,
) -> Result<Option<PathBuf>> {
    let (chemin, problemes) = chercher_executable(racine, environnement, nom);
    for probleme in &problemes {
        rapport.echec(nom, probleme)?;
    }
    if chemin.is_none() {
        rapport.info(nom, &format!("« {nom} » absent du PATH (ce n'est pas une erreur)"))?;
    }
    Ok(chemin)
}

fn sous_dossier_present(racine: &Path, dossier: &Path, sous: &str) -> bool {
    sous_racine(racine, dossier).join(sous).is_dir()
}

fn sonder_wine(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    executeur: &dyn Executeur,
    rapport: &mut Rapporteur,
) -> Result<Option<InfoWine>> {
    let Some(chemin) = sonder_executable(racine, environnement, PROGRAMME_WINE, rapport)? else {
        return Ok(None);
    };
    let version = version_programme(
        executeur,
        PROGRAMME_WINE,
        "wine.version",
        version_wine_depuis,
        rapport,
    )?;
    let dossiers_modules: Vec<PathBuf> = DOSSIERS_MODULES_WINE
        .iter()
        .map(|d| chemin_systeme(d))
        .filter(|d| sous_racine(racine, d).is_dir())
        .collect();
    let mode = if dossiers_modules
        .iter()
        .any(|d| sous_dossier_present(racine, d, MODULES_WINE_ANCIEN))
    {
        ModeWine::Ancien
    } else if dossiers_modules
        .iter()
        .any(|d| sous_dossier_present(racine, d, MODULES_WINE_WOW64))
    {
        ModeWine::Wow64
    } else {
        rapport.echec(
            "wine.mode",
            &format!(
                "mode de Wine indéterminé : aucun dossier de modules ne contient « {MODULES_WINE_ANCIEN} » ni « {MODULES_WINE_WOW64} » ({} dossier(s) de modules trouvé(s))",
                dossiers_modules.len()
            ),
        )?;
        ModeWine::Inconnu
    };
    rapport.info("wine", &format!("Wine trouvé : {}, mode {mode:?}", chemin.display()))?;
    Ok(Some(InfoWine {
        chemin,
        version,
        mode,
        dossiers_modules,
    }))
}

fn sonder_lutris(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    executeur: &dyn Executeur,
    rapport: &mut Rapporteur,
) -> Result<Option<InfoLutris>> {
    let Some(chemin) = sonder_executable(racine, environnement, PROGRAMME_LUTRIS, rapport)? else {
        return Ok(None);
    };
    let version = version_programme(
        executeur,
        PROGRAMME_LUTRIS,
        "lutris.version",
        version_lutris_depuis,
        rapport,
    )?;
    Ok(Some(InfoLutris { chemin, version }))
}

#[derive(Debug)]
enum ClasseManifeste {
    Bits32,
    Bits64,
}

fn classer_manifeste(racine: &Path, nom: &str, contenu: &str) -> std::result::Result<ClasseManifeste, String> {
    let valeur: serde_json::Value =
        serde_json::from_str(contenu).map_err(|e| format!("JSON invalide : {e}"))?;
    let icd = valeur
        .get("ICD")
        .ok_or_else(|| "objet « ICD » absent".to_string())?;
    let bibliotheque = icd
        .get("library_path")
        .and_then(serde_json::Value::as_str)
        .filter(|b| !b.is_empty())
        .ok_or_else(|| "« library_path » absent ou n'est pas un texte".to_string())?;
    let architecture = match icd.get("library_arch") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(a)) => Some(a.as_str()),
        Some(_) => return Err("« library_arch » n'est pas un texte".to_string()),
    };
    let nom_minuscule = nom.to_lowercase();
    let par_nom = MARQUES_NOM_32.iter().any(|m| nom_minuscule.contains(m));
    let par_arch = architecture == Some(ARCHITECTURE_32);
    let par_biblio = Path::new(bibliotheque).file_name().is_some_and(|fichier| {
        DOSSIERS_BIBLIOTHEQUES_32
            .iter()
            .any(|d| racine.join(d).join(fichier).exists())
    });
    Ok(if par_nom || par_arch || par_biblio {
        ClasseManifeste::Bits32
    } else {
        ClasseManifeste::Bits64
    })
}

fn sonder_manifestes(racine: &Path, rapport: &mut Rapporteur, vulkan: &mut InfoVulkan) -> Result<()> {
    for relatif in DOSSIERS_ICD {
        let dossier = racine.join(relatif);
        let entrees = match fs::read_dir(&dossier) {
            Ok(entrees) => entrees,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => {
                rapport.echec("vulkan.manifestes", &raison_io("lecture du dossier", &dossier, &e))?;
                continue;
            }
        };
        let mut noms = Vec::new();
        for entree in entrees {
            match entree {
                Ok(entree) => noms.push(entree.file_name().to_string_lossy().into_owned()),
                Err(e) => rapport.echec("vulkan.manifestes", &raison_io("parcours du dossier", &dossier, &e))?,
            }
        }
        noms.sort();
        for nom in noms
            .iter()
            .filter(|n| Path::new(n).extension().is_some_and(|x| x == EXTENSION_MANIFESTE))
        {
            let fichier = dossier.join(nom);
            let systeme = chemin_systeme(relatif).join(nom);
            let contenu = match fs::read_to_string(&fichier) {
                Ok(contenu) => contenu,
                Err(e) => {
                    rapport.echec("vulkan.manifestes", &raison_io("lecture du manifeste", &fichier, &e))?;
                    continue;
                }
            };
            match classer_manifeste(racine, nom, &contenu) {
                Ok(ClasseManifeste::Bits32) => vulkan.manifestes_32.push(systeme),
                Ok(ClasseManifeste::Bits64) => vulkan.manifestes_64.push(systeme),
                Err(raison) => rapport.echec(
                    "vulkan.manifestes",
                    &format!("manifeste « {} » inutilisable : {raison}", systeme.display()),
                )?,
            }
        }
    }
    vulkan.pilote_32_present = !vulkan.manifestes_32.is_empty();
    Ok(())
}

fn sonder_vulkan(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    executeur: &dyn Executeur,
    rapport: &mut Rapporteur,
) -> Result<(InfoVulkan, Option<String>)> {
    let outil = sonder_executable(racine, environnement, PROGRAMME_VULKANINFO, rapport)?;
    let mut vulkan = InfoVulkan {
        outil_present: outil.is_some(),
        peripheriques: Vec::new(),
        manifestes_64: Vec::new(),
        manifestes_32: Vec::new(),
        pilote_32_present: false,
    };
    let mut sortie_resume = None;
    if outil.is_some() {
        let commande = Commande::new(PROGRAMME_VULKANINFO)
            .arg(ARGUMENT_RESUME)
            .avec_delai(DELAI_SONDE);
        match executeur.lancer(&commande) {
            Ok(sortie) if sortie.reussie() => {
                vulkan.peripheriques = peripheriques_depuis(&sortie.stdout);
                if vulkan.peripheriques.is_empty() {
                    rapport.echec(
                        "vulkan.peripheriques",
                        &format!("aucune ligne « {CLE_PERIPHERIQUE} » dans la sortie de vulkaninfo"),
                    )?;
                }
                sortie_resume = Some(sortie.stdout);
            }
            Ok(sortie) => rapport.echec(
                "vulkan.peripheriques",
                &format!(
                    "« {PROGRAMME_VULKANINFO} {ARGUMENT_RESUME} » a rendu le code {} : {}",
                    sortie.code,
                    sortie.stderr.trim()
                ),
            )?,
            Err(e) => rapport.echec(
                "vulkan.peripheriques",
                &format!("« {PROGRAMME_VULKANINFO} {ARGUMENT_RESUME} » n'a pas pu être lancé : {e}"),
            )?,
        }
    }
    sonder_manifestes(racine, rapport, &mut vulkan)?;
    Ok((vulkan, sortie_resume))
}

fn famille_depuis_vendeur(vendeur: u32) -> Option<FamillePilote> {
    match vendeur {
        VENDEUR_NVIDIA => Some(FamillePilote::Nvidia { version: None }),
        VENDEUR_AMD => Some(FamillePilote::Amd),
        VENDEUR_INTEL => Some(FamillePilote::Intel),
        _ => None,
    }
}

fn sonder_drm(racine: &Path, rapport: &mut Rapporteur) -> Result<Option<FamillePilote>> {
    let dossier = racine.join(DOSSIER_DRM);
    let entrees = match fs::read_dir(&dossier) {
        Ok(entrees) => entrees,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            rapport.echec("pilote.drm", &raison_io("lecture du dossier", &dossier, &e))?;
            return Ok(None);
        }
    };
    let mut cartes = Vec::new();
    for entree in entrees {
        match entree {
            Ok(entree) => {
                let nom = entree.file_name().to_string_lossy().into_owned();
                let suffixe = nom.strip_prefix(PREFIXE_CARTE_DRM);
                if suffixe.is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())) {
                    cartes.push(nom);
                }
            }
            Err(e) => rapport.echec("pilote.drm", &raison_io("parcours du dossier", &dossier, &e))?,
        }
    }
    cartes.sort();
    for carte in cartes {
        let relatif = format!("{DOSSIER_DRM}/{carte}/{FICHIER_VENDEUR_DRM}");
        let texte = match lire(racine, &relatif) {
            Lecture::Texte(texte) => texte,
            Lecture::Absent => continue,
            Lecture::Illisible(raison) => {
                rapport.echec("pilote.drm", &raison)?;
                continue;
            }
        };
        let brut = texte.trim();
        let chiffres = brut.strip_prefix("0x").unwrap_or(brut);
        match u32::from_str_radix(chiffres, 16) {
            Ok(vendeur) => match famille_depuis_vendeur(vendeur) {
                Some(famille) => {
                    rapport.info(
                        "pilote",
                        &format!("pilote déduit du fabricant {brut} de la carte {carte} (déduction, pas un fait)"),
                    )?;
                    return Ok(Some(famille));
                }
                None => rapport.info("pilote", &format!("fabricant {brut} de la carte {carte} non reconnu"))?,
            },
            Err(_) => rapport.echec(
                "pilote.drm",
                &format!("fabricant illisible « {brut} » pour la carte {carte}"),
            )?,
        }
    }
    Ok(None)
}

fn sonder_pilote(
    racine: &Path,
    sortie_vulkan: Option<&str>,
    rapport: &mut Rapporteur,
) -> Result<FamillePilote> {
    match lire(racine, FICHIER_NVIDIA) {
        Lecture::Texte(texte) => {
            let version = version_nvidia_depuis(&texte);
            if version.is_none() {
                rapport.echec(
                    "pilote.version_nvidia",
                    &format!("version NVIDIA illisible dans « {FICHIER_NVIDIA} »"),
                )?;
            }
            rapport.info("pilote", &format!("pilote NVIDIA constaté par « {FICHIER_NVIDIA} »"))?;
            return Ok(FamillePilote::Nvidia { version });
        }
        Lecture::Illisible(raison) => rapport.echec("pilote.nvidia", &raison)?,
        Lecture::Absent => {}
    }
    if let Some(famille) = sortie_vulkan.and_then(pilote_depuis_vulkaninfo) {
        rapport.info(
            "pilote",
            &format!("pilote déduit du driverName de vulkaninfo : {famille:?} (déduction, pas un fait)"),
        )?;
        return Ok(famille);
    }
    if let Some(famille) = sonder_drm(racine, rapport)? {
        return Ok(famille);
    }
    rapport.echec(
        "pilote",
        "famille de pilote indéterminée : ni proc NVIDIA, ni driverName de vulkaninfo, ni fabricant DRM reconnu",
    )?;
    Ok(FamillePilote::Inconnue)
}

fn sonder_limites(racine: &Path, rapport: &mut Rapporteur) -> Result<(Option<u64>, Option<u64>)> {
    match lire(racine, FICHIER_LIMITES) {
        Lecture::Texte(texte) => match limites_fichiers_depuis(&texte) {
            Ok((souple, dure)) => return Ok((Some(souple), Some(dure))),
            Err(raison) => rapport.echec(
                "limites",
                &format!("« {FICHIER_LIMITES} » inexploitable : {raison}"),
            )?,
        },
        Lecture::Illisible(raison) => rapport.echec("limites", &raison)?,
        Lecture::Absent => rapport.echec(
            "limites",
            &format!("« {FICHIER_LIMITES} » absent sous « {} »", racine.display()),
        )?,
    }
    Ok((None, None))
}

fn sonder_max_map_count(racine: &Path, rapport: &mut Rapporteur) -> Result<Option<u64>> {
    match lire(racine, FICHIER_MAX_MAP_COUNT) {
        Lecture::Texte(texte) => match texte.trim().parse::<u64>() {
            Ok(valeur) => return Ok(Some(valeur)),
            Err(_) => rapport.echec(
                "max_map_count",
                &format!("contenu non numérique dans « {FICHIER_MAX_MAP_COUNT} » : « {} »", texte.trim()),
            )?,
        },
        Lecture::Illisible(raison) => rapport.echec("max_map_count", &raison)?,
        Lecture::Absent => rapport.echec(
            "max_map_count",
            &format!("« {FICHIER_MAX_MAP_COUNT} » absent sous « {} »", racine.display()),
        )?,
    }
    Ok(None)
}

fn sonder_ntsync(racine: &Path, rapport: &mut Rapporteur) -> Result<EtatNtsync> {
    let chemin = racine.join(FICHIER_NTSYNC);
    match fs::metadata(&chemin) {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(EtatNtsync::Absent),
        Err(e) => {
            rapport.info("ntsync", &raison_io("examen de", &chemin, &e))?;
            Ok(EtatNtsync::PresentInaccessible)
        }
        Ok(_) => match fs::OpenOptions::new().read(true).write(true).open(&chemin) {
            Ok(_) => Ok(EtatNtsync::Utilisable),
            Err(e) => {
                rapport.info("ntsync", &raison_io("ouverture en lecture et écriture de", &chemin, &e))?;
                Ok(EtatNtsync::PresentInaccessible)
            }
        },
    }
}

/// Sonde la machine sous `racine`. Une sonde en échec est journalisée (avertissement) et
/// listée dans `sondes_en_echec` ; seule une panne du journal fait échouer l'ensemble.
pub fn sonder(
    racine: &Path,
    environnement: &BTreeMap<String, String>,
    executeur: &dyn Executeur,
    journal: &Journal,
) -> Result<Systeme> {
    let mut rapport = Rapporteur {
        journal,
        echecs: Vec::new(),
    };
    let distribution = sonder_distribution(racine, &mut rapport)?;
    let wine = sonder_wine(racine, environnement, executeur, &mut rapport)?;
    let lutris = sonder_lutris(racine, environnement, executeur, &mut rapport)?;
    let (vulkan, sortie_vulkan) = sonder_vulkan(racine, environnement, executeur, &mut rapport)?;
    let pilote = sonder_pilote(racine, sortie_vulkan.as_deref(), &mut rapport)?;
    let session = session_depuis(environnement);
    if session == Session::Inconnue {
        rapport.echec(
            "session",
            &format!("session indéterminée : {VARIABLE_TYPE_SESSION}, {VARIABLE_WAYLAND} et {VARIABLE_X11} absents ou inexploitables"),
        )?;
    }
    let (limite_fichiers_souple, limite_fichiers_dure) = sonder_limites(racine, &mut rapport)?;
    let max_map_count = sonder_max_map_count(racine, &mut rapport)?;
    let ntsync = sonder_ntsync(racine, &mut rapport)?;
    Ok(Systeme {
        distribution,
        wine,
        lutris,
        vulkan,
        pilote,
        session,
        limite_fichiers_souple,
        limite_fichiers_dure,
        max_map_count,
        ntsync,
        sondes_en_echec: rapport.echecs,
    })
}
