use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::OsString;
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::erreur::{Erreur, Result};
use crate::secret::{MASQUE, Secret};

pub const TAILLE_MAX_SORTIE: usize = 64 * 1024;
pub const MARQUE_TRONCATURE: &str = "\n[sortie tronquée]";
pub const DELAI_PAR_DEFAUT: Duration = Duration::from_secs(300);
pub const PROGRAMME_ELEVATION: &str = "pkexec";
pub const PROGRAMME_ENVIRONNEMENT: &str = "env";
pub const SHELL_GROUPE: &str = "/bin/sh";
pub const CODE_PKEXEC_ANNULE: i32 = 126;
pub const CODE_PKEXEC_NON_AUTORISE: i32 = 127;
const DECALAGE_CODE_SIGNAL: i32 = 128;
const PAS_D_ATTENTE: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub enum Argument {
    Public(String),
    Secret(Secret),
}

impl Argument {
    /// Texte montrable : le clair d'un argument public, `***` pour un secret.
    pub fn affichage(&self) -> &str {
        match self {
            Self::Public(texte) => texte,
            Self::Secret(_) => MASQUE,
        }
    }

    fn clair(&self) -> &str {
        match self {
            Self::Public(texte) => texte,
            Self::Secret(secret) => secret.exposer(),
        }
    }
}

#[derive(Debug)]
pub struct Commande {
    pub programme: String,
    pub arguments: Vec<Argument>,
    pub environnement: BTreeMap<String, String>,
    pub repertoire: Option<PathBuf>,
    pub elevation: bool,
    pub delai: Duration,
}

impl Commande {
    pub fn new(programme: &str) -> Self {
        Self {
            programme: programme.to_string(),
            arguments: Vec::new(),
            environnement: BTreeMap::new(),
            repertoire: None,
            elevation: false,
            delai: DELAI_PAR_DEFAUT,
        }
    }

    pub fn arg(mut self, texte: &str) -> Self {
        self.arguments.push(Argument::Public(texte.to_string()));
        self
    }

    pub fn arg_secret(mut self, secret: Secret) -> Self {
        self.arguments.push(Argument::Secret(secret));
        self
    }

    pub fn env(mut self, nom: &str, valeur: &str) -> Self {
        self.environnement.insert(nom.to_string(), valeur.to_string());
        self
    }

    pub fn dans(mut self, repertoire: impl Into<PathBuf>) -> Self {
        self.repertoire = Some(repertoire.into());
        self
    }

    pub fn eleve(mut self) -> Self {
        self.elevation = true;
        self
    }

    pub fn avec_delai(mut self, delai: Duration) -> Self {
        self.delai = delai;
        self
    }

    /// Ligne de commande montrable : les secrets y sont masqués.
    pub fn affichage(&self) -> String {
        let mut morceaux = Vec::new();
        if self.elevation {
            morceaux.push(PROGRAMME_ELEVATION.to_string());
        }
        morceaux.push(quoter_shell(&self.programme));
        morceaux.extend(self.arguments.iter().map(|a| quoter_affichage(a.affichage())));
        morceaux.join(" ")
    }

    fn contient_secret(&self) -> bool {
        self.arguments.iter().any(|a| matches!(a, Argument::Secret(_)))
    }

    /// Remplace tout secret de cette commande qui apparaîtrait dans un texte.
    fn masquer_secrets(&self, texte: &str) -> String {
        self.arguments
            .iter()
            .filter_map(|a| match a {
                Argument::Secret(secret) if !secret.est_vide() => Some(secret.exposer()),
                _ => None,
            })
            .fold(texte.to_string(), |courant, clair| courant.replace(clair, MASQUE))
    }

    fn verifier_elevation(&self) -> Result<()> {
        if self.elevation {
            self.verifier_pour_script()
        } else {
            Ok(())
        }
    }

    fn verifier_pour_script(&self) -> Result<()> {
        if self.contient_secret() {
            return Err(Erreur::Elevation {
                raison: format!(
                    "un argument secret ne peut pas passer par {PROGRAMME_ELEVATION} (commande « {} »)",
                    self.affichage()
                ),
            });
        }
        verifier_noms_variables(&self.environnement)
    }
}

fn quoter_affichage(texte: &str) -> String {
    if !texte.is_empty() && texte.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%*".contains(c)) {
        texte.to_string()
    } else {
        quoter_shell(texte)
    }
}

fn verifier_noms_variables(environnement: &BTreeMap<String, String>) -> Result<()> {
    match environnement.keys().find(|nom| !nom_variable_valide(nom)) {
        Some(nom) => Err(Erreur::Elevation {
            raison: format!(
                "nom de variable d'environnement invalide : « {nom} » (attendu : lettres, chiffres et soulignés, sans chiffre au début)"
            ),
        }),
        None => Ok(()),
    }
}

/// Forme `[A-Za-z_][A-Za-z0-9_]*` : un nom qui commence par `-` serait lu comme option par `env`.
fn nom_variable_valide(nom: &str) -> bool {
    let mut caracteres = nom.chars();
    caracteres.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && caracteres.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Guillemetage pour `/bin/sh` : tout entre apostrophes, chaque apostrophe devient `'\''`.
/// Correct pour toute chaîne (vide, retours à la ligne, `$`, `;`, apostrophes).
pub fn quoter_shell(texte: &str) -> String {
    format!("'{}'", texte.replace('\'', "'\\''"))
}

fn ligne_script(commande: &Commande) -> Result<String> {
    commande.verifier_pour_script()?;
    let mut mots = Vec::new();
    if !commande.environnement.is_empty() {
        mots.push(PROGRAMME_ENVIRONNEMENT.to_string());
        mots.extend(commande.environnement.iter().map(|(nom, valeur)| quoter_shell(&format!("{nom}={valeur}"))));
    }
    mots.push(quoter_shell(&commande.programme));
    mots.extend(commande.arguments.iter().map(|a| quoter_shell(a.clair())));
    let ligne = mots.join(" ");
    Ok(match &commande.repertoire {
        Some(repertoire) => format!("(cd {} && {ligne})", quoter_shell(&repertoire.to_string_lossy())),
        None => ligne,
    })
}

/// Regroupe des commandes en UNE SEULE commande élevée `pkexec /bin/sh -c '<script>'` (une
/// seule authentification). Le script commence par `set -e`. Un argument secret est refusé :
/// il se verrait dans la ligne de commande du script.
pub fn grouper_eleve(commandes: Vec<Commande>) -> Result<Commande> {
    if commandes.is_empty() {
        return Err(Erreur::Elevation {
            raison: "aucune commande à grouper".to_string(),
        });
    }
    let mut lignes = vec!["set -e".to_string()];
    let mut delai = Duration::ZERO;
    for commande in &commandes {
        lignes.push(ligne_script(commande)?);
        delai = delai.saturating_add(commande.delai);
    }
    Ok(Commande::new(SHELL_GROUPE)
        .arg("-c")
        .arg(&lignes.join("\n"))
        .eleve()
        .avec_delai(delai))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sortie {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub tronquee: bool,
}

impl Sortie {
    pub fn nouvelle(code: i32, stdout: &str, stderr: &str) -> Self {
        Self {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            tronquee: false,
        }
    }

    pub fn reussie(&self) -> bool {
        self.code == 0
    }

    /// Un code non nul n'est pas une erreur de `lancer` ; cette aide le transforme en
    /// `Erreur::Commande` (secrets de la commande masqués) quand l'appelant l'exige.
    pub fn exiger_succes(self, commande: &Commande) -> Result<Self> {
        if self.reussie() {
            return Ok(self);
        }
        let detail = if self.stderr.trim().is_empty() { &self.stdout } else { &self.stderr };
        Err(Erreur::Commande {
            programme: commande.programme.clone(),
            code: Some(self.code),
            sortie: commande.masquer_secrets(detail.trim()),
        })
    }
}

pub trait Executeur {
    fn lancer(&self, commande: &Commande) -> Result<Sortie>;
    fn lancer_detache(&self, commande: &Commande) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enregistrement {
    pub programme: String,
    pub arguments: Vec<String>,
    pub environnement: BTreeMap<String, String>,
    pub repertoire: Option<PathBuf>,
    pub elevation: bool,
}

#[derive(Default)]
pub struct ExecuteurSimule {
    reponses: Mutex<HashMap<String, VecDeque<Sortie>>>,
    commandes: Mutex<Vec<Enregistrement>>,
}

impl ExecuteurSimule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Programme la réponse de la prochaine commande de ce programme ; les réponses d'un même
    /// programme sont rendues dans l'ordre.
    pub fn programmer(&self, programme: &str, sortie: Sortie) {
        self.reponses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(programme.to_string())
            .or_default()
            .push_back(sortie);
    }

    pub fn commandes(&self) -> Vec<Enregistrement> {
        self.commandes.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn consommer(&self, commande: &Commande) -> Result<Sortie> {
        commande.verifier_elevation()?;
        self.commandes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Enregistrement {
                programme: commande.programme.clone(),
                arguments: commande.arguments.iter().map(|a| a.affichage().to_string()).collect(),
                environnement: commande.environnement.clone(),
                repertoire: commande.repertoire.clone(),
                elevation: commande.elevation,
            });
        self.reponses
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&commande.programme)
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| Erreur::Commande {
                programme: commande.programme.clone(),
                code: None,
                sortie: format!(
                    "commande non programmée dans l'exécuteur simulé : {}",
                    commande.affichage()
                ),
            })
    }
}

impl Executeur for ExecuteurSimule {
    fn lancer(&self, commande: &Commande) -> Result<Sortie> {
        self.consommer(commande)
    }

    fn lancer_detache(&self, commande: &Commande) -> Result<()> {
        self.consommer(commande).map(|_| ())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ExecuteurReel;

const VARIABLES_APPIMAGE: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];
const LISTES_DE_CHEMINS: [&str; 7] = [
    "PATH",
    "LD_LIBRARY_PATH",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "PYTHONPATH",
    "GST_PLUGIN_SYSTEM_PATH",
    "QT_PLUGIN_PATH",
];

fn sous_appdir(valeur: &[u8], appdir: &[u8]) -> bool {
    valeur.strip_prefix(appdir).is_some_and(|reste| reste.is_empty() || reste.starts_with(b"/"))
}

/// Corrections à appliquer à l'environnement d'un processus enfant quand le launcher tourne en AppImage
/// (`None` = retirer la variable, `Some` = nouvelle valeur). Sans `APPDIR`, aucune correction. Fonction pure.
pub fn corrections_environnement(parent: &[(OsString, OsString)]) -> Vec<(OsString, Option<OsString>)> {
    let Some((_, appdir)) = parent.iter().find(|(nom, _)| nom == "APPDIR") else {
        return Vec::new();
    };
    let appdir = appdir.as_bytes().trim_ascii_end();
    let appdir = appdir.strip_suffix(b"/").unwrap_or(appdir);
    if appdir.is_empty() {
        return Vec::new();
    }
    let mut corrections = Vec::new();
    for (nom, valeur) in parent {
        if VARIABLES_APPIMAGE.iter().any(|v| nom == v) {
            corrections.push((nom.clone(), None));
            continue;
        }
        let octets = valeur.as_bytes();
        let liste = octets.contains(&b':') || LISTES_DE_CHEMINS.iter().any(|v| nom == v);
        if !liste {
            if sous_appdir(octets, appdir) {
                corrections.push((nom.clone(), None));
            }
            continue;
        }
        let gardes: Vec<&[u8]> = octets.split(|c| *c == b':').filter(|e| !sous_appdir(e, appdir)).collect();
        if gardes.len() == octets.split(|c| *c == b':').count() {
            continue;
        }
        let nouvelle = if gardes.is_empty() { None } else { Some(OsString::from_vec(gardes.join(&b':'))) };
        corrections.push((nom.clone(), nouvelle));
    }
    corrections
}

/// Retire de l'environnement du processus enfant ce que le runtime AppImage y a mis. À appeler avant
/// d'appliquer l'environnement propre à la commande, qui garde la priorité.
pub fn nettoyer_environnement(processus: &mut std::process::Command) {
    let parent: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    for (nom, valeur) in corrections_environnement(&parent) {
        match valeur {
            Some(valeur) => processus.env(nom, valeur),
            None => processus.env_remove(nom),
        };
    }
}

fn preparer(commande: &Commande) -> Result<std::process::Command> {
    commande.verifier_elevation()?;
    let mut processus;
    if commande.elevation {
        if commande.repertoire.is_some() {
            return Err(Erreur::Elevation {
                raison: format!(
                    "un répertoire de travail n'est pas pris en charge avec {PROGRAMME_ELEVATION} (commande « {} »)",
                    commande.affichage()
                ),
            });
        }
        processus = std::process::Command::new(PROGRAMME_ELEVATION);
        if !commande.environnement.is_empty() {
            processus.arg(PROGRAMME_ENVIRONNEMENT);
            for (nom, valeur) in &commande.environnement {
                processus.arg(format!("{nom}={valeur}"));
            }
        }
        processus.arg(&commande.programme);
    } else {
        processus = std::process::Command::new(&commande.programme);
        nettoyer_environnement(&mut processus);
        processus.envs(&commande.environnement);
        if let Some(repertoire) = &commande.repertoire {
            processus.current_dir(repertoire);
        }
    }
    processus.args(commande.arguments.iter().map(Argument::clair));
    processus.stdin(Stdio::null());
    Ok(processus)
}

fn echec_lancement(commande: &Commande, erreur: &std::io::Error) -> Erreur {
    Erreur::Commande {
        programme: commande.programme.clone(),
        code: None,
        sortie: format!("impossible de lancer le programme : {erreur}"),
    }
}

fn code_de(statut: std::process::ExitStatus) -> i32 {
    statut
        .code()
        .or_else(|| statut.signal().map(|signal| DECALAGE_CODE_SIGNAL + signal))
        .unwrap_or(-1)
}

fn lire_borne(mut flux: impl Read + Send + 'static) -> Receiver<std::io::Result<(Vec<u8>, bool)>> {
    let (envoi, reception) = mpsc::channel();
    thread::spawn(move || {
        let mut garde = Vec::new();
        let mut tronque = false;
        let mut tampon = [0u8; 8192];
        let resultat = loop {
            match flux.read(&mut tampon) {
                Ok(0) => break Ok((garde, tronque)),
                Ok(lus) => {
                    let place = TAILLE_MAX_SORTIE.saturating_sub(garde.len());
                    garde.extend_from_slice(&tampon[..lus.min(place)]);
                    tronque |= lus > place;
                }
                Err(erreur) => break Err(erreur),
            }
        };
        // Sans récepteur, le délai est déjà dépassé et l'erreur a été rendue à l'appelant : rien à ajouter.
        match envoi.send(resultat) {
            Ok(()) | Err(_) => {}
        }
    });
    reception
}

fn attendre_fin(enfant: &mut Child, limite: Instant) -> std::io::Result<Option<std::process::ExitStatus>> {
    loop {
        if let Some(statut) = enfant.try_wait()? {
            return Ok(Some(statut));
        }
        if Instant::now() >= limite {
            return Ok(None);
        }
        thread::sleep(PAS_D_ATTENTE);
    }
}

fn texte_borne(octets: Vec<u8>, tronque: bool) -> String {
    let mut texte = String::from_utf8_lossy(&octets).into_owned();
    if tronque {
        texte.push_str(MARQUE_TRONCATURE);
    }
    texte
}

impl Executeur for ExecuteurReel {
    fn lancer(&self, commande: &Commande) -> Result<Sortie> {
        let mut processus = preparer(commande)?;
        processus.stdout(Stdio::piped()).stderr(Stdio::piped());
        let limite = Instant::now() + commande.delai;
        let mut enfant = processus.spawn().map_err(|e| echec_lancement(commande, &e))?;
        let (Some(sortie_std), Some(sortie_err)) = (enfant.stdout.take(), enfant.stderr.take()) else {
            return Err(Erreur::Commande {
                programme: commande.programme.clone(),
                code: None,
                sortie: "flux de sortie indisponibles".to_string(),
            });
        };
        let lecture_std = lire_borne(sortie_std);
        let lecture_err = lire_borne(sortie_err);
        let delai_depasse = || Erreur::Commande {
            programme: commande.programme.clone(),
            code: None,
            sortie: format!(
                "délai de {} s dépassé, commande interrompue : {}",
                commande.delai.as_secs_f64(),
                commande.affichage()
            ),
        };
        let statut = match attendre_fin(&mut enfant, limite).map_err(|e| echec_lancement(commande, &e))? {
            Some(statut) => statut,
            None => {
                tuer(&mut enfant, commande)?;
                return Err(delai_depasse());
            }
        };
        let recuperer = |lecture: &Receiver<std::io::Result<(Vec<u8>, bool)>>| {
            let reste = limite.saturating_duration_since(Instant::now());
            match lecture.recv_timeout(reste) {
                Ok(Ok(lu)) => Ok(lu),
                Ok(Err(erreur)) => Err(echec_lancement(commande, &erreur)),
                Err(_) => Err(delai_depasse()),
            }
        };
        let (stdout, tronque_std) = recuperer(&lecture_std)?;
        let (stderr, tronque_err) = recuperer(&lecture_err)?;
        let code = code_de(statut);
        if commande.elevation {
            if code == CODE_PKEXEC_ANNULE {
                return Err(Erreur::Elevation {
                    raison: format!("authentification annulée (code {code} de {PROGRAMME_ELEVATION})"),
                });
            }
            if code == CODE_PKEXEC_NON_AUTORISE {
                return Err(Erreur::Elevation {
                    raison: format!("action non autorisée (code {code} de {PROGRAMME_ELEVATION})"),
                });
            }
        }
        Ok(Sortie {
            code,
            stdout: commande.masquer_secrets(&texte_borne(stdout, tronque_std)),
            stderr: commande.masquer_secrets(&texte_borne(stderr, tronque_err)),
            tronquee: tronque_std || tronque_err,
        })
    }

    fn lancer_detache(&self, commande: &Commande) -> Result<()> {
        let mut processus = preparer(commande)?;
        processus.stdout(Stdio::null()).stderr(Stdio::null());
        let mut enfant = processus.spawn().map_err(|e| echec_lancement(commande, &e))?;
        let programme = commande.programme.clone();
        thread::spawn(move || {
            if let Err(erreur) = enfant.wait() {
                eprintln!("Attente du processus détaché « {programme} » impossible : {erreur}");
            }
        });
        Ok(())
    }
}

fn tuer(enfant: &mut Child, commande: &Commande) -> Result<()> {
    enfant.kill().map_err(|e| echec_lancement(commande, &e))?;
    enfant.wait().map_err(|e| echec_lancement(commande, &e))?;
    Ok(())
}
