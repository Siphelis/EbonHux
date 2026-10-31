//! Addons : lecture des `.toc`, liste hors Blizzard, catalogue embarqué, installation depuis un zip.
//!
//! Le dossier du jeu est partagé avec Windows : ce module n'écrit que sous `<jeu>/Interface/`
//! (`AddOns` pour les addons, un dossier frère de `AddOns` pour le travail).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ureq::http::Uri;
use zip::ZipArchive;

use crate::erreur::{Erreur, Result};
use crate::journal::Journal;
use crate::reseau::Telechargeur;

const ETAPE: &str = "addons";
const DOSSIER_INTERFACE: &str = "Interface";
const DOSSIER_ADDONS: &str = "AddOns";
/// Frère de `AddOns` : téléchargement et extraction y sont faits avant tout déplacement.
pub const DOSSIER_TRAVAIL: &str = "EbonHux-travail";
const EXTENSION_TOC: &str = "toc";
const EXTENSION_ZIP: &str = ".zip";
const PREFIXE_BLIZZARD: &str = "Blizzard_";
const AUTEUR_BLIZZARD: &str = "Blizzard Entertainment";
const NOM_ARCHIVE_TEMPORAIRE: &str = "archive.zip";
const NOM_EXTRAIT_TEMPORAIRE: &str = "extrait";
const NOM_ANNULE_TEMPORAIRE: &str = "annule";
const NOM_ANCIENS_TEMPORAIRE: &str = "anciens";
const CARACTERES_INTERDITS: &str = "<>:\"|?*";
const LONGUEUR_NOM_DOSSIER_MAX: usize = 255;

pub const CATALOGUE_EMBARQUE: &str = include_str!("../catalogue-addons.json");
/// Taille maximale d'un zip téléchargé.
pub const LIMITE_ZIP_OCTETS: u64 = 128 * 1024 * 1024;
pub const LIMITE_TAILLE_TOTALE_OCTETS: u64 = 512 * 1024 * 1024;
pub const LIMITE_TAILLE_ENTREE_OCTETS: u64 = 128 * 1024 * 1024;
pub const LIMITE_NOMBRE_ENTREES: usize = 20_000;
pub const LIMITE_LONGUEUR_CHEMIN: usize = 240;

// ---------- lecture d'un .toc ----------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FicheToc {
    /// Champ requis, codes de couleur retirés.
    pub titre: String,
    pub notes: Option<String>,
    pub version: Option<String>,
    pub auteur: Option<String>,
    pub interface: Vec<u32>,
    pub dependances: Vec<String>,
    pub dependances_optionnelles: Vec<String>,
    /// `Title-frFR` etc. : clé = suffixe de langue tel qu'écrit.
    pub titres_localises: BTreeMap<String, String>,
}

fn erreur_toc(raison: impl std::fmt::Display) -> Erreur {
    Erreur::analyse("le fichier .toc", raison)
}

/// Retire `|cAARRGGBB` et `|r` (`||` reste un tuyau littéral, laissé tel quel).
fn retirer_codes_couleur(texte: &str) -> String {
    let caracteres: Vec<char> = texte.chars().collect();
    let mut sortie = String::new();
    let mut i = 0;
    while i < caracteres.len() {
        if caracteres[i] == '|' {
            match caracteres.get(i + 1) {
                Some('c' | 'C')
                    if i + 10 <= caracteres.len()
                        && caracteres[i + 2..i + 10].iter().all(char::is_ascii_hexdigit) =>
                {
                    i += 10;
                    continue;
                }
                Some('r' | 'R') => {
                    i += 2;
                    continue;
                }
                Some('|') => {
                    sortie.push_str("||");
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        sortie.push(caracteres[i]);
        i += 1;
    }
    sortie.trim().to_string()
}

fn liste_separee(valeur: &str) -> Vec<String> {
    valeur
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn lire_interface(valeur: &str) -> Result<Vec<u32>> {
    valeur
        .split(',')
        .map(|morceau| {
            morceau
                .trim()
                .parse::<u32>()
                .map_err(|_| erreur_toc(format!("champ Interface illisible : « {valeur} »")))
        })
        .collect()
}

fn est_cle_dependance(cle: &str) -> bool {
    matches!(cle, "dependencies" | "requireddeps" | "reqdeps")
        || cle
            .strip_prefix("dep")
            .is_some_and(|suite| !suite.is_empty() && suite.bytes().all(|o| o.is_ascii_digit()))
}

/// Fonction pure. `Interface`, s'il est présent, doit être numérique. Tous les autres champs sont
/// facultatifs et le restent (`None` / vide) quand ils manquent ; sans `Title` (ou vide après retrait des
/// codes couleur), `titre` reste vide et l'appelant le remplace par le nom du dossier.
pub fn analyser_toc(texte: &str) -> Result<FicheToc> {
    let texte = texte.strip_prefix('\u{feff}').unwrap_or(texte);
    let mut fiche = FicheToc::default();
    let mut titre: Option<String> = None;
    for ligne in texte.lines() {
        let Some(reste) = ligne.trim().strip_prefix("##") else { continue };
        let Some((cle, valeur)) = reste.split_once(':') else { continue };
        let (cle, valeur) = (cle.trim(), valeur.trim());
        let cle_minuscule = cle.to_ascii_lowercase();
        match cle_minuscule.as_str() {
            "title" => titre = Some(retirer_codes_couleur(valeur)),
            "notes" => fiche.notes = Some(valeur.to_string()),
            "version" => fiche.version = Some(valeur.to_string()),
            "author" => fiche.auteur = Some(valeur.to_string()),
            "interface" => fiche.interface = lire_interface(valeur)?,
            "optionaldeps" => fiche.dependances_optionnelles = liste_separee(valeur),
            autre if est_cle_dependance(autre) => fiche.dependances.extend(liste_separee(valeur)),
            autre => {
                if autre.starts_with("title-") {
                    let langue = cle.get("title-".len()..).unwrap_or_default();
                    fiche
                        .titres_localises
                        .insert(langue.to_string(), retirer_codes_couleur(valeur));
                }
            }
        }
    }
    fiche.titre = titre.unwrap_or_default();
    Ok(fiche)
}

// ---------- liste des addons installés ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonInstalle {
    pub dossier: String,
    pub chemin: PathBuf,
    pub fiche: FicheToc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemeAddon {
    pub dossier: String,
    pub raison: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListeAddons {
    pub addons: Vec<AddonInstalle>,
    /// Dossiers sans `.toc` ou au `.toc` illisible : comptés, jamais ignorés.
    pub problemes: Vec<ProblemeAddon>,
    pub blizzard_exclus: usize,
    /// Fichiers isolés directement sous `AddOns` (pas des addons).
    pub fichiers_ignores: usize,
    pub dossier_absent: bool,
}

pub fn dossier_addons(jeu: &Path) -> PathBuf {
    jeu.join(DOSSIER_INTERFACE).join(DOSSIER_ADDONS)
}

fn dossier_interface(jeu: &Path) -> PathBuf {
    jeu.join(DOSSIER_INTERFACE)
}

fn nom_est_blizzard(nom: &str) -> bool {
    nom.to_lowercase().starts_with(&PREFIXE_BLIZZARD.to_lowercase())
}

fn auteur_est_blizzard(fiche: &FicheToc) -> bool {
    fiche
        .auteur
        .as_deref()
        .is_some_and(|a| a.trim().eq_ignore_ascii_case(AUTEUR_BLIZZARD))
}

/// `<dossier>/<nom>.toc`, nom comparé sans tenir compte de la casse.
fn trouver_toc(dossier: &Path, nom: &str) -> Result<Option<PathBuf>> {
    let attendu = format!("{nom}.{EXTENSION_TOC}").to_lowercase();
    let entrees = fs::read_dir(dossier).map_err(|e| Erreur::io("lire le dossier de l'addon", dossier, e))?;
    let mut trouves = Vec::new();
    for entree in entrees {
        let entree = entree.map_err(|e| Erreur::io("lire le dossier de l'addon", dossier, e))?;
        let chemin = entree.path();
        let correspond = entree
            .file_name()
            .to_str()
            .is_some_and(|n| n.to_lowercase() == attendu);
        if correspond && chemin.is_file() {
            trouves.push(chemin);
        }
    }
    trouves.sort();
    Ok(trouves.into_iter().next())
}

/// Lit la fiche d'un `.toc` ; sans titre, le nom du dossier de l'addon en tient lieu (comme dans le jeu).
fn lire_fiche(toc: &Path, dossier: &str) -> Result<FicheToc> {
    let texte = fs::read_to_string(toc).map_err(|e| match e.kind() {
        ErrorKind::InvalidData => erreur_toc(format!("« {} » n'est pas du texte UTF-8", toc.display())),
        _ => Erreur::io("lire le fichier .toc", toc, e),
    })?;
    let mut fiche = analyser_toc(&texte)?;
    if fiche.titre.is_empty() {
        fiche.titre = dossier.to_string();
    }
    Ok(fiche)
}

pub fn lister(jeu: &Path, journal: &Journal) -> Result<ListeAddons> {
    let racine = dossier_addons(jeu);
    let mut liste = ListeAddons::default();
    let entrees = match fs::read_dir(&racine) {
        Ok(entrees) => entrees,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            journal.avertir(ETAPE, &format!("dossier des addons absent : « {} »", racine.display()))?;
            liste.dossier_absent = true;
            return Ok(liste);
        }
        Err(e) => return Err(Erreur::io("lire le dossier des addons", &racine, e)),
    };
    let mut noms = Vec::new();
    for entree in entrees {
        let entree = entree.map_err(|e| Erreur::io("lire le dossier des addons", &racine, e))?;
        noms.push(entree.file_name());
    }
    noms.sort();
    for nom_os in noms {
        let chemin = racine.join(&nom_os);
        let Some(nom) = nom_os.to_str().map(str::to_string) else {
            let dossier = nom_os.to_string_lossy().into_owned();
            signaler(journal, &mut liste, dossier, "nom de dossier non UTF-8".to_string())?;
            continue;
        };
        let metadonnees = fs::metadata(&chemin).map_err(|e| Erreur::io("lire l'entrée", &chemin, e))?;
        if !metadonnees.is_dir() {
            liste.fichiers_ignores += 1;
            continue;
        }
        if nom_est_blizzard(&nom) {
            liste.blizzard_exclus += 1;
            continue;
        }
        let toc = match trouver_toc(&chemin, &nom)? {
            Some(toc) => toc,
            None => {
                signaler(journal, &mut liste, nom.clone(), format!("aucun fichier « {nom}.toc »"))?;
                continue;
            }
        };
        match lire_fiche(&toc, &nom) {
            Ok(fiche) if auteur_est_blizzard(&fiche) => liste.blizzard_exclus += 1,
            Ok(fiche) => liste.addons.push(AddonInstalle { dossier: nom, chemin, fiche }),
            Err(erreur) => signaler(journal, &mut liste, nom, erreur.to_string())?,
        }
    }
    liste
        .addons
        .sort_by(|a, b| (a.dossier.to_lowercase(), &a.dossier).cmp(&(b.dossier.to_lowercase(), &b.dossier)));
    journal.info(
        ETAPE,
        &format!(
            "{} addon(s) listé(s), {} problème(s), {} Blizzard exclu(s), {} fichier(s) isolé(s)",
            liste.addons.len(),
            liste.problemes.len(),
            liste.blizzard_exclus,
            liste.fichiers_ignores
        ),
    )?;
    Ok(liste)
}

fn signaler(journal: &Journal, liste: &mut ListeAddons, dossier: String, raison: String) -> Result<()> {
    journal.avertir(ETAPE, &format!("addon « {dossier} » inutilisable : {raison}"))?;
    liste.problemes.push(ProblemeAddon { dossier, raison });
    Ok(())
}

// ---------- catalogue embarqué ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntreeCatalogue {
    pub nom: String,
    pub description: String,
    /// Lien zip direct, `https://` uniquement.
    pub url: String,
    /// Dossier de premier niveau que le zip doit contenir (inconnu pour le catalogue réel).
    pub dossier: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Telle qu'annoncée par le catalogue, brute (ex. « 4.4.2-12-g94f3b58 ») : voir [`version_plus_recente`].
    #[serde(default)]
    pub version: Option<String>,
    /// Description longue du catalogue (texte brut ou HTML), jamais tronquée ni limitée.
    #[serde(default)]
    pub description_longue: Option<String>,
    /// Adresses absolues des images de présentation.
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntreeInvalide {
    /// Rang dans le tableau, à partir de 1.
    pub position: usize,
    pub raison: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalogue {
    pub entrees: Vec<EntreeCatalogue>,
    pub invalides: Vec<EntreeInvalide>,
}

/// Un nom de dossier simple : un seul composant, sans séparateur ni caractère interdit sous Windows.
fn valider_nom_simple(nom: &str) -> std::result::Result<(), String> {
    if nom.is_empty() || nom.trim() != nom {
        return Err("nom vide ou entouré d'espaces".to_string());
    }
    if nom == "." || nom == ".." {
        return Err(format!("« {nom} » n'est pas un nom de dossier simple"));
    }
    if nom.len() > LONGUEUR_NOM_DOSSIER_MAX {
        return Err("nom trop long".to_string());
    }
    if nom.contains('/') || nom.contains('\\') {
        return Err(format!("« {nom} » contient un séparateur de chemin"));
    }
    if nom.chars().any(|c| c.is_control() || CARACTERES_INTERDITS.contains(c)) {
        return Err(format!("« {nom} » contient un caractère interdit"));
    }
    Ok(())
}

fn valider_url_zip(url: &str) -> std::result::Result<(), String> {
    let uri: Uri = url.parse().map_err(|e| format!("URL invalide : {e}"))?;
    if uri.scheme_str() != Some("https") {
        return Err("l'URL doit commencer par https://".to_string());
    }
    if uri.host().is_none_or(str::is_empty) {
        return Err("URL sans nom d'hôte".to_string());
    }
    if !uri.path().to_lowercase().ends_with(EXTENSION_ZIP) {
        return Err(format!("l'URL doit désigner un fichier {EXTENSION_ZIP}"));
    }
    Ok(())
}

fn valider_entree(entree: &EntreeCatalogue, noms_vus: &mut BTreeSet<String>) -> std::result::Result<(), String> {
    if entree.nom.trim().is_empty() {
        return Err("nom vide".to_string());
    }
    if entree.description.trim().is_empty() {
        return Err("description vide".to_string());
    }
    valider_url_zip(&entree.url)?;
    if let Some(dossier) = &entree.dossier {
        valider_nom_simple(dossier).map_err(|e| format!("dossier attendu invalide : {e}"))?;
    }
    if !noms_vus.insert(entree.nom.trim().to_lowercase()) {
        return Err(format!("nom « {} » déjà utilisé par une entrée précédente", entree.nom));
    }
    Ok(())
}

/// JSON malformé ou qui n'est pas un tableau : refus de tout le catalogue.
/// Une entrée invalide est mise de côté avec sa position ; les autres restent utilisables.
pub fn analyser_catalogue(texte: &str) -> Result<Catalogue> {
    let valeurs: Vec<serde_json::Value> =
        serde_json::from_str(texte).map_err(|e| Erreur::analyse("le catalogue d'addons", e))?;
    let mut catalogue = Catalogue::default();
    let mut noms_vus = BTreeSet::new();
    for (rang, valeur) in valeurs.into_iter().enumerate() {
        let position = rang + 1;
        let resultat = serde_json::from_value::<EntreeCatalogue>(valeur)
            .map_err(|e| e.to_string())
            .and_then(|entree| valider_entree(&entree, &mut noms_vus).map(|()| entree));
        match resultat {
            Ok(entree) => catalogue.entrees.push(entree),
            Err(raison) => catalogue.invalides.push(EntreeInvalide { position, raison }),
        }
    }
    Ok(catalogue)
}

/// Retire un `v` de tête puis coupe au premier caractère qui n'est ni un chiffre ni un point
/// (ex. « 4.4.2-12-g94f3b58 » -> « 4.4.2 », « v5.11.04 » -> « 5.11.04 »).
fn alleger_version(brute: &str) -> &str {
    let sans_v = brute.strip_prefix(['v', 'V']).unwrap_or(brute);
    let fin = sans_v.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(sans_v.len());
    &sans_v[..fin]
}

/// Composants numériques d'un numéro allégé ; `None` si vide ou si un composant n'est pas un entier.
fn composantes_version(allegee: &str) -> Option<Vec<u64>> {
    if allegee.is_empty() {
        return None;
    }
    allegee.split('.').map(|c| c.parse::<u64>().ok()).collect()
}

/// Fonction pure : la version du catalogue est-elle plus récente que celle installée ?
/// Compare composant par composant en entier (pas en chaîne), un composant manquant valant 0
/// (`4.10` est donc plus récent que `4.9`, et `4.4` égale `4.4.0`). `None` si l'un des deux numéros
/// est illisible une fois allégé (absent, vide, ou contenant un composant non numérique).
pub fn version_plus_recente(catalogue: Option<&str>, installee: Option<&str>) -> Option<bool> {
    let catalogue = composantes_version(alleger_version(catalogue?))?;
    let installee = composantes_version(alleger_version(installee?))?;
    let longueur = catalogue.len().max(installee.len());
    for i in 0..longueur {
        let c = catalogue.get(i).copied().unwrap_or(0);
        let inst = installee.get(i).copied().unwrap_or(0);
        if c != inst {
            return Some(c > inst);
        }
    }
    Some(false)
}

const HOTE_ELENDIL_ADDONS: &str = "https://launcher.way-of-elendil.fr/addons/";

#[derive(Deserialize)]
struct EntreeApiElendil {
    #[serde(default)]
    name: String,
    #[serde(default)]
    addon_url: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    images: Vec<serde_json::Value>,
    #[serde(default)]
    activated: bool,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    long_description: Option<String>,
}

/// way-of-elendil rend ses chemins (zip comme avatar) relatifs à son hôte de téléchargement :
/// "/downloads/…zip" ou "/downloads/…jpg" doivent devenir "https://launcher.way-of-elendil.fr/addons/downloads/…".
fn adresse_elendil_absolue(chemin: &str) -> String {
    if chemin.starts_with("https://") || chemin.starts_with("http://") {
        chemin.to_string()
    } else {
        format!("{HOTE_ELENDIL_ADDONS}{}", chemin.trim_start_matches('/'))
    }
}

/// Catalogue réel de way-of-elendil.fr : seules les entrées `activated` sont gardées ; une entrée
/// invalide est mise de côté avec sa position, un JSON qui n'est pas un tableau refuse tout.
pub fn depuis_api_elendil(texte: &str) -> Result<Catalogue> {
    let valeurs: Vec<serde_json::Value> =
        serde_json::from_str(texte).map_err(|e| Erreur::analyse("le catalogue d'addons (API way-of-elendil)", e))?;
    let mut catalogue = Catalogue::default();
    let mut noms_vus = BTreeSet::new();
    for (rang, valeur) in valeurs.into_iter().enumerate() {
        let position = rang + 1;
        let api = match serde_json::from_value::<EntreeApiElendil>(valeur) {
            Ok(api) => api,
            Err(e) => {
                catalogue.invalides.push(EntreeInvalide { position, raison: e.to_string() });
                continue;
            }
        };
        if !api.activated {
            continue;
        }
        let entree = EntreeCatalogue {
            nom: api.name,
            description: api.short_description,
            url: adresse_elendil_absolue(&api.addon_url),
            dossier: None,
            avatar: api
                .avatar
                .filter(|a| !a.is_empty())
                .or_else(|| api.images.iter().find_map(|i| i.as_str().filter(|i| !i.is_empty()).map(str::to_string)))
                .map(|a| adresse_elendil_absolue(&a)),
            categories: api.categories,
            version: api.version.filter(|v| !v.trim().is_empty()),
            description_longue: api.long_description.filter(|d| !d.trim().is_empty()),
            images: api
                .images
                .iter()
                .filter_map(|i| i.as_str().filter(|i| !i.is_empty()))
                .map(adresse_elendil_absolue)
                .collect(),
        };
        match valider_entree(&entree, &mut noms_vus) {
            Ok(()) => catalogue.entrees.push(entree),
            Err(raison) => catalogue.invalides.push(EntreeInvalide { position, raison }),
        }
    }
    Ok(catalogue)
}

pub fn catalogue_embarque(journal: &Journal) -> Result<Catalogue> {
    let catalogue = match analyser_catalogue(CATALOGUE_EMBARQUE) {
        Ok(catalogue) => catalogue,
        Err(erreur) => {
            journal.journaliser_erreur(ETAPE, &erreur)?;
            return Err(erreur);
        }
    };
    for invalide in &catalogue.invalides {
        journal.avertir(
            ETAPE,
            &format!("catalogue : entrée n° {} refusée : {}", invalide.position, invalide.raison),
        )?;
    }
    journal.info(
        ETAPE,
        &format!(
            "catalogue embarqué : {} entrée(s) valide(s), {} refusée(s)",
            catalogue.entrees.len(),
            catalogue.invalides.len()
        ),
    )?;
    Ok(catalogue)
}

// ---------- installation depuis un zip ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitesExtraction {
    pub taille_totale: u64,
    pub taille_entree: u64,
    pub nombre_entrees: usize,
    pub longueur_chemin: usize,
}

impl Default for LimitesExtraction {
    fn default() -> Self {
        Self {
            taille_totale: LIMITE_TAILLE_TOTALE_OCTETS,
            taille_entree: LIMITE_TAILLE_ENTREE_OCTETS,
            nombre_entrees: LIMITE_NOMBRE_ENTREES,
            longueur_chemin: LIMITE_LONGUEUR_CHEMIN,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OptionsInstallation {
    /// Sans cette option, un addon du même nom déjà installé fait refuser l'installation.
    pub remplacer: bool,
    pub limites: LimitesExtraction,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResultatInstallation {
    pub installes: Vec<String>,
}

fn refus(raison: impl std::fmt::Display) -> Erreur {
    Erreur::Archive { raison: raison.to_string() }
}

fn nanos() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
}

/// Validation d'un nom d'entrée de zip ; rend ses composantes.
fn valider_nom_entree(nom: &str, limites: &LimitesExtraction) -> Result<Vec<String>> {
    if nom.is_empty() {
        return Err(refus("entrée au nom vide"));
    }
    if nom.len() > limites.longueur_chemin {
        return Err(refus(format!(
            "nom d'entrée trop long ({} octets, limite {}) : {nom}",
            nom.len(),
            limites.longueur_chemin
        )));
    }
    if nom.contains('\\') {
        return Err(refus(format!("antislash refusé dans « {nom} »")));
    }
    if nom.starts_with('/') {
        return Err(refus(format!("chemin absolu refusé : {nom}")));
    }
    let sans_fin = nom.strip_suffix('/').unwrap_or(nom);
    let mut composantes = Vec::new();
    for composante in sans_fin.split('/') {
        match composante {
            "" => return Err(refus(format!("composante vide refusée dans « {nom} »"))),
            "." => return Err(refus(format!("« . » refusé dans « {nom} »"))),
            ".." => return Err(refus(format!("« .. » refusé dans « {nom} »"))),
            _ => {}
        }
        if composante.chars().any(|c| c.is_control() || CARACTERES_INTERDITS.contains(c)) {
            return Err(refus(format!("caractère interdit dans « {nom} »")));
        }
        composantes.push(composante.to_string());
    }
    Ok(composantes)
}

struct EntreePlanifiee {
    composantes: Vec<String>,
    dossier: bool,
}

/// Type de fichier des bits de mode Unix (0o120000 = lien symbolique).
fn mode_autorise(mode: Option<u32>) -> bool {
    const MASQUE_TYPE: u32 = 0o170_000;
    const FICHIER: u32 = 0o100_000;
    const DOSSIER: u32 = 0o040_000;
    match mode {
        None => true,
        Some(m) => matches!(m & MASQUE_TYPE, 0 | FICHIER | DOSSIER),
    }
}

const SIGNATURE_ENTETE_CENTRAL: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const TAILLE_ENTETE_CENTRAL: usize = 46;

/// Compte les en-têtes du répertoire central lus directement : la bibliothèque `zip` fusionne
/// silencieusement deux entrées de même nom, ce qui masquerait un doublon.
fn compter_entetes_centraux(archive: &Path, debut: u64) -> Result<usize> {
    let erreur_io = |e| Erreur::io("lire le répertoire central du zip", archive, e);
    let mut fichier = File::open(archive).map_err(erreur_io)?;
    fichier.seek(SeekFrom::Start(debut)).map_err(erreur_io)?;
    let mut compte = 0;
    loop {
        let mut tete = [0u8; TAILLE_ENTETE_CENTRAL];
        match fichier.read_exact(&mut tete) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(erreur_io(e)),
        }
        if tete[..4] != SIGNATURE_ENTETE_CENTRAL {
            break;
        }
        let variable: i64 = [28, 30, 32]
            .iter()
            .map(|&o| i64::from(u16::from_le_bytes([tete[o], tete[o + 1]])))
            .sum();
        fichier.seek(SeekFrom::Current(variable)).map_err(erreur_io)?;
        compte += 1;
    }
    Ok(compte)
}

/// Valide TOUT le zip (noms, types, doublons, limites annoncées) avant d'écrire quoi que ce soit.
fn planifier(zip: &mut ZipArchive<File>, archive: &Path, limites: &LimitesExtraction) -> Result<Vec<EntreePlanifiee>> {
    let nombre = zip.len();
    if nombre == 0 {
        return Err(refus("zip vide"));
    }
    if nombre > limites.nombre_entrees {
        return Err(refus(format!(
            "trop d'entrées ({nombre}, limite {})",
            limites.nombre_entrees
        )));
    }
    let declarees = compter_entetes_centraux(archive, zip.central_directory_start())?;
    if declarees != nombre {
        return Err(refus(format!(
            "entrée en double ou répertoire central incohérent ({declarees} en-têtes, {nombre} entrées distinctes)"
        )));
    }
    let mut vus = BTreeSet::new();
    let mut plan = Vec::with_capacity(nombre);
    let mut total: u64 = 0;
    for i in 0..nombre {
        let entree = zip
            .by_index_raw(i)
            .map_err(|e| refus(format!("entrée {i} illisible : {e}")))?;
        let nom = entree.name().to_string();
        if entree.is_symlink() || !mode_autorise(entree.unix_mode()) {
            return Err(refus(format!("lien symbolique ou fichier spécial refusé : {nom}")));
        }
        let composantes = valider_nom_entree(&nom, limites)?;
        let dossier = entree.is_dir();
        // Insensible à la casse : le dossier du jeu est partagé avec Windows.
        if !vus.insert(composantes.join("/").to_lowercase()) {
            return Err(refus(format!("entrée en double : {nom}")));
        }
        if composantes.len() == 1 && !dossier {
            return Err(refus(format!("fichier à la racine du zip, hors d'un dossier d'addon : {nom}")));
        }
        let taille = entree.size();
        if taille > limites.taille_entree {
            return Err(refus(format!(
                "entrée trop grande ({taille} octets, limite {}) : {nom}",
                limites.taille_entree
            )));
        }
        total = total.saturating_add(taille);
        if total > limites.taille_totale {
            return Err(refus(format!(
                "taille décompressée totale trop grande (plus de {} octets)",
                limites.taille_totale
            )));
        }
        plan.push(EntreePlanifiee { composantes, dossier });
    }
    Ok(plan)
}

/// Extrait dans `sortie` (dossier à créer, neuf) ; rend les dossiers de premier niveau, triés.
/// Les tailles réellement écrites sont recomptées : une taille annoncée mensongère ne contourne pas les limites.
fn extraire_zip(archive: &Path, sortie: &Path, limites: &LimitesExtraction) -> Result<Vec<String>> {
    let fichier = File::open(archive).map_err(|e| Erreur::io("ouvrir le zip", archive, e))?;
    let mut zip = ZipArchive::new(fichier).map_err(|e| refus(format!("zip illisible : {e}")))?;
    let plan = planifier(&mut zip, archive, limites)?;
    fs::create_dir(sortie).map_err(|e| Erreur::io("créer le dossier d'extraction", sortie, e))?;
    let mut racines = BTreeSet::new();
    let mut total: u64 = 0;
    for (i, element) in plan.iter().enumerate() {
        racines.insert(element.composantes[0].clone());
        let mut cible = sortie.to_path_buf();
        cible.extend(&element.composantes);
        if element.dossier {
            fs::create_dir_all(&cible).map_err(|e| Erreur::io("créer un dossier extrait", &cible, e))?;
            continue;
        }
        if let Some(parent) = cible.parent() {
            fs::create_dir_all(parent).map_err(|e| Erreur::io("créer un dossier extrait", parent, e))?;
        }
        let mut entree = zip
            .by_index(i)
            .map_err(|e| refus(format!("entrée « {} » illisible : {e}", element.composantes.join("/"))))?;
        let mut ecrit = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&cible)
            .map_err(|e| Erreur::io("créer un fichier extrait", &cible, e))?;
        let copies = io::copy(&mut (&mut entree).take(limites.taille_entree.saturating_add(1)), &mut ecrit)
            .map_err(|e| refus(format!("archive tronquée ou corrompue ({}) : {e}", element.composantes.join("/"))))?;
        if copies > limites.taille_entree {
            return Err(refus(format!(
                "entrée plus grande que sa taille annoncée et que la limite ({}) : {}",
                limites.taille_entree,
                element.composantes.join("/")
            )));
        }
        total = total.saturating_add(copies);
        if total > limites.taille_totale {
            return Err(refus(format!(
                "taille décompressée totale trop grande (plus de {} octets)",
                limites.taille_totale
            )));
        }
    }
    Ok(racines.into_iter().collect())
}

/// Chaque dossier de premier niveau doit être un addon complet et non Blizzard.
fn verifier_addons(extrait: &Path, racines: &[String], attendu: Option<&str>) -> Result<()> {
    for racine in racines {
        if nom_est_blizzard(racine) {
            return Err(refus(format!("« {racine} » est un addon Blizzard : jamais touché")));
        }
        let Some(toc) = trouver_toc(&extrait.join(racine), racine)? else {
            return Err(refus(format!("le dossier « {racine} » ne contient pas « {racine}/{racine}.toc »")));
        };
        lire_fiche(&toc, racine).map_err(|e| refus(format!("addon « {racine} » : {e}")))?;
    }
    if let Some(attendu) = attendu
        && !racines.iter().any(|r| r.eq_ignore_ascii_case(attendu))
    {
        return Err(refus(format!(
            "le zip ne contient pas le dossier attendu « {attendu} » (trouvé : {})",
            racines.join(", ")
        )));
    }
    Ok(())
}

fn trouver_existant(addons: &Path, nom: &str) -> Result<Option<PathBuf>> {
    let entrees = match fs::read_dir(addons) {
        Ok(entrees) => entrees,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Erreur::io("lire le dossier des addons", addons, e)),
    };
    let minuscule = nom.to_lowercase();
    for entree in entrees {
        let entree = entree.map_err(|e| Erreur::io("lire le dossier des addons", addons, e))?;
        if entree.file_name().to_str().is_some_and(|n| n.to_lowercase() == minuscule) {
            return Ok(Some(entree.path()));
        }
    }
    Ok(None)
}

struct Deplacement {
    nom: String,
    cible: PathBuf,
    /// (ancien emplacement, place provisoire dans le dossier de travail)
    ancien: Option<(PathBuf, PathBuf)>,
}

fn renommer(action: &str, de: &Path, vers: &Path) -> Result<()> {
    fs::rename(de, vers).map_err(|e| Erreur::io(&format!("{action} (vers « {} »)", vers.display()), de, e))
}

fn deplacer_un(
    travail: &Path,
    addons: &Path,
    extrait: &Path,
    nom: &str,
    journal: &Journal,
) -> Result<Deplacement> {
    let cible = addons.join(nom);
    let mut ancien = None;
    if let Some(existant) = trouver_existant(addons, nom)? {
        let provisoire = travail.join(NOM_ANCIENS_TEMPORAIRE).join(nom);
        if let Some(parent) = provisoire.parent() {
            fs::create_dir_all(parent).map_err(|e| Erreur::io("créer le dossier de travail", parent, e))?;
        }
        renommer("écarter l'ancien addon", &existant, &provisoire)?;
        journal.info(ETAPE, &format!("addon « {nom} » remplacé"))?;
        ancien = Some((existant, provisoire));
    }
    let deplacement = Deplacement { nom: nom.to_string(), cible: cible.clone(), ancien };
    match renommer("installer l'addon", &extrait.join(nom), &cible) {
        Ok(()) => Ok(deplacement),
        Err(erreur) => Err(annuler(vec![deplacement], None, erreur)),
    }
}

/// Désinstalle un addon : son dossier est réellement supprimé (aucune sauvegarde).
/// Un lien symbolique est seulement retiré : sa cible n'est jamais touchée.
pub fn desinstaller(jeu: &Path, dossier: &str, journal: &Journal) -> Result<()> {
    if nom_est_blizzard(dossier) {
        return Err(refus(format!("« {dossier} » est un addon Blizzard : jamais touché")));
    }
    let addons = dossier_addons(jeu);
    let Some(existant) = trouver_existant(&addons, dossier)? else {
        return Err(refus(format!("l'addon « {dossier} » n'est pas installé")));
    };
    let meta = fs::symlink_metadata(&existant).map_err(|e| Erreur::io("lire l'addon à supprimer", &existant, e))?;
    let resultat = if meta.file_type().is_symlink() {
        fs::remove_file(&existant).or_else(|_| fs::remove_dir(&existant))
    } else {
        fs::remove_dir_all(&existant)
    };
    resultat.map_err(|e| Erreur::io("supprimer l'addon", &existant, e))?;
    journal.info(ETAPE, &format!("addon « {dossier} » supprimé"))?;
    Ok(())
}

/// Défait les déplacements dans l'ordre inverse ; les échecs du retour arrière sont ajoutés à l'erreur.
fn annuler(faits: Vec<Deplacement>, travail: Option<&Path>, erreur: Erreur) -> Erreur {
    let mut echecs = Vec::new();
    for d in faits.into_iter().rev() {
        if d.cible.exists() {
            let retour = travail.map(|t| t.join(NOM_ANNULE_TEMPORAIRE));
            let resultat = match retour {
                Some(retour) => fs::create_dir_all(&retour)
                    .map_err(|e| Erreur::io("créer le dossier de retour arrière", &retour, e))
                    .and_then(|()| renommer("retirer l'addon installé", &d.cible, &retour.join(&d.nom))),
                None => Err(Erreur::Chemin {
                    chemin: d.cible.clone(),
                    raison: "aucun dossier de retour arrière".to_string(),
                }),
            };
            if let Err(e) = resultat {
                echecs.push(e.to_string());
            }
        }
        if let Some((existant, provisoire)) = &d.ancien
            && let Err(e) = renommer("restaurer l'ancien addon", provisoire, existant)
        {
            echecs.push(e.to_string());
        }
    }
    if echecs.is_empty() {
        erreur
    } else {
        refus(format!("{erreur} ; retour arrière incomplet : {}", echecs.join(" ; ")))
    }
}

fn appliquer(
    addons: &Path,
    travail: &Path,
    racines: &[String],
    options: &OptionsInstallation,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    let extrait = travail.join(NOM_EXTRAIT_TEMPORAIRE);
    if !options.remplacer {
        for nom in racines {
            if let Some(existant) = trouver_existant(addons, nom)? {
                return Err(Erreur::Chemin {
                    chemin: existant,
                    raison: format!("addon « {nom} » déjà installé : refus (option « remplacer » non demandée)"),
                });
            }
        }
    }
    fs::create_dir_all(addons).map_err(|e| Erreur::io("créer le dossier des addons", addons, e))?;
    let mut faits: Vec<Deplacement> = Vec::new();
    for nom in racines {
        match deplacer_un(travail, addons, &extrait, nom, journal) {
            Ok(d) => faits.push(d),
            Err(erreur) => return Err(annuler(faits, Some(travail), erreur)),
        }
    }
    let mut resultat = ResultatInstallation::default();
    for d in faits {
        resultat.installes.push(d.nom);
    }
    Ok(resultat)
}

/// Dossier de travail neuf sous `Interface/EbonHux-travail`, retiré en fin (succès ou échec).
fn avec_travail<T>(interface: &Path, journal: &Journal, travail_fn: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let parent = interface.join(DOSSIER_TRAVAIL);
    fs::create_dir_all(&parent).map_err(|e| Erreur::io("créer le dossier de travail", &parent, e))?;
    let travail = parent.join(format!("session-{}-{}", process::id(), nanos()));
    fs::create_dir(&travail).map_err(|e| Erreur::io("créer le dossier de travail", &travail, e))?;
    let resultat = travail_fn(&travail);
    let nettoyage = fs::remove_dir_all(&travail)
        .map_err(|e| Erreur::io("retirer le dossier de travail", &travail, e))
        .and_then(|()| match fs::remove_dir(&parent) {
            Ok(()) => Ok(()),
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty) => Ok(()),
            Err(e) => Err(Erreur::io("retirer le dossier de travail", &parent, e)),
        });
    match (resultat, nettoyage) {
        (resultat, Ok(())) => resultat,
        (Ok(valeur), Err(e)) => {
            journal.avertir(ETAPE, &format!("nettoyage incomplet : {e}"))?;
            Ok(valeur)
        }
        (Err(erreur), Err(e)) => Err(refus(format!("{erreur} ; en plus, nettoyage impossible : {e}"))),
    }
}

enum Source<'a> {
    Fichier(&'a Path),
    Url(&'a str, &'a dyn Telechargeur),
}

fn installer(
    jeu: &Path,
    source: &Source<'_>,
    attendu: Option<&str>,
    options: &OptionsInstallation,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    if !jeu.is_dir() {
        return Err(Erreur::Chemin {
            chemin: jeu.to_path_buf(),
            raison: "le dossier du jeu n'existe pas".to_string(),
        });
    }
    let interface = dossier_interface(jeu);
    let addons = dossier_addons(jeu);
    fs::create_dir_all(&interface).map_err(|e| Erreur::io("créer le dossier Interface", &interface, e))?;
    avec_travail(&interface, journal, |travail| {
        let archive = match source {
            Source::Fichier(chemin) => chemin.to_path_buf(),
            Source::Url(url, telechargeur) => {
                valider_url_zip(url).map_err(|raison| Erreur::Reseau { url: (*url).to_string(), raison })?;
                let destination = travail.join(NOM_ARCHIVE_TEMPORAIRE);
                journal.info(ETAPE, &format!("téléchargement de {url}"))?;
                let bilan = telechargeur.telecharger(url, &destination, LIMITE_ZIP_OCTETS)?;
                journal.info(ETAPE, &format!("{} octets reçus", bilan.octets))?;
                destination
            }
        };
        let extrait = travail.join(NOM_EXTRAIT_TEMPORAIRE);
        let racines = extraire_zip(&archive, &extrait, &options.limites)?;
        verifier_addons(&extrait, &racines, attendu)?;
        let resultat = appliquer(&addons, travail, &racines, options, journal)?;
        journal.info(ETAPE, &format!("addon(s) installé(s) : {}", resultat.installes.join(", ")))?;
        Ok(resultat)
    })
}

fn journaliser_echec<T>(journal: &Journal, resultat: Result<T>) -> Result<T> {
    if let Err(erreur) = &resultat {
        journal.journaliser_erreur(ETAPE, erreur)?;
    }
    resultat
}

/// Installe un zip local. `attendu` : dossier que le zip doit contenir (entrée du catalogue).
pub fn installer_depuis_zip(
    jeu: &Path,
    zip: &Path,
    attendu: Option<&str>,
    options: &OptionsInstallation,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    journaliser_echec(journal, installer(jeu, &Source::Fichier(zip), attendu, options, journal))
}

/// Télécharge (https, `.zip`, taille limitée) puis installe.
pub fn installer_depuis_url(
    jeu: &Path,
    url: &str,
    attendu: Option<&str>,
    options: &OptionsInstallation,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    journaliser_echec(journal, installer(jeu, &Source::Url(url, telechargeur), attendu, options, journal))
}

/// Bouton « Installer » du catalogue.
pub fn installer_entree(
    jeu: &Path,
    entree: &EntreeCatalogue,
    options: &OptionsInstallation,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    installer_depuis_url(jeu, &entree.url, entree.dossier.as_deref(), options, telechargeur, journal)
}

#[cfg(test)]
mod tests {
    use super::depuis_api_elendil;

    #[test]
    fn api_elendil_avatar_repli_sur_la_premiere_image() {
        let texte = r#"[
            {"name":"A","addon_url":"/a.zip","short_description":"d","avatar":"","images":["","/img/a0.jpg","/img/a1.jpg"],"activated":true},
            {"name":"B","addon_url":"/b.zip","short_description":"d","avatar":"/av/b.png","images":["/img/b0.jpg"],"activated":true},
            {"name":"C","addon_url":"/c.zip","short_description":"d","images":[],"activated":true},
            {"name":"D","addon_url":"/d.zip","short_description":"d","images":["https://cdn.test/d.jpg"],"activated":true}
        ]"#;
        let c = depuis_api_elendil(texte).unwrap();
        assert_eq!(c.entrees[0].avatar.as_deref(), Some("https://launcher.way-of-elendil.fr/addons/img/a0.jpg"));
        assert_eq!(c.entrees[1].avatar.as_deref(), Some("https://launcher.way-of-elendil.fr/addons/av/b.png"));
        assert_eq!(c.entrees[2].avatar, None);
        assert_eq!(c.entrees[3].avatar.as_deref(), Some("https://cdn.test/d.jpg"));
    }
}
