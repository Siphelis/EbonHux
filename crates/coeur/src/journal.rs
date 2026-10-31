use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::config::{Dossiers, creer_dossier_prive, ouvrir_fichier_prive};
use crate::erreur::{Erreur, Result};
use crate::secret::MASQUE;

pub const EXTENSION_JOURNAL: &str = "jsonl";
const REMPLACEMENT_MAISON: &str = "~";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Niveau {
    Info,
    Avertissement,
    Erreur,
}

impl Niveau {
    fn libelle(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Avertissement => "AVERTISSEMENT",
            Self::Erreur => "ERREUR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evenement {
    pub horodatage: String,
    pub niveau: Niveau,
    pub etape: String,
    pub message: String,
    pub contexte: BTreeMap<String, String>,
    pub erreur: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatRapport {
    Texte,
    Json,
}

#[derive(Serialize)]
struct RapportJson<'a> {
    evenements: &'a [Evenement],
    lignes_illisibles: usize,
}

enum Destination {
    Fichier(PathBuf),
    Memoire(Mutex<Vec<Evenement>>),
}

type Horloge = Box<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct Journal {
    destination: Destination,
    maison: PathBuf,
    horloge: Horloge,
}

#[derive(Clone, Copy)]
enum Debut {
    Libre,
    MotEntier,
}

struct Regle {
    motif: &'static str,
    debut: Debut,
    separateur_obligatoire: bool,
}

const REGLES: [Regle; 3] = [
    Regle { motif: "-password", debut: Debut::MotEntier, separateur_obligatoire: true },
    Regle { motif: "password=", debut: Debut::Libre, separateur_obligatoire: false },
    Regle { motif: "authorization: bearer", debut: Debut::Libre, separateur_obligatoire: true },
];

fn est_blanc(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn fin_valeur(texte: &str, debut: usize) -> usize {
    let reste = &texte[debut..];
    match reste.chars().next() {
        Some(guillemet @ ('"' | '\'')) => reste[guillemet.len_utf8()..]
            .find(guillemet)
            .map_or(texte.len(), |rel| debut + guillemet.len_utf8() + rel + guillemet.len_utf8()),
        _ => reste.find(char::is_whitespace).map_or(texte.len(), |rel| debut + rel),
    }
}

fn debut_valide(bas: &str, position: usize, debut: Debut) -> bool {
    match debut {
        Debut::Libre => true,
        Debut::MotEntier => bas[..position]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || c == '-'),
    }
}

fn masquer_regle(texte: &str, regle: &Regle) -> String {
    let bas = texte.to_ascii_lowercase();
    let mut sortie = String::with_capacity(texte.len());
    let mut position = 0;
    while let Some(relatif) = bas[position..].find(regle.motif) {
        let debut = position + relatif;
        let apres = debut + regle.motif.len();
        let blancs = if regle.separateur_obligatoire {
            texte[apres..].chars().take_while(|c| est_blanc(*c)).count()
        } else {
            0
        };
        let valeur = apres + blancs;
        let fin = fin_valeur(texte, valeur);
        let applicable = debut_valide(&bas, debut, regle.debut)
            && (!regle.separateur_obligatoire || blancs > 0)
            && fin > valeur;
        if applicable {
            sortie.push_str(&texte[position..valeur]);
            sortie.push_str(MASQUE);
            position = fin;
        } else {
            sortie.push_str(&texte[position..apres]);
            position = apres;
        }
    }
    sortie.push_str(&texte[position..]);
    sortie
}

fn masquer_maison(texte: &str, maison: &Path) -> String {
    let maison = maison.to_string_lossy();
    let maison = maison.trim_end_matches('/');
    if maison.is_empty() {
        return texte.to_string();
    }
    let mut sortie = String::with_capacity(texte.len());
    let mut position = 0;
    while let Some(relatif) = texte[position..].find(maison) {
        let debut = position + relatif;
        let fin = debut + maison.len();
        let borne = texte[fin..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.'));
        sortie.push_str(&texte[position..debut]);
        sortie.push_str(if borne { REMPLACEMENT_MAISON } else { maison });
        position = fin;
    }
    sortie.push_str(&texte[position..]);
    sortie
}

/// Retire d'un texte tout ce qui ne doit jamais atteindre le disque : le dossier personnel
/// devient `~`, et la valeur qui suit `-password`, `password=` ou `Authorization: Bearer`
/// devient `***` (une valeur entre guillemets, espaces compris, est prise en entier).
pub fn masquer(texte: &str, maison: &Path) -> String {
    let sans_secret = REGLES
        .iter()
        .fold(texte.to_string(), |courant, regle| masquer_regle(&courant, regle));
    masquer_maison(&sans_secret, maison)
}

fn verrouiller<T>(verrou: &Mutex<T>) -> MutexGuard<'_, T> {
    verrou.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Journal {
    pub fn vers_fichier(dossiers: &Dossiers) -> Self {
        Self::nouveau(Destination::Fichier(dossiers.dossier_journal()), dossiers.maison.clone())
    }

    pub fn en_memoire(maison: impl Into<PathBuf>) -> Self {
        Self::nouveau(Destination::Memoire(Mutex::new(Vec::new())), maison.into())
    }

    fn nouveau(destination: Destination, maison: PathBuf) -> Self {
        Self {
            destination,
            maison,
            horloge: Box::new(OffsetDateTime::now_utc),
        }
    }

    pub fn avec_horloge(
        mut self,
        horloge: impl Fn() -> OffsetDateTime + Send + Sync + 'static,
    ) -> Self {
        self.horloge = Box::new(horloge);
        self
    }

    pub fn evenements_memoire(&self) -> Vec<Evenement> {
        match &self.destination {
            Destination::Memoire(liste) => verrouiller(liste).clone(),
            Destination::Fichier(_) => Vec::new(),
        }
    }

    pub fn info(&self, etape: &str, message: &str) -> Result<()> {
        self.evenement(Niveau::Info, etape, message, &BTreeMap::new(), None)
    }

    pub fn avertir(&self, etape: &str, message: &str) -> Result<()> {
        self.evenement(Niveau::Avertissement, etape, message, &BTreeMap::new(), None)
    }

    pub fn erreur(&self, etape: &str, message: &str) -> Result<()> {
        self.evenement(Niveau::Erreur, etape, message, &BTreeMap::new(), None)
    }

    pub fn journaliser_erreur(&self, etape: &str, erreur: &Erreur) -> Result<()> {
        self.evenement(
            Niveau::Erreur,
            etape,
            &erreur.to_string(),
            &BTreeMap::new(),
            Some(format!("{erreur:?}")),
        )
    }

    pub fn evenement(
        &self,
        niveau: Niveau,
        etape: &str,
        message: &str,
        contexte: &BTreeMap<String, String>,
        erreur: Option<String>,
    ) -> Result<()> {
        let maintenant = (self.horloge)();
        let horodatage = maintenant
            .format(&Rfc3339)
            .map_err(|raison| Erreur::analyse("l'horodatage du journal", raison))?;
        let evenement = Evenement {
            horodatage,
            niveau,
            etape: self.masquer(etape),
            message: self.masquer(message),
            contexte: contexte
                .iter()
                .map(|(cle, valeur)| (self.masquer(cle), self.masquer(valeur)))
                .collect(),
            erreur: erreur.map(|texte| self.masquer(&texte)),
        };
        match &self.destination {
            Destination::Memoire(liste) => {
                verrouiller(liste).push(evenement);
                Ok(())
            }
            Destination::Fichier(dossier) => ecrire_ligne(dossier, &maintenant, &evenement),
        }
    }

    fn masquer(&self, texte: &str) -> String {
        masquer(texte, &self.maison)
    }

    fn lire(&self) -> Result<(Vec<Evenement>, usize)> {
        match &self.destination {
            Destination::Memoire(liste) => Ok((verrouiller(liste).clone(), 0)),
            Destination::Fichier(dossier) => lire_dossier(dossier),
        }
    }

    /// Rapport exportable des événements écrits, du plus ancien au plus récent ; chaque texte
    /// est masqué une seconde fois à la lecture. Les lignes illisibles sont comptées et le
    /// compte figure dans le rapport.
    pub fn rapport(&self, format: FormatRapport, dernieres: Option<usize>) -> Result<String> {
        let (mut evenements, illisibles) = self.lire()?;
        if let Some(nombre) = dernieres {
            let surplus = evenements.len().saturating_sub(nombre);
            evenements.drain(..surplus);
        }
        let evenements: Vec<Evenement> = evenements.into_iter().map(|e| self.masquer_evenement(e)).collect();
        match format {
            FormatRapport::Json => serde_json::to_string_pretty(&RapportJson {
                evenements: &evenements,
                lignes_illisibles: illisibles,
            })
            .map_err(|raison| Erreur::analyse("le rapport du journal", raison)),
            FormatRapport::Texte => Ok(rapport_texte(&evenements, illisibles)),
        }
    }

    fn masquer_evenement(&self, evenement: Evenement) -> Evenement {
        Evenement {
            horodatage: evenement.horodatage,
            niveau: evenement.niveau,
            etape: self.masquer(&evenement.etape),
            message: self.masquer(&evenement.message),
            contexte: evenement
                .contexte
                .iter()
                .map(|(cle, valeur)| (self.masquer(cle), self.masquer(valeur)))
                .collect(),
            erreur: evenement.erreur.map(|texte| self.masquer(&texte)),
        }
    }
}

fn rapport_texte(evenements: &[Evenement], illisibles: usize) -> String {
    let mut texte = String::new();
    for evenement in evenements {
        texte.push_str(&format!(
            "{} [{}] {} : {}",
            evenement.horodatage,
            evenement.niveau.libelle(),
            evenement.etape,
            evenement.message
        ));
        for (cle, valeur) in &evenement.contexte {
            texte.push_str(&format!(" {cle}={valeur}"));
        }
        if let Some(erreur) = &evenement.erreur {
            texte.push_str(&format!(" | erreur : {erreur}"));
        }
        texte.push('\n');
    }
    texte.push_str(&format!(
        "{} événement(s), {} ligne(s) illisible(s)\n",
        evenements.len(),
        illisibles
    ));
    texte
}

fn nom_fichier_du_jour(instant: &OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}.{EXTENSION_JOURNAL}",
        instant.year(),
        u8::from(instant.month()),
        instant.day()
    )
}

fn ecrire_ligne(dossier: &Path, instant: &OffsetDateTime, evenement: &Evenement) -> Result<()> {
    creer_dossier_prive(dossier, "créer le dossier du journal")?;
    let fichier = dossier.join(nom_fichier_du_jour(instant));
    let mut ligne = serde_json::to_string(evenement)
        .map_err(|raison| Erreur::analyse("un événement du journal", raison))?;
    ligne.push('\n');
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    let mut ouvert = ouvrir_fichier_prive(&fichier, &mut options, "ouvrir le journal")?;
    ouvert
        .write_all(ligne.as_bytes())
        .map_err(|erreur| Erreur::io("écrire dans le journal", &fichier, erreur))
}

fn lire_dossier(dossier: &Path) -> Result<(Vec<Evenement>, usize)> {
    let entrees = match fs::read_dir(dossier) {
        Ok(entrees) => entrees,
        Err(erreur) if erreur.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), 0));
        }
        Err(erreur) => return Err(Erreur::io("lire le dossier du journal", dossier, erreur)),
    };
    let mut fichiers = Vec::new();
    for entree in entrees {
        let entree = entree.map_err(|erreur| Erreur::io("parcourir le dossier du journal", dossier, erreur))?;
        let chemin = entree.path();
        if chemin.extension().is_some_and(|extension| extension == EXTENSION_JOURNAL) {
            fichiers.push(chemin);
        }
    }
    fichiers.sort();
    let mut evenements = Vec::new();
    let mut illisibles = 0;
    for chemin in fichiers {
        let texte = fs::read_to_string(&chemin)
            .map_err(|erreur| Erreur::io("lire le journal", &chemin, erreur))?;
        for ligne in texte.lines().filter(|ligne| !ligne.trim().is_empty()) {
            match serde_json::from_str::<Evenement>(ligne) {
                Ok(evenement) => evenements.push(evenement),
                Err(_) => illisibles += 1,
            }
        }
    }
    Ok((evenements, illisibles))
}
