//! Suivi des comptes lancés : horloge et sonde de processus injectables, registre en mémoire,
//! calcul pur de la phase d'une session. Aucun essai ne dort pour de vrai ni ne lit le vrai `/proc`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Durée de l'état « Lancement » après le lancement du processus (estimation, voir `lib.rs`).
pub const DUREE_LANCEMENT_S: u64 = 25;
/// Écart entre deux lancements de « Tout lancer ».
pub const PAUSE_ENTRE_LANCEMENTS_S: u64 = 3;
pub const PROGRESSION_MAX: u8 = 99;
const POURCENT: u64 = 100;
const ETAT_ZOMBIE: char = 'Z';
const ETAT_MORT: char = 'X';

// ---------- horloge ----------

pub trait Horloge: Send + Sync {
    /// Secondes écoulées depuis le 1er janvier 1970 (UTC).
    fn maintenant_s(&self) -> u64;
    fn attendre(&self, secondes: u64);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HorlogeReelle;

impl Horloge for HorlogeReelle {
    fn maintenant_s(&self) -> u64 {
        // Une horloge réglée avant 1970 rend 0 : les sessions paraissent alors à leur début, sans panique.
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    fn attendre(&self, secondes: u64) {
        thread::sleep(Duration::from_secs(secondes));
    }
}

/// Horloge des essais : le temps n'avance que si l'essai le demande, `attendre` l'avance sans dormir
/// et enregistre la durée demandée.
#[derive(Debug, Default)]
pub struct HorlogeSimulee {
    instant: AtomicU64,
    attentes: Mutex<Vec<u64>>,
}

impl HorlogeSimulee {
    pub fn new(depart_s: u64) -> Self {
        Self { instant: AtomicU64::new(depart_s), attentes: Mutex::new(Vec::new()) }
    }

    pub fn avancer(&self, secondes: u64) {
        self.instant.fetch_add(secondes, Ordering::SeqCst);
    }

    pub fn attentes(&self) -> Vec<u64> {
        verrouiller(&self.attentes).clone()
    }
}

impl Horloge for HorlogeSimulee {
    fn maintenant_s(&self) -> u64 {
        self.instant.load(Ordering::SeqCst)
    }

    fn attendre(&self, secondes: u64) {
        verrouiller(&self.attentes).push(secondes);
        self.avancer(secondes);
    }
}

// ---------- sonde de processus ----------

/// Ce que la sonde sait d'un pid : absent (ou zombie), présent sans identité lisible, ou présent avec
/// son heure de démarrage (`starttime`, en ticks depuis le démarrage du système).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demarrage {
    Absent,
    Inconnu,
    Heure(u64),
}

pub trait SondeProcessus: Send + Sync {
    fn demarrage(&self, pid: u32) -> Demarrage;

    fn vivant(&self, pid: u32) -> bool {
        self.demarrage(pid) != Demarrage::Absent
    }
}

/// Rang (à partir de l'état, champ 3) du champ `starttime` (champ 22) après le nom du processus.
const RANG_STARTTIME: usize = 19;

/// Heure de démarrage (vingt-deuxième champ) lue dans le contenu de `/proc/<pid>/stat`. Même règle que
/// `etat_depuis_stat` pour le nom. `None` si malformé.
pub fn demarrage_depuis_stat(contenu: &str) -> Option<u64> {
    let (_, apres) = contenu.rsplit_once(')')?;
    apres.split_whitespace().nth(RANG_STARTTIME)?.parse().ok()
}

/// État (lettre) lu dans le contenu de `/proc/<pid>/stat` : `pid (nom) E ...`. Le nom peut contenir
/// espaces et parenthèses, d'où la recherche de la DERNIÈRE parenthèse fermante. `None` si malformé.
pub fn etat_depuis_stat(contenu: &str) -> Option<char> {
    let (_, apres) = contenu.rsplit_once(')')?;
    apres.trim_start().chars().next()
}

/// Un processus zombie ou mort est terminé : seul son suivi par le système reste.
pub fn etat_vivant(etat: char) -> bool {
    etat != ETAT_ZOMBIE && etat != ETAT_MORT
}

/// Sonde réelle : lit `<racine>/proc/<pid>/stat`.
#[derive(Debug, Clone)]
pub struct SondeProc {
    racine: PathBuf,
}

impl SondeProc {
    pub fn new(racine: impl Into<PathBuf>) -> Self {
        Self { racine: racine.into() }
    }

    fn fichier(&self, pid: u32) -> PathBuf {
        self.racine.join("proc").join(pid.to_string()).join("stat")
    }
}

impl SondeProcessus for SondeProc {
    fn demarrage(&self, pid: u32) -> Demarrage {
        match fs::read_to_string(self.fichier(pid)) {
            Ok(contenu) if !etat_depuis_stat(&contenu).is_none_or(etat_vivant) => Demarrage::Absent,
            Ok(contenu) => demarrage_depuis_stat(&contenu).map_or(Demarrage::Inconnu, Demarrage::Heure),
            Err(erreur) if erreur.kind() == ErrorKind::NotFound => Demarrage::Absent,
            Err(_) => Demarrage::Inconnu,
        }
    }
}

/// Heure de démarrage d'un processus simulé tant que l'essai ne l'a pas changée.
pub const DEMARRAGE_SIMULE: u64 = 1;

/// Sonde des essais : tout processus est vivant jusqu'à ce que l'essai le « tue ».
#[derive(Debug, Default)]
pub struct SondeSimulee {
    morts: Mutex<BTreeSet<u32>>,
    heures: Mutex<BTreeMap<u32, u64>>,
}

impl SondeSimulee {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tuer(&self, pid: u32) {
        verrouiller(&self.morts).insert(pid);
    }

    /// Le pid vit toujours mais appartient désormais à un autre processus (autre heure de démarrage).
    pub fn reattribuer(&self, pid: u32, heure: u64) {
        verrouiller(&self.heures).insert(pid, heure);
    }
}

impl SondeProcessus for SondeSimulee {
    fn demarrage(&self, pid: u32) -> Demarrage {
        if verrouiller(&self.morts).contains(&pid) {
            return Demarrage::Absent;
        }
        Demarrage::Heure(verrouiller(&self.heures).get(&pid).copied().unwrap_or(DEMARRAGE_SIMULE))
    }
}

// ---------- phase d'une session (pure) ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseSession {
    Lancement { progression: u8 },
    EnJeu { depuis_s: u64 },
}

/// `lancement` (progression = min(99, écoulé × 100 / 25)) pendant `DUREE_LANCEMENT_S`, puis `en_jeu`.
pub fn phase_session(debut_s: u64, maintenant_s: u64) -> PhaseSession {
    let ecoule = maintenant_s.saturating_sub(debut_s);
    if ecoule < DUREE_LANCEMENT_S {
        let pourcent = (ecoule * POURCENT / DUREE_LANCEMENT_S).min(u64::from(PROGRESSION_MAX));
        PhaseSession::Lancement { progression: u8::try_from(pourcent).unwrap_or(PROGRESSION_MAX) }
    } else {
        PhaseSession::EnJeu { depuis_s: ecoule - DUREE_LANCEMENT_S }
    }
}

// ---------- registre ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub pid: u32,
    pub debut_s: u64,
    /// Heure de démarrage du processus à l'enregistrement (`None` si la sonde ne la connaissait pas).
    pub demarrage: Option<u64>,
}

/// Une session est vivante si le processus existe et, quand les deux heures sont connues, n'a pas changé
/// (un pid réattribué à un autre processus n'est pas la session).
pub fn session_vivante(session: &Session, maintenant: Demarrage) -> bool {
    match maintenant {
        Demarrage::Absent => false,
        Demarrage::Inconnu => true,
        Demarrage::Heure(heure) => session.demarrage.is_none_or(|enregistree| enregistree == heure),
    }
}

fn verrouiller<T>(verrou: &Mutex<T>) -> MutexGuard<'_, T> {
    verrou.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sessions en mémoire, par nom de compte en minuscules.
#[derive(Debug, Default)]
pub struct Registre {
    sessions: Mutex<BTreeMap<String, Session>>,
}

impl Registre {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enregistrer(&self, cle: &str, session: Session) {
        verrouiller(&self.sessions).insert(cle.to_string(), session);
    }

    /// Sessions dont le processus vit encore ; les autres disparaissent du registre.
    pub fn actives(&self, sonde: &dyn SondeProcessus) -> BTreeMap<String, Session> {
        let mut sessions = verrouiller(&self.sessions);
        sessions.retain(|_, session| session_vivante(session, sonde.demarrage(session.pid)));
        sessions.clone()
    }

    pub fn phase(&self, cle: &str, sonde: &dyn SondeProcessus, maintenant_s: u64) -> Option<PhaseSession> {
        self.actives(sonde).get(cle).map(|s| phase_session(s.debut_s, maintenant_s))
    }
}
