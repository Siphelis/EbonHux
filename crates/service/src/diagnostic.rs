//! Commande `diagnostic` : la machine (chaînes lisibles construites par des fonctions pures à partir
//! de la sonde du cœur) et le journal (une ligne par événement). Les textes du journal sont déjà
//! masqués (dossier personnel, secrets) : le service n'y ajoute rien de sensible.

use std::collections::BTreeSet;

use ebonhux_coeur::journal::{Evenement, FormatRapport, Niveau};
use ebonhux_coeur::systeme::{FamillePilote, InfoVulkan, InfoWine, ModeWine, Session, Systeme, sonder};
use serde::{Deserialize, Serialize};

use crate::comptes::{CONTEXTE_COMPTE, CONTEXTE_RESULTAT, Service, VALEUR_ATTENTE};
use crate::erreur::{CODE_DONNEES_INVALIDES, CODE_PORT_ABSENT, ErreurService, Resultat};

pub const INCONNU: &str = "Inconnu";
/// Nombre d'événements les plus récents rendus par `diagnostic`.
pub const LIGNES_MAX: usize = 500;
const ETAPE_JOURNAL: &str = "Journal";
const CLES_TECHNIQUES: [&str; 3] = [CONTEXTE_COMPTE, CONTEXTE_RESULTAT, "etapes_faites"];
const LONGUEUR_CODE_MAX: usize = 60;
const SEPARATEUR_DETAIL: &str = " — ";

/// Libellés français des étapes, trouvés par le début du message du journal ; sans correspondance,
/// le message lui-même sert de libellé.
pub const LIBELLES_ETAPES: [(&str, &str); 20] = [
    ("Compte résolu", "Compte reconnu"),
    ("Secret du compte trouvé", "Mot de passe récupéré"),
    ("Cache vidé", "Cache du jeu vidé"),
    ("Cache conservé", "Cache du jeu conservé"),
    ("Cache absent", "Cache du jeu absent"),
    ("Étape du cache terminée", "Cache du jeu préparé"),
    ("Config.wtf lu", "Réglages du jeu lus"),
    ("Étape de Config.wtf terminée", "Réglages du jeu vérifiés"),
    ("Réglage de fermeture rapide", "Fermeture du jeu accélérée"),
    ("Étape du réglage de fermeture", "Fermeture du jeu vérifiée"),
    ("Dossier de lancement prêt", "Dossier de lancement préparé"),
    ("Commande du jeu construite", "Commande du jeu préparée"),
    ("Jeu lancé", "Jeu lancé"),
    ("Mot de passe demandé", "Mot de passe demandé"),
    ("Mot de passe enregistré", "Mot de passe enregistré"),
    ("Compte ajouté", "Compte ajouté"),
    ("Compte déconnecté", "Compte déconnecté"),
    ("Langue revérifiée", "Langue vérifiée"),
    ("Site du projet ouvert", "Site du projet ouvert"),
    ("Suivi des mises à jour", "Suivi des addons"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemeAffiche {
    pub wine: String,
    pub vulkan: String,
    pub pilote: String,
    pub fichiers_ouverts: String,
    pub session: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultatLigne {
    Ok,
    Avertissement,
    Erreur,
    Attente,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LigneDiagnostic {
    pub heure: String,
    pub compte: Option<String>,
    pub etape: String,
    pub resultat: ResultatLigne,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseDiagnostic {
    pub systeme: SystemeAffiche,
    pub lignes: Vec<LigneDiagnostic>,
    pub comptes: Vec<String>,
    pub etapes: usize,
    pub erreurs: usize,
    pub avertissements: usize,
}

#[derive(Deserialize)]
struct RapportJournal {
    evenements: Vec<Evenement>,
    lignes_illisibles: usize,
}

// ---------- chaînes de la machine (pures) ----------

pub fn libelle_wine(wine: Option<&InfoWine>) -> String {
    let Some(wine) = wine else {
        return INCONNU.to_string();
    };
    let base = wine.version.as_ref().map_or_else(|| "Wine".to_string(), |version| format!("Wine {version}"));
    match wine.mode {
        ModeWine::Wow64 => format!("{base} (WoW64)"),
        ModeWine::Ancien => format!("{base} (ancien mode)"),
        ModeWine::Inconnu => base,
    }
}

pub fn libelle_vulkan(vulkan: &InfoVulkan) -> String {
    match (!vulkan.manifestes_64.is_empty(), !vulkan.manifestes_32.is_empty()) {
        (true, true) => "Vulkan 64 et 32 bits".to_string(),
        (true, false) => "Vulkan 64 bits".to_string(),
        (false, true) => "Vulkan 32 bits".to_string(),
        (false, false) => INCONNU.to_string(),
    }
}

pub fn libelle_pilote(pilote: &FamillePilote) -> String {
    match pilote {
        FamillePilote::Nvidia { .. } => "NVIDIA".to_string(),
        FamillePilote::Amd | FamillePilote::Intel | FamillePilote::Logiciel => "Mesa".to_string(),
        FamillePilote::Inconnue => INCONNU.to_string(),
    }
}

pub fn libelle_fichiers(limite: Option<u64>) -> String {
    limite.map_or_else(|| INCONNU.to_string(), |valeur| format!("Limite {valeur}"))
}

pub fn libelle_session(session: &Session) -> String {
    match session {
        Session::Wayland => "Wayland".to_string(),
        Session::X11 => "X11".to_string(),
        Session::Inconnue => INCONNU.to_string(),
    }
}

pub fn systeme_affiche(systeme: &Systeme) -> SystemeAffiche {
    SystemeAffiche {
        wine: libelle_wine(systeme.wine.as_ref()),
        vulkan: libelle_vulkan(&systeme.vulkan),
        pilote: libelle_pilote(&systeme.pilote),
        fichiers_ouverts: libelle_fichiers(systeme.limite_fichiers_souple),
        session: libelle_session(&systeme.session),
    }
}

// ---------- lignes du journal (pures) ----------

/// ISO 8601 UTC à la seconde. Les horodatages du journal sont déjà en UTC (`Z`) : seules les fractions sont retirées.
pub fn heure_iso(horodatage: &str) -> String {
    match (horodatage.strip_suffix('Z'), horodatage.find('.')) {
        (Some(_), Some(point)) => format!("{}Z", &horodatage[..point]),
        _ => horodatage.to_string(),
    }
}

pub fn libelle_etape(evenement: &Evenement) -> String {
    LIBELLES_ETAPES
        .iter()
        .find(|(debut, _)| evenement.message.starts_with(debut))
        .map_or_else(
            || if evenement.message.is_empty() { evenement.etape.clone() } else { evenement.message.clone() },
            |(_, libelle)| (*libelle).to_string(),
        )
}

pub fn resultat_de(evenement: &Evenement) -> ResultatLigne {
    if evenement.niveau == Niveau::Erreur {
        ResultatLigne::Erreur
    } else if evenement.niveau == Niveau::Avertissement {
        ResultatLigne::Avertissement
    } else if evenement.contexte.get(CONTEXTE_RESULTAT).map(String::as_str) == Some(VALEUR_ATTENTE) {
        ResultatLigne::Attente
    } else {
        ResultatLigne::Ok
    }
}

fn detail_de(evenement: &Evenement, libelle: &str, resultat: ResultatLigne) -> String {
    let mut parties = Vec::new();
    if libelle != evenement.message || matches!(resultat, ResultatLigne::Erreur | ResultatLigne::Avertissement) {
        parties.push(evenement.message.clone());
    }
    for (cle, valeur) in &evenement.contexte {
        if !CLES_TECHNIQUES.contains(&cle.as_str()) {
            parties.push(format!("{cle} : {valeur}"));
        }
    }
    if let Some(code) = &evenement.erreur
        && resultat == ResultatLigne::Erreur
        && code.len() <= LONGUEUR_CODE_MAX
        && !code.contains(char::is_whitespace)
    {
        parties.push(format!("code : {code}"));
    }
    parties.join(SEPARATEUR_DETAIL)
}

pub fn ligne_depuis(evenement: &Evenement) -> LigneDiagnostic {
    let etape = libelle_etape(evenement);
    let resultat = resultat_de(evenement);
    LigneDiagnostic {
        heure: heure_iso(&evenement.horodatage),
        compte: evenement.contexte.get(CONTEXTE_COMPTE).cloned(),
        detail: detail_de(evenement, &etape, resultat),
        etape,
        resultat,
    }
}

/// Lignes, noms de comptes distincts (dans l'ordre de rencontre), nombre d'étapes et d'erreurs.
pub fn assembler(evenements: &[Evenement], illisibles: usize) -> (Vec<LigneDiagnostic>, Vec<String>) {
    let mut lignes: Vec<LigneDiagnostic> = evenements.iter().map(ligne_depuis).collect();
    if illisibles > 0 {
        let heure = lignes.last().map(|l| l.heure.clone()).unwrap_or_default();
        lignes.push(LigneDiagnostic {
            heure,
            compte: None,
            etape: ETAPE_JOURNAL.to_string(),
            resultat: ResultatLigne::Erreur,
            detail: format!("{illisibles} ligne(s) illisible(s) dans le journal"),
        });
    }
    let mut vus = BTreeSet::new();
    let comptes = lignes
        .iter()
        .filter_map(|l| l.compte.clone())
        .filter(|nom| vus.insert(nom.to_lowercase()))
        .collect();
    (lignes, comptes)
}

impl Service {
    /// Commande `diagnostic`.
    pub fn diagnostic(&self) -> Resultat<ReponseDiagnostic> {
        let Some(executeur) = &self.executeur else {
            return Err(ErreurService::nouvelle(CODE_PORT_ABSENT, "Le diagnostic de la machine est indisponible dans cette installation."));
        };
        let systeme = self.lecture_silencieuse(|journal| sonder(&self.racine_systeme, &self.environnement, executeur.as_ref(), journal))?;
        let texte = self.journal.rapport(FormatRapport::Json, Some(LIGNES_MAX))?;
        let rapport: RapportJournal = serde_json::from_str(&texte)
            .map_err(|erreur| ErreurService::nouvelle(CODE_DONNEES_INVALIDES, format!("Journal illisible pour le diagnostic : {erreur}")))?;
        let (lignes, comptes) = assembler(&rapport.evenements, rapport.lignes_illisibles);
        let erreurs = lignes.iter().filter(|l| l.resultat == ResultatLigne::Erreur).count();
        let avertissements = lignes.iter().filter(|l| l.resultat == ResultatLigne::Avertissement).count();
        Ok(ReponseDiagnostic { systeme: systeme_affiche(&systeme), etapes: lignes.len(), erreurs, avertissements, lignes, comptes })
    }
}
