//! Lancement du jeu : commande de `Wow.exe` (fonction pure), préparation (vider `Cache`, lire
//! `Config.wtf` sans écrire) et lancement détaché derrière un port. Le mot de passe ne vit que dans
//! un `Secret` ; la ligne de commande de `Wow.exe` n'est ni journalisée ni affichée.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::SystemTime;

use time::OffsetDateTime;

use crate::awesomewotlk::{self, DllAttendue, VERSION_EPINGLEE};
use crate::coffre::CoffreSecrets;
use crate::dossier_lancement;
use crate::comptes::{lister_comptes_jeu, trouver_compte};
use crate::config::{Dossiers, Royaume, creer_dossier_prive};
use crate::erreur::{Erreur, Result};
use crate::executeur::{Argument, Commande, nettoyer_environnement};
use crate::journal::{Journal, Niveau};
use crate::prefixe::environnement_lancement;
use crate::secret::Secret;

const ETAPE: &str = "lancement";

pub const PROGRAMME_WINE_DEFAUT: &str = "wine";
pub const EXECUTABLE_JEU: &str = "Wow.exe";
pub const OPTION_COMPTE: &str = "-login";
pub const OPTION_MOT_DE_PASSE: &str = "-password";
pub const OPTION_REALMLIST: &str = "-realmlist";
pub const OPTION_REALMNAME: &str = "-realmname";
pub const DOSSIER_CACHE: &str = "Cache";
pub const SOUS_CHEMIN_CONFIG_WTF: [&str; 2] = ["WTF", "Config.wtf"];
pub const MOT_REGLAGE: &str = "SET";
pub const MARQUE_COMMENTAIRE: &str = "//";
pub const CLE_LOCALE: &str = "locale";
pub const CLE_REALMLIST: &str = "realmlist";
pub const CLE_REALMNAME: &str = "realmname";
pub const CLE_PORTAIL: &str = "portal";
/// Premier identifiant rendu par le lanceur simulé.
pub const PID_SIMULE_INITIAL: u32 = 4242;

pub const ETAPE_COMPTE: &str = "compte résolu";
pub const ETAPE_SECRET: &str = "secret lu";
pub const ETAPE_CACHE: &str = "cache vidé";
pub const ETAPE_CONFIG: &str = "Config.wtf lu";
pub const ETAPE_ALERTE: &str = "fermeture rapide réglée";
pub const ETAPE_DOSSIER_LANCEMENT: &str = "dossier de lancement prêt";
pub const ETAPE_COMMANDE: &str = "commande construite";
pub const ETAPE_LANCE: &str = "jeu lancé";

// ---------- commande de jeu (pure) ----------

fn compte_valide(compte: &str) -> Result<()> {
    if compte.trim().is_empty() {
        return Err(Erreur::configuration("le nom du compte est vide"));
    }
    if compte.chars().any(char::is_control) {
        return Err(Erreur::configuration(format!(
            "le nom du compte {compte:?} contient un caractère de contrôle"
        )));
    }
    Ok(())
}

fn chemin_sans_remontee(chemin: &Path, objet: &str) -> Result<()> {
    if chemin.as_os_str().is_empty() {
        return Err(Erreur::Chemin { chemin: chemin.to_path_buf(), raison: format!("{objet} : chemin vide") });
    }
    if chemin.components().any(|c| c == Component::ParentDir) {
        return Err(Erreur::Chemin {
            chemin: chemin.to_path_buf(),
            raison: format!("{objet} : le chemin contient « .. »"),
        });
    }
    Ok(())
}

/// Valeur facultative d'une option du royaume : absente ou blanche = omise ; caractère de contrôle = refusée.
fn valeur_royaume<'a>(valeur: Option<&'a str>, objet: &str) -> Result<Option<&'a str>> {
    let Some(valeur) = valeur.filter(|v| !v.trim().is_empty()) else {
        return Ok(None);
    };
    if valeur.chars().any(char::is_control) {
        return Err(Erreur::configuration(format!("{objet} {valeur:?} contient un caractère de contrôle")));
    }
    Ok(Some(valeur))
}

/// Commande de `Wow.exe` sous Wine : fonction pure, sans élévation. Le secret devient un argument
/// secret (masqué par `Debug`, `Display` et `Commande::affichage`). Après `-password`, `-realmlist` et
/// `-realmname` ne sont ajoutés que s'ils sont connus et non vides.
pub fn commande_jeu(
    dossier_jeu: &Path,
    compte: &str,
    secret: Secret,
    prefixe: &Path,
    programme_wine: &str,
    realmlist: Option<&str>,
    realmname: Option<&str>,
) -> Result<Commande> {
    compte_valide(compte)?;
    let realmlist = valeur_royaume(realmlist, "le realmlist")?;
    let realmname = valeur_royaume(realmname, "le nom du royaume")?;
    if programme_wine.trim().is_empty() {
        return Err(Erreur::configuration("le programme Wine est vide"));
    }
    chemin_sans_remontee(dossier_jeu, "dossier du jeu")?;
    chemin_sans_remontee(prefixe, "préfixe Wine")?;
    let mut commande = Commande::new(programme_wine)
        .arg(EXECUTABLE_JEU)
        .arg(OPTION_COMPTE)
        .arg(compte)
        .arg(OPTION_MOT_DE_PASSE)
        .arg_secret(secret)
        .dans(dossier_jeu);
    if let Some(realmlist) = realmlist {
        commande = commande.arg(OPTION_REALMLIST).arg(realmlist);
    }
    if let Some(realmname) = realmname {
        commande = commande.arg(OPTION_REALMNAME).arg(realmname);
    }
    commande.environnement = environnement_lancement(prefixe);
    Ok(commande)
}

// ---------- Config.wtf (lecture seule) ----------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LigneMalformee {
    pub numero: usize,
    pub raison: String,
}

/// Réglages `SET clef "valeur"` de `Config.wtf`. Les clefs sont gardées en minuscules ; en cas de
/// répétition la dernière gagne et la répétition est comptée.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigWtf {
    pub reglages: BTreeMap<String, String>,
    pub lignes_malformees: Vec<LigneMalformee>,
    pub clefs_repetees: usize,
}

impl ConfigWtf {
    pub fn valeur(&self, clef: &str) -> Option<&str> {
        self.reglages.get(&clef.to_lowercase()).map(String::as_str)
    }

    pub fn locale(&self) -> Option<&str> {
        self.valeur(CLE_LOCALE)
    }

    pub fn realmlist(&self) -> Option<&str> {
        self.valeur(CLE_REALMLIST)
    }

    pub fn realm_name(&self) -> Option<&str> {
        self.valeur(CLE_REALMNAME)
    }

    pub fn portail(&self) -> Option<&str> {
        self.valeur(CLE_PORTAIL)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LectureWtf {
    Absent,
    Presente(ConfigWtf),
}

fn analyser_ligne(ligne: &str) -> std::result::Result<(String, String), String> {
    let mut morceaux = ligne.splitn(2, char::is_whitespace);
    let mot = morceaux.next().unwrap_or_default();
    if !mot.eq_ignore_ascii_case(MOT_REGLAGE) {
        return Err(format!("la ligne ne commence pas par {MOT_REGLAGE}"));
    }
    let reste = morceaux.next().unwrap_or_default().trim_start();
    let Some((clef, valeur)) = reste.split_once(char::is_whitespace) else {
        return Err("clef ou valeur manquante".to_string());
    };
    if clef.is_empty() {
        return Err("clef manquante".to_string());
    }
    let valeur = valeur.trim();
    let Some(sans_ouverture) = valeur.strip_prefix('"') else {
        return Err("valeur sans guillemet ouvrant".to_string());
    };
    let Some(fin) = sans_ouverture.rfind('"') else {
        return Err("valeur sans guillemet fermant".to_string());
    };
    let apres = sans_ouverture[fin + 1..].trim();
    if !apres.is_empty() && !apres.starts_with(MARQUE_COMMENTAIRE) {
        return Err("texte après le guillemet fermant".to_string());
    }
    Ok((clef.to_lowercase(), sans_ouverture[..fin].to_string()))
}

/// Analyse le texte de `Config.wtf` (BOM, CRLF, lignes vides, valeurs vides tolérés). Une ligne
/// malformée est comptée avec son numéro et sa raison, jamais son contenu.
pub fn analyser_config_wtf(texte: &str) -> ConfigWtf {
    let texte = texte.strip_prefix('\u{feff}').unwrap_or(texte);
    let mut config = ConfigWtf::default();
    for (index, brute) in texte.lines().enumerate() {
        let ligne = brute.trim();
        if ligne.is_empty() || ligne.starts_with(MARQUE_COMMENTAIRE) {
            continue;
        }
        match analyser_ligne(ligne) {
            Ok((clef, valeur)) => {
                if config.reglages.insert(clef, valeur).is_some() {
                    config.clefs_repetees += 1;
                }
            }
            Err(raison) => config.lignes_malformees.push(LigneMalformee { numero: index + 1, raison }),
        }
    }
    config
}

pub fn chemin_config_wtf(dossier_jeu: &Path) -> PathBuf {
    SOUS_CHEMIN_CONFIG_WTF.iter().fold(dossier_jeu.to_path_buf(), |chemin, partie| chemin.join(partie))
}

/// Lit `<jeu>/WTF/Config.wtf` sans jamais écrire. Fichier absent = `Absent` (journalisé), pas une erreur.
pub fn lire_config_wtf(dossier_jeu: &Path, journal: &Journal) -> Result<LectureWtf> {
    let chemin = chemin_config_wtf(dossier_jeu);
    let octets = match fs::read(&chemin) {
        Ok(octets) => octets,
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => {
            journaliser(journal, Niveau::Info, "Config.wtf absent : rien à lire", dossier_jeu, None, &[])?;
            return Ok(LectureWtf::Absent);
        }
        Err(erreur) => {
            let erreur = Erreur::io("lire Config.wtf", &chemin, erreur);
            journaliser_echec(journal, &erreur, dossier_jeu, None, &[])?;
            return Err(erreur);
        }
    };
    let texte = match String::from_utf8(octets) {
        Ok(texte) => texte,
        Err(raison) => {
            let erreur = Erreur::analyse(&chemin.display().to_string(), format!("texte non UTF-8 ({raison})"));
            journaliser_echec(journal, &erreur, dossier_jeu, None, &[])?;
            return Err(erreur);
        }
    };
    let config = analyser_config_wtf(&texte);
    let message = format!(
        "Config.wtf lu : {} réglage(s), {} ligne(s) malformée(s), {} clef(s) répétée(s)",
        config.reglages.len(),
        config.lignes_malformees.len(),
        config.clefs_repetees
    );
    let niveau = if config.lignes_malformees.is_empty() { Niveau::Info } else { Niveau::Avertissement };
    journaliser(journal, niveau, &message, dossier_jeu, None, &[])?;
    for ligne in &config.lignes_malformees {
        let detail = format!("Config.wtf, ligne {} malformée : {}", ligne.numero, ligne.raison);
        journaliser(journal, Niveau::Avertissement, &detail, dossier_jeu, None, &[])?;
    }
    Ok(LectureWtf::Presente(config))
}

// ---------- réglage de fermeture rapide (serverAlert) ----------

pub const CLE_ALERTE_SERVEUR: &str = "serveralert";
pub const LIGNE_ALERTE_VIDE: &str = "SET serverAlert \"\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReglageFermeture {
    DejaEnPlace,
    /// Ligne ajoutée ou corrigée (dans le plan à blanc : serait ajoutée ou corrigée).
    Ajoute,
    /// `Config.wtf` créé (dans le plan à blanc : serait créé).
    Cree,
    NonEcrit,
}

fn est_alerte_serveur(ligne: &str) -> bool {
    matches!(analyser_ligne(ligne.trim()), Ok((clef, _)) if clef == CLE_ALERTE_SERVEUR)
}

fn est_alerte_vide(ligne: &str) -> bool {
    matches!(analyser_ligne(ligne.trim()), Ok((clef, valeur)) if clef == CLE_ALERTE_SERVEUR && valeur.is_empty())
}

/// Contenu de `Config.wtf` avec `SET serverAlert ""`, ou `None` si rien n'est à changer. Le reste du
/// texte (BOM, fins de ligne, lignes malformées) est conservé tel quel.
pub fn alerte_vide_dans(texte: &str) -> Option<String> {
    let fin_de_ligne = if texte.contains("\r\n") { "\r\n" } else { "\n" };
    let mut trouvee = false;
    let mut change = false;
    let mut sortie = String::with_capacity(texte.len() + LIGNE_ALERTE_VIDE.len() + 2);
    for (index, segment) in texte.split_inclusive('\n').enumerate() {
        let corps = segment.trim_end_matches(['\n', '\r']);
        let fin = &segment[corps.len()..];
        let (bom, utile) = match (index, corps.strip_prefix('\u{feff}')) {
            (0, Some(reste)) => ("\u{feff}", reste),
            _ => ("", corps),
        };
        if est_alerte_serveur(utile) {
            trouvee = true;
            if est_alerte_vide(utile) {
                sortie.push_str(segment);
            } else {
                change = true;
                sortie.push_str(bom);
                sortie.push_str(LIGNE_ALERTE_VIDE);
                sortie.push_str(fin);
            }
        } else {
            sortie.push_str(segment);
        }
    }
    if trouvee {
        return change.then_some(sortie);
    }
    if !sortie.is_empty() && !sortie.ends_with('\n') {
        sortie.push_str(fin_de_ligne);
    }
    sortie.push_str(LIGNE_ALERTE_VIDE);
    sortie.push_str(fin_de_ligne);
    Some(sortie)
}

fn refus_reglage(chemin: &Path, raison: &str) -> Erreur {
    Erreur::Chemin { chemin: chemin.to_path_buf(), raison: raison.to_string() }
}

/// Ce qu'il faut faire de `Config.wtf` : le genre de changement et le contenu complet à écrire.
fn examiner_alerte(dossier_jeu: &Path) -> Result<(ReglageFermeture, Option<String>)> {
    let wtf = dossier_jeu.join(SOUS_CHEMIN_CONFIG_WTF[0]);
    let chemin = chemin_config_wtf(dossier_jeu);
    match fs::symlink_metadata(&wtf) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(refus_reglage(&wtf, "le dossier WTF est un lien symbolique"));
        }
        Ok(meta) if !meta.is_dir() => return Err(refus_reglage(&wtf, "WTF n'est pas un dossier")),
        Ok(_) => {}
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => {
            return Ok((ReglageFermeture::Cree, alerte_vide_dans("")));
        }
        Err(erreur) => return Err(Erreur::io("examiner le dossier WTF", &wtf, erreur)),
    }
    match fs::symlink_metadata(&chemin) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(refus_reglage(&chemin, "Config.wtf est un lien symbolique"));
        }
        Ok(meta) if !meta.is_file() => return Err(refus_reglage(&chemin, "Config.wtf n'est pas un fichier")),
        Ok(_) => {}
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => {
            return Ok((ReglageFermeture::Cree, alerte_vide_dans("")));
        }
        Err(erreur) => return Err(Erreur::io("examiner Config.wtf", &chemin, erreur)),
    }
    let octets = fs::read(&chemin).map_err(|e| Erreur::io("lire Config.wtf", &chemin, e))?;
    let texte = String::from_utf8(octets)
        .map_err(|raison| Erreur::analyse(&chemin.display().to_string(), format!("texte non UTF-8 ({raison})")))?;
    Ok(match alerte_vide_dans(&texte) {
        Some(neuf) => (ReglageFermeture::Ajoute, Some(neuf)),
        None => (ReglageFermeture::DejaEnPlace, None),
    })
}

/// Écrit `contenu` dans un fichier voisin puis le renomme : jamais de `Config.wtf` à moitié écrit.
fn ecrire_atomique(chemin: &Path, contenu: &str) -> Result<()> {
    let dossier = chemin.parent().ok_or_else(|| refus_reglage(chemin, "pas de dossier parent"))?;
    fs::create_dir_all(dossier).map_err(|e| Erreur::io("créer le dossier WTF", dossier, e))?;
    let temporaire = dossier.join(format!("{}.ebonhux-tmp", SOUS_CHEMIN_CONFIG_WTF[1]));
    let resultat = ecrire_puis_renommer(chemin, &temporaire, contenu);
    if resultat.is_err() && let Err(erreur) = fs::remove_file(&temporaire) && erreur.kind() != ErrorKind::NotFound {
        return Err(Erreur::io("retirer le fichier temporaire de Config.wtf", &temporaire, erreur));
    }
    resultat
}

fn ecrire_puis_renommer(chemin: &Path, temporaire: &Path, contenu: &str) -> Result<()> {
    let mut fichier = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporaire)
        .map_err(|e| Erreur::io("créer le fichier temporaire de Config.wtf", temporaire, e))?;
    fichier.write_all(contenu.as_bytes()).map_err(|e| Erreur::io("écrire Config.wtf", temporaire, e))?;
    fichier.sync_all().map_err(|e| Erreur::io("terminer l'écriture de Config.wtf", temporaire, e))?;
    if let Ok(meta) = fs::metadata(chemin) {
        fs::set_permissions(temporaire, meta.permissions())
            .map_err(|e| Erreur::io("copier les droits de Config.wtf", temporaire, e))?;
    }
    fs::rename(temporaire, chemin).map_err(|e| Erreur::io("remplacer Config.wtf", chemin, e))
}

fn avertir_reglage_non_ecrit(dossier_jeu: &Path, journal: &Journal, erreur: &Erreur) -> Result<ReglageFermeture> {
    let chemin = chemin_config_wtf(dossier_jeu);
    let message = format!("Réglage de fermeture rapide non écrit : {erreur} ({})", chemin.display());
    journaliser(journal, Niveau::Avertissement, &message, dossier_jeu, None, &[])?;
    Ok(ReglageFermeture::NonEcrit)
}

/// Met `SET serverAlert ""` dans `<jeu>/WTF/Config.wtf`. Un refus ou une erreur d'écriture est
/// journalisé en avertissement : le lancement continue sans ce réglage.
pub fn ecrire_alerte_vide(dossier_jeu: &Path, journal: &Journal) -> Result<ReglageFermeture> {
    let chemin = chemin_config_wtf(dossier_jeu);
    let resultat = examiner_alerte(dossier_jeu).and_then(|(genre, neuf)| match neuf {
        Some(contenu) => ecrire_atomique(&chemin, &contenu).map(|()| genre),
        None => Ok(genre),
    });
    let genre = match resultat {
        Ok(genre) => genre,
        Err(erreur) => return avertir_reglage_non_ecrit(dossier_jeu, journal, &erreur),
    };
    let message = match genre {
        ReglageFermeture::Cree => "Réglage de fermeture rapide ajouté à Config.wtf (fichier créé)",
        ReglageFermeture::DejaEnPlace => "Réglage de fermeture rapide déjà en place dans Config.wtf",
        ReglageFermeture::Ajoute | ReglageFermeture::NonEcrit => "Réglage de fermeture rapide ajouté à Config.wtf",
    };
    journaliser(journal, Niveau::Info, message, dossier_jeu, None, &[])?;
    Ok(genre)
}

/// Comme `ecrire_alerte_vide` mais sans rien écrire : dit ce qui serait fait.
pub fn prevoir_alerte_vide(dossier_jeu: &Path, journal: &Journal) -> Result<ReglageFermeture> {
    match examiner_alerte(dossier_jeu) {
        Ok((genre, _)) => Ok(genre),
        Err(erreur) => avertir_reglage_non_ecrit(dossier_jeu, journal, &erreur),
    }
}

// ---------- dossier Cache ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatCache {
    Absent,
    Vide { retirees: usize },
    /// Le vidage n'a pas eu lieu (`vider_cache` à faux) : un autre compte utilise peut-être le dossier.
    Conserve,
}

/// Ligne d'information journalisée quand le dossier `Cache` est volontairement laissé en place.
pub const MESSAGE_CACHE_CONSERVE: &str = "Cache conservé : un autre compte est lancé";

pub fn chemin_cache(dossier_jeu: &Path) -> PathBuf {
    dossier_jeu.join(DOSSIER_CACHE)
}

fn refus_cache(chemin: &Path, raison: &str) -> Erreur {
    Erreur::Chemin { chemin: chemin.to_path_buf(), raison: raison.to_string() }
}

/// `Some(chemin)` si `<jeu>/Cache` est un vrai dossier situé exactement sous le dossier du jeu.
fn cache_verifie(dossier_jeu: &Path) -> Result<Option<PathBuf>> {
    chemin_sans_remontee(dossier_jeu, "dossier du jeu")?;
    let jeu_reel =
        fs::canonicalize(dossier_jeu).map_err(|e| Erreur::io("résoudre le dossier du jeu", dossier_jeu, e))?;
    let cache = chemin_cache(dossier_jeu);
    let meta = match fs::symlink_metadata(&cache) {
        Ok(meta) => meta,
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => return Ok(None),
        Err(erreur) => return Err(Erreur::io("examiner le cache", &cache, erreur)),
    };
    if meta.file_type().is_symlink() {
        return Err(refus_cache(&cache, "le cache est un lien symbolique : rien n'est touché"));
    }
    if !meta.is_dir() {
        return Err(refus_cache(&cache, "le cache n'est pas un dossier : rien n'est touché"));
    }
    let reel = fs::canonicalize(&cache).map_err(|e| Erreur::io("résoudre le cache", &cache, e))?;
    let nom_ok = reel.file_name().and_then(|n| n.to_str()) == Some(DOSSIER_CACHE);
    if reel.parent() != Some(jeu_reel.as_path()) || !nom_ok {
        return Err(refus_cache(&cache, "le cache sort du dossier du jeu : rien n'est touché"));
    }
    Ok(Some(cache))
}

fn entrees_cache(cache: &Path) -> Result<Vec<(PathBuf, bool)>> {
    let lecture = fs::read_dir(cache).map_err(|e| Erreur::io("lire le cache", cache, e))?;
    let mut entrees = Vec::new();
    for entree in lecture {
        let entree = entree.map_err(|e| Erreur::io("lire une entrée du cache", cache, e))?;
        let chemin = entree.path();
        let type_fichier = entree.file_type().map_err(|e| Erreur::io("lire le type de l'entrée", &chemin, e))?;
        // Un lien (même vers un dossier) est retiré comme un fichier : sa cible n'est jamais suivie.
        entrees.push((chemin, type_fichier.is_dir()));
    }
    entrees.sort();
    Ok(entrees)
}

/// Nombre d'entrées que `vider_cache` retirerait (`None` si `Cache` est absent). N'écrit rien.
pub fn compter_cache(dossier_jeu: &Path) -> Result<Option<usize>> {
    match cache_verifie(dossier_jeu)? {
        Some(cache) => Ok(Some(entrees_cache(&cache)?.len())),
        None => Ok(None),
    }
}

/// Vide `<jeu>/Cache` (le dossier lui-même est gardé). Rien d'autre n'est touché.
pub fn vider_cache(dossier_jeu: &Path, journal: &Journal) -> Result<ResultatCache> {
    vider_cache_suivi(dossier_jeu, journal, None, &[])
}

fn retirer_entrees(dossier_jeu: &Path) -> Result<ResultatCache> {
    let Some(cache) = cache_verifie(dossier_jeu)? else {
        return Ok(ResultatCache::Absent);
    };
    let mut retirees = 0;
    for (chemin, est_dossier) in entrees_cache(&cache)? {
        let retrait = if est_dossier { fs::remove_dir_all(&chemin) } else { fs::remove_file(&chemin) };
        retrait.map_err(|e| Erreur::io("retirer l'entrée du cache", &chemin, e))?;
        retirees += 1;
    }
    Ok(ResultatCache::Vide { retirees })
}

fn vider_cache_suivi(
    dossier_jeu: &Path,
    journal: &Journal,
    compte: Option<&str>,
    faites: &[&str],
) -> Result<ResultatCache> {
    let resultat = retirer_entrees(dossier_jeu);
    match &resultat {
        Ok(ResultatCache::Absent) => {
            journaliser(journal, Niveau::Info, "Cache absent : rien à vider", dossier_jeu, compte, faites)?;
        }
        Ok(ResultatCache::Vide { retirees }) => {
            let message = format!("Cache vidé : {retirees} entrée(s) retirée(s)");
            journaliser(journal, Niveau::Info, &message, dossier_jeu, compte, faites)?;
        }
        Ok(ResultatCache::Conserve) => {
            journaliser(journal, Niveau::Info, MESSAGE_CACHE_CONSERVE, dossier_jeu, compte, faites)?;
        }
        Err(erreur) => journaliser_echec(journal, erreur, dossier_jeu, compte, faites)?,
    }
    resultat
}

// ---------- journal ----------

fn contexte(dossier_jeu: &Path, compte: Option<&str>, faites: &[&str]) -> BTreeMap<String, String> {
    let mut contexte = BTreeMap::from([("dossier_jeu".to_string(), dossier_jeu.display().to_string())]);
    if let Some(compte) = compte {
        contexte.insert("compte".to_string(), compte.to_string());
    }
    contexte.insert("etapes_faites".to_string(), faites.join(", "));
    contexte
}

fn journaliser(
    journal: &Journal,
    niveau: Niveau,
    message: &str,
    dossier_jeu: &Path,
    compte: Option<&str>,
    faites: &[&str],
) -> Result<()> {
    journal.evenement(niveau, ETAPE, message, &contexte(dossier_jeu, compte, faites), None)
}

fn journaliser_echec(
    journal: &Journal,
    erreur: &Erreur,
    dossier_jeu: &Path,
    compte: Option<&str>,
    faites: &[&str],
) -> Result<()> {
    journal.evenement(
        Niveau::Erreur,
        ETAPE,
        &erreur.to_string(),
        &contexte(dossier_jeu, compte, faites),
        Some(format!("{erreur:?}")),
    )
}

// ---------- port de lancement détaché ----------

const MODE_FICHIER_SORTIE: u32 = 0o600;

/// Lance un processus sans l'attendre et rend son identifiant.
pub trait LanceurDetache {
    fn lancer(&self, commande: &Commande) -> Result<u32>;
}

/// Lanceur réel : le processus quitte le groupe de processus du launcher (il survit à un Ctrl-C du
/// terminal), n'a pas d'entrée, et sa sortie va au néant ou à un fichier de journal de Wine.
/// Ce fichier (mode 0600) ne doit jamais recevoir un `WINEDEBUG` verbeux de type processus : il
/// pourrait y consigner la ligne de commande du jeu, donc le mot de passe.
#[derive(Default)]
pub struct LanceurReel {
    fichier_sortie: Option<PathBuf>,
    journal: Option<Arc<Journal>>,
}

impl fmt::Debug for LanceurReel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanceurReel")
            .field("fichier_sortie", &self.fichier_sortie)
            .field("journal", &self.journal.is_some())
            .finish()
    }
}

const SOUS_DOSSIER_ERREURS: &str = "Errors";
const SUFFIXE_CRASH: &str = "Crash.txt";
const PREFIXES_ENTETE_CRASH: [&str; 2] = ["ERROR #", "Exception:"];

/// `wine-AAAAMMJJ.log`, dans le dossier du journal.
pub fn chemin_sortie_wine(dossiers: &Dossiers, instant: &OffsetDateTime) -> PathBuf {
    let nom = format!(
        "wine-{:04}{:02}{:02}.log",
        instant.year(),
        u8::from(instant.month()),
        instant.day()
    );
    dossiers.dossier_journal().join(nom)
}

/// Lignes « ERROR #… » et « Exception: … » d'un texte de `Crash.txt`.
pub fn en_tete_crash(texte: &str) -> Vec<String> {
    texte
        .lines()
        .map(str::trim_end)
        .filter(|ligne| PREFIXES_ENTETE_CRASH.iter().any(|prefixe| ligne.starts_with(prefixe)))
        .map(str::to_string)
        .collect()
}

/// Noms de `*Crash.txt` dont la date de modification est postérieure ou égale à `depuis`.
pub fn noms_crash_nouveaux(fichiers: &[(String, SystemTime)], depuis: SystemTime) -> Vec<String> {
    fichiers
        .iter()
        .filter(|(nom, date)| nom.ends_with(SUFFIXE_CRASH) && *date >= depuis)
        .map(|(nom, _)| nom.clone())
        .collect()
}

/// Niveau et message du journal à la fin du jeu (`None` : tué par un signal).
pub fn message_fin(code: Option<i32>) -> (Niveau, String) {
    match code {
        Some(0) => (Niveau::Info, "Le jeu s'est arrêté (code 0)".to_string()),
        Some(code) => (Niveau::Avertissement, format!("Le jeu s'est arrêté (code {code})")),
        None => (Niveau::Avertissement, "Le jeu s'est arrêté (interrompu par un signal)".to_string()),
    }
}

/// Les `Errors/*Crash.txt` apparus depuis `depuis`, avec leur en-tête ; illisible ou absent : ignoré.
pub fn crashs_depuis(dossier_jeu: &Path, depuis: SystemTime) -> Vec<(PathBuf, Vec<String>)> {
    let dossier = dossier_jeu.join(SOUS_DOSSIER_ERREURS);
    let Ok(entrees) = fs::read_dir(&dossier) else {
        return Vec::new();
    };
    let fichiers: Vec<(String, SystemTime)> = entrees
        .flatten()
        .filter_map(|entree| {
            let date = entree.metadata().ok()?.modified().ok()?;
            Some((entree.file_name().to_string_lossy().into_owned(), date))
        })
        .collect();
    noms_crash_nouveaux(&fichiers, depuis)
        .into_iter()
        .map(|nom| {
            let chemin = dossier.join(nom);
            let entete = fs::read(&chemin).map(|octets| en_tete_crash(&String::from_utf8_lossy(&octets)));
            (chemin, entete.unwrap_or_default())
        })
        .collect()
}

fn journaliser_fin(journal: &Journal, code: Option<i32>, dossier_jeu: Option<&Path>, depuis: SystemTime) -> Result<()> {
    let (niveau, message) = message_fin(code);
    journal.evenement(niveau, ETAPE, &message, &BTreeMap::new(), None)?;
    let Some(dossier_jeu) = dossier_jeu else {
        return Ok(());
    };
    for (chemin, entete) in crashs_depuis(dossier_jeu, depuis) {
        let message = format!("Le jeu a planté : {} ({})", chemin.display(), entete.join(" ; "));
        journal.evenement(Niveau::Erreur, ETAPE, &message, &BTreeMap::new(), None)?;
    }
    Ok(())
}

impl LanceurReel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lanceur de production : sortie de Wine dans `wine-AAAAMMJJ.log` à côté du journal, fin du jeu
    /// et plantages inscrits au journal.
    pub fn pour_production(dossiers: &Dossiers, journal: Arc<Journal>) -> Self {
        let chemin = chemin_sortie_wine(dossiers, &OffsetDateTime::now_utc());
        // Le dossier du journal est créé ici ; si cela échoue, l'ouverture du fichier au lancement
        // rend l'erreur, avec le chemin.
        let _ = creer_dossier_prive(&dossiers.dossier_journal(), "créer le dossier du journal");
        Self { fichier_sortie: Some(chemin), journal: Some(journal) }
    }

    pub fn fichier_sortie(&self) -> Option<&Path> {
        self.fichier_sortie.as_deref()
    }

    /// La fin du jeu (et ses fichiers `Crash.txt`) est inscrite à ce journal.
    pub fn avec_journal(mut self, journal: Arc<Journal>) -> Self {
        self.journal = Some(journal);
        self
    }

    /// La sortie standard et d'erreur de Wine est ajoutée à ce fichier (jamais la commande).
    pub fn avec_fichier_sortie(mut self, chemin: impl Into<PathBuf>) -> Self {
        self.fichier_sortie = Some(chemin.into());
        self
    }

    fn flux(&self) -> Result<(Stdio, Stdio)> {
        let Some(chemin) = &self.fichier_sortie else {
            return Ok((Stdio::null(), Stdio::null()));
        };
        let ouvrir = || OpenOptions::new().create(true).append(true).mode(MODE_FICHIER_SORTIE).open(chemin);
        let sortie = ouvrir().map_err(|e| Erreur::io("ouvrir le fichier de sortie de Wine", chemin, e))?;
        let erreurs = ouvrir().map_err(|e| Erreur::io("ouvrir le fichier de sortie de Wine", chemin, e))?;
        // `mode` ne joue qu'à la création : un fichier existant plus ouvert est resserré.
        sortie
            .set_permissions(fs::Permissions::from_mode(MODE_FICHIER_SORTIE))
            .map_err(|e| Erreur::io("restreindre les droits du fichier de sortie de Wine", chemin, e))?;
        Ok((Stdio::from(sortie), Stdio::from(erreurs)))
    }
}

impl LanceurDetache for LanceurReel {
    fn lancer(&self, commande: &Commande) -> Result<u32> {
        if commande.elevation {
            return Err(Erreur::Elevation {
                raison: "le lancement du jeu ne passe jamais par une élévation de droits".to_string(),
            });
        }
        let (sortie, erreurs) = self.flux()?;
        let mut processus = std::process::Command::new(&commande.programme);
        nettoyer_environnement(&mut processus);
        processus.envs(&commande.environnement);
        if let Some(repertoire) = &commande.repertoire {
            processus.current_dir(repertoire);
        }
        for argument in &commande.arguments {
            match argument {
                Argument::Public(texte) => processus.arg(texte),
                Argument::Secret(secret) => processus.arg(secret.exposer()),
            };
        }
        processus.stdin(Stdio::null()).stdout(sortie).stderr(erreurs).process_group(0);
        // Les messages d'erreur du système ne citent pas les arguments : la commande n'y figure pas.
        let mut enfant = processus.spawn().map_err(|e| Erreur::Commande {
            programme: commande.programme.clone(),
            code: None,
            sortie: format!("impossible de lancer le programme : {e}"),
        })?;
        let pid = enfant.id();
        let programme = commande.programme.clone();
        let journal = self.journal.clone();
        let dossier_jeu = commande.repertoire.clone();
        let depuis = SystemTime::now();
        thread::spawn(move || match enfant.wait() {
            Err(erreur) => eprintln!("Attente du processus détaché « {programme} » impossible : {erreur}"),
            Ok(statut) => {
                let Some(journal) = journal else {
                    return;
                };
                if let Err(erreur) = journaliser_fin(&journal, statut.code(), dossier_jeu.as_deref(), depuis) {
                    eprintln!("Fin du jeu non journalisée : {erreur}");
                }
            }
        });
        Ok(pid)
    }
}

/// Ce que le lanceur simulé a reçu. Le mot de passe y est un `Secret` (masqué partout).
pub struct LancementRecu {
    pub programme: String,
    pub arguments: Vec<String>,
    pub secrets: Vec<Secret>,
    pub environnement: BTreeMap<String, String>,
    pub repertoire: Option<PathBuf>,
    pub elevation: bool,
}

impl fmt::Debug for LancementRecu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LancementRecu")
            .field("programme", &self.programme)
            .field("arguments", &self.arguments.len())
            .field("secrets", &self.secrets.len())
            .finish_non_exhaustive()
    }
}

/// Lanceur pour les essais : enregistre la commande reçue, ne lance rien.
#[derive(Default)]
pub struct LanceurSimule {
    recus: Mutex<Vec<LancementRecu>>,
    raison_echec: Option<String>,
}

impl LanceurSimule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lanceur dont tout lancement échoue avec cette raison.
    pub fn qui_echoue(raison: &str) -> Self {
        Self { recus: Mutex::new(Vec::new()), raison_echec: Some(raison.to_string()) }
    }

    pub fn nombre_lancements(&self) -> usize {
        self.recus.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// Visite les lancements reçus sans les copier.
    pub fn examiner<T>(&self, visiteur: impl FnOnce(&[LancementRecu]) -> T) -> T {
        visiteur(&self.recus.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl LanceurDetache for LanceurSimule {
    fn lancer(&self, commande: &Commande) -> Result<u32> {
        if let Some(raison) = &self.raison_echec {
            return Err(Erreur::Commande {
                programme: commande.programme.clone(),
                code: None,
                sortie: raison.clone(),
            });
        }
        let mut recus = self.recus.lock().unwrap_or_else(PoisonError::into_inner);
        let secrets = commande
            .arguments
            .iter()
            .filter_map(|a| match a {
                Argument::Secret(secret) => Some(Secret::new(secret.exposer().to_string())),
                Argument::Public(_) => None,
            })
            .collect();
        recus.push(LancementRecu {
            programme: commande.programme.clone(),
            arguments: commande.arguments.iter().map(|a| a.affichage().to_string()).collect(),
            secrets,
            environnement: commande.environnement.clone(),
            repertoire: commande.repertoire.clone(),
            elevation: commande.elevation,
        });
        let decalage = u32::try_from(recus.len() - 1).unwrap_or(0);
        Ok(PID_SIMULE_INITIAL + decalage)
    }
}

// ---------- orchestration ----------

#[derive(Debug, Clone, Copy)]
pub struct ParametresLancement<'a> {
    pub dossier_jeu: &'a Path,
    pub compte: &'a str,
    pub prefixe: &'a Path,
    pub programme_wine: &'a str,
    /// Royaume saisi en configuration : passé au jeu (`-realmlist`, `-realmname`), champ par champ, avant
    /// `Config.wtf` ; sert aussi à signaler un écart de realmlist avec `Config.wtf`.
    pub royaume: Option<&'a Royaume>,
    /// Faux quand un autre compte est déjà lancé : son jeu utilise le dossier `Cache`, qu'on ne vide pas.
    pub vider_cache: bool,
    /// Vrai quand l'appelant a déjà validé le compte (compte ajouté depuis le launcher : le jeu crée
    /// son dossier `WTF/Account/<nom>` à la première connexion). La ligne de commande laisse faux.
    pub compte_deja_valide: bool,
    /// Racine des données du launcher : on y trouve la DLL AwesomeWotlk et le dossier de lancement.
    pub donnees: &'a Path,
    /// DLL AwesomeWotlk attendue (nom, somme) : la v37 en production, une DLL factice dans les essais.
    pub awesomewotlk: DllAttendue<'a>,
}

/// État après la préparation, prêt à lancer. Pas de `Debug` dérivé : la commande n'est jamais montrée.
pub struct Preparation {
    pub compte: String,
    pub cache: ResultatCache,
    pub config_wtf: LectureWtf,
    commande: Commande,
    faites: Vec<&'static str>,
    dossier_jeu: PathBuf,
}

impl fmt::Debug for Preparation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Preparation")
            .field("compte", &self.compte)
            .field("cache", &self.cache)
            .field("config_wtf", &self.config_wtf)
            .finish_non_exhaustive()
    }
}

/// Ce que la ligne de commande affiche après un lancement : jamais la commande ni le secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatLancement {
    pub pid: u32,
    pub compte: String,
    pub cache: ResultatCache,
    pub config_wtf: LectureWtf,
    pub etapes: Vec<String>,
}

impl ResultatLancement {
    pub fn resume(&self) -> String {
        format!("Jeu lancé pour le compte « {} » (processus {}).", self.compte, self.pid)
    }
}

/// Plan « à blanc » : ce qui se passerait, sans le secret et sans la commande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLancement {
    pub compte: String,
    pub dossier_jeu: PathBuf,
    pub prefixe: PathBuf,
    pub programme_wine: String,
    pub secret_present: bool,
    /// Entrées que le vidage retirerait ; `None` si `Cache` est absent.
    pub entrees_cache: Option<usize>,
    pub config_wtf: LectureWtf,
    pub reglage_fermeture: ReglageFermeture,
    /// Dossier d'où le jeu serait lancé (rien n'y est écrit à blanc).
    pub dossier_lancement: PathBuf,
    /// Vrai si la DLL AwesomeWotlk attendue est déjà là, avec la bonne somme.
    pub awesomewotlk_prete: bool,
}

struct Suivi<'a> {
    journal: &'a Journal,
    dossier_jeu: &'a Path,
    compte: Option<String>,
    faites: Vec<&'static str>,
}

impl Suivi<'_> {
    fn reussi(&mut self, etape: &'static str, message: &str) -> Result<()> {
        self.faites.push(etape);
        journaliser(self.journal, Niveau::Info, message, self.dossier_jeu, self.compte.as_deref(), &self.faites)
    }

    /// Journalise l'échec avec les étapes déjà faites ; si le journal échoue lui-même, c'est son erreur qui sort.
    fn echec(&self, erreur: Erreur) -> Erreur {
        match journaliser_echec(self.journal, &erreur, self.dossier_jeu, self.compte.as_deref(), &self.faites) {
            Ok(()) => erreur,
            Err(erreur_journal) => erreur_journal,
        }
    }
}

fn resoudre_compte(params: &ParametresLancement<'_>, suivi: &mut Suivi<'_>) -> Result<String> {
    compte_valide(params.compte).map_err(|e| suivi.echec(e))?;
    let comptes = lister_comptes_jeu(params.dossier_jeu, suivi.journal).map_err(|e| suivi.echec(e))?;
    let Some(trouve) = trouver_compte(params.compte, &comptes) else {
        if params.compte_deja_valide {
            let nom = params.compte.to_string();
            suivi.compte = Some(nom.clone());
            suivi.reussi(ETAPE_COMPTE, "Compte résolu (sans dossier dans WTF/Account : créé par le jeu à la première connexion)")?;
            return Ok(nom);
        }
        let erreur = Erreur::configuration(format!(
            "le compte « {} » n'existe pas dans le dossier des comptes du jeu (WTF/Account)",
            params.compte
        ));
        return Err(suivi.echec(erreur));
    };
    let nom = trouve.nom.clone();
    suivi.compte = Some(nom.clone());
    suivi.reussi(ETAPE_COMPTE, "Compte résolu")?;
    Ok(nom)
}

fn lire_secret(compte: &str, coffre: &dyn CoffreSecrets, suivi: &mut Suivi<'_>) -> Result<Secret> {
    let secret = coffre.lire(compte).map_err(|e| suivi.echec(e))?;
    match secret {
        Some(secret) if !secret.est_vide() => {
            suivi.reussi(ETAPE_SECRET, "Secret du compte trouvé dans le coffre")?;
            Ok(secret)
        }
        _ => Err(suivi.echec(Erreur::Coffre {
            raison: format!("aucun mot de passe enregistré pour le compte « {compte} »"),
        })),
    }
}

fn signaler_ecart_realmlist(params: &ParametresLancement<'_>, wtf: &LectureWtf, suivi: &Suivi<'_>) -> Result<()> {
    let (Some(royaume), LectureWtf::Presente(config)) = (params.royaume, wtf) else {
        return Ok(());
    };
    match config.realmlist() {
        Some(lu) if lu.eq_ignore_ascii_case(&royaume.realmlist) => Ok(()),
        autre => {
            let message = format!(
                "Config.wtf : realmlist {} alors que la configuration attend « {} » (le jeu n'est pas modifié)",
                autre.map_or("absent".to_string(), |v| format!("« {v} »")),
                royaume.realmlist
            );
            journaliser(
                suivi.journal,
                Niveau::Avertissement,
                &message,
                suivi.dossier_jeu,
                suivi.compte.as_deref(),
                &suivi.faites,
            )
        }
    }
}

/// Commande du jeu avec le royaume de la configuration, à défaut celui de `Config.wtf` (champ par champ).
fn commande_pour(
    params: &ParametresLancement<'_>,
    depart: &Path,
    compte: &str,
    secret: Secret,
    wtf: &LectureWtf,
) -> Result<Commande> {
    let non_vide = |texte: &'_ str| (!texte.trim().is_empty()).then_some(texte.to_string());
    let configuration = params.royaume;
    let lu = match wtf {
        LectureWtf::Presente(config) => Some(config),
        LectureWtf::Absent => None,
    };
    let realmlist = configuration
        .and_then(|r| non_vide(&r.realmlist))
        .or_else(|| lu.and_then(|c| c.realmlist()).and_then(non_vide));
    let realmname = configuration
        .and_then(|r| non_vide(&r.nom))
        .or_else(|| lu.and_then(|c| c.realm_name()).and_then(non_vide));
    commande_jeu(
        depart,
        compte,
        secret,
        params.prefixe,
        params.programme_wine,
        realmlist.as_deref(),
        realmname.as_deref(),
    )
}

const MESSAGE_DLL_ABSENTE: &str =
    "La connexion automatique (AwesomeWotlk v37) n'est pas installée : relancez la préparation du jeu.";

/// Chemin de la DLL attendue dans les données du launcher.
fn chemin_dll_attendue(params: &ParametresLancement<'_>) -> PathBuf {
    awesomewotlk::chemin_dll(params.donnees, VERSION_EPINGLEE, &params.awesomewotlk)
}

/// Résout le compte, lit le secret, vérifie la DLL v37, vide `Cache`, lit `Config.wtf`, met à jour le
/// dossier de lancement, construit la commande (lancée depuis ce dossier, jamais depuis le jeu).
pub fn preparer(
    params: &ParametresLancement<'_>,
    coffre: &dyn CoffreSecrets,
    journal: &Journal,
) -> Result<Preparation> {
    let mut suivi = Suivi { journal, dossier_jeu: params.dossier_jeu, compte: None, faites: Vec::new() };
    let compte = resoudre_compte(params, &mut suivi)?;
    let secret = lire_secret(params.compte, coffre, &mut suivi)?;
    let chemin_dll = chemin_dll_attendue(params);
    if !awesomewotlk::dll_prete(&chemin_dll, &params.awesomewotlk).map_err(|e| suivi.echec(e))? {
        return Err(suivi.echec(Erreur::configuration(MESSAGE_DLL_ABSENTE)));
    }
    let cache = if params.vider_cache {
        vider_cache_suivi(params.dossier_jeu, journal, Some(&compte), &suivi.faites)?
    } else {
        journaliser(journal, Niveau::Info, MESSAGE_CACHE_CONSERVE, params.dossier_jeu, Some(&compte), &suivi.faites)?;
        ResultatCache::Conserve
    };
    suivi.reussi(ETAPE_CACHE, "Étape du cache terminée")?;
    let config_wtf = lire_config_wtf(params.dossier_jeu, journal).map_err(|e| suivi.echec(e))?;
    suivi.reussi(ETAPE_CONFIG, "Étape de Config.wtf terminée")?;
    signaler_ecart_realmlist(params, &config_wtf, &suivi)?;
    let reglage = ecrire_alerte_vide(params.dossier_jeu, journal).map_err(|e| suivi.echec(e))?;
    if reglage != ReglageFermeture::NonEcrit {
        suivi.reussi(ETAPE_ALERTE, "Étape du réglage de fermeture terminée")?;
    }
    let dossier_depart = dossier_lancement::dossier_par_defaut(params.donnees);
    let bilan = dossier_lancement::synchroniser(params.dossier_jeu, &dossier_depart, &chemin_dll, journal)
        .map_err(|e| suivi.echec(e))?;
    if !bilan.conflits.is_empty() {
        let noms: Vec<String> = bilan.conflits.iter().map(|n| n.to_string_lossy().into_owned()).collect();
        let message = format!(
            "Dossier de lancement : {} entrée(s) du jeu non reprises (un vrai fichier ou dossier est déjà là) : {}",
            noms.len(),
            noms.join(", ")
        );
        journaliser(journal, Niveau::Avertissement, &message, params.dossier_jeu, Some(&compte), &suivi.faites)?;
    }
    suivi.reussi(ETAPE_DOSSIER_LANCEMENT, "Dossier de lancement prêt")?;
    let commande = commande_pour(params, &dossier_depart, &compte, secret, &config_wtf).map_err(|e| suivi.echec(e))?;
    suivi.reussi(ETAPE_COMMANDE, "Commande du jeu construite (non journalisée)")?;
    Ok(Preparation {
        compte,
        cache,
        config_wtf,
        commande,
        faites: suivi.faites,
        dossier_jeu: params.dossier_jeu.to_path_buf(),
    })
}

/// Lance le jeu, détaché : le launcher ne l'attend pas.
pub fn lancer(preparation: Preparation, lanceur: &dyn LanceurDetache, journal: &Journal) -> Result<ResultatLancement> {
    let mut suivi = Suivi {
        journal,
        dossier_jeu: &preparation.dossier_jeu,
        compte: Some(preparation.compte.clone()),
        faites: preparation.faites.clone(),
    };
    let pid = lanceur.lancer(&preparation.commande).map_err(|e| suivi.echec(e))?;
    suivi.reussi(ETAPE_LANCE, &format!("Jeu lancé, détaché (processus {pid})"))?;
    let etapes = suivi.faites.iter().map(|e| (*e).to_string()).collect();
    Ok(ResultatLancement {
        pid,
        compte: preparation.compte.clone(),
        cache: preparation.cache,
        config_wtf: preparation.config_wtf.clone(),
        etapes,
    })
}

pub fn preparer_et_lancer(
    params: &ParametresLancement<'_>,
    coffre: &dyn CoffreSecrets,
    lanceur: &dyn LanceurDetache,
    journal: &Journal,
) -> Result<ResultatLancement> {
    let preparation = preparer(params, coffre, journal)?;
    lancer(preparation, lanceur, journal)
}

/// Mode « à blanc » : mêmes contrôles (compte, secret présent, commande constructible, `Config.wtf`
/// lu) mais rien n'est vidé ni lancé, et le plan ne contient ni secret ni commande.
pub fn plan_a_blanc(
    params: &ParametresLancement<'_>,
    coffre: &dyn CoffreSecrets,
    journal: &Journal,
) -> Result<PlanLancement> {
    let mut suivi = Suivi { journal, dossier_jeu: params.dossier_jeu, compte: None, faites: Vec::new() };
    let compte = resoudre_compte(params, &mut suivi)?;
    let secret = lire_secret(params.compte, coffre, &mut suivi)?;
    let entrees_cache = compter_cache(params.dossier_jeu).map_err(|e| suivi.echec(e))?;
    let config_wtf = lire_config_wtf(params.dossier_jeu, journal).map_err(|e| suivi.echec(e))?;
    let reglage_fermeture = prevoir_alerte_vide(params.dossier_jeu, journal).map_err(|e| suivi.echec(e))?;
    let dossier_depart = dossier_lancement::dossier_par_defaut(params.donnees);
    commande_pour(params, &dossier_depart, &compte, secret, &config_wtf).map_err(|e| suivi.echec(e))?;
    let awesomewotlk_prete = awesomewotlk::dll_prete(&chemin_dll_attendue(params), &params.awesomewotlk)
        .map_err(|e| suivi.echec(e))?;
    journaliser(
        journal,
        Niveau::Info,
        "Plan à blanc établi : rien n'a été vidé ni lancé",
        params.dossier_jeu,
        Some(&compte),
        &suivi.faites,
    )?;
    Ok(PlanLancement {
        compte,
        dossier_jeu: params.dossier_jeu.to_path_buf(),
        prefixe: params.prefixe.to_path_buf(),
        programme_wine: params.programme_wine.to_string(),
        secret_present: true,
        entrees_cache,
        config_wtf,
        reglage_fermeture,
        dossier_lancement: dossier_depart,
        awesomewotlk_prete,
    })
}
