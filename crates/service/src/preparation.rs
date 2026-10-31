//! Première installation (docs/interface.md, décisions 17 à 20) : état en trois étapes
//! (`installation`), choix du dossier du jeu, installation de Wine par le gestionnaire de paquets
//! (plan du cœur, élévation `pkexec` groupée) et préparation du préfixe Wine avec DXVK.
//! Les états se décident par des fonctions pures sur des faits sondés ; la présence du préfixe et
//! de DXVK se lit sans rien écrire.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ebonhux_coeur::awesomewotlk;
use ebonhux_coeur::config::Config;
use ebonhux_coeur::dxvk::{self, DLL_REQUISES};
use ebonhux_coeur::erreur::Erreur;
use ebonhux_coeur::executeur::Executeur;
use ebonhux_coeur::installation::{self, Mode, Statut};
use ebonhux_coeur::journal::Niveau;
use ebonhux_coeur::prefixe::{self, EXTENSION_DLL, REPERE_PREFIXE_PRET, SOUS_CHEMIN_SYSTEM32, SOUS_CHEMIN_SYSWOW64};
use ebonhux_coeur::systeme::{FamillePaquets, Systeme, sonder};
use serde::Serialize;

use crate::comptes::{ETAPE_SERVICE, Service};
use crate::erreur::{
    CODE_DEJA_EN_COURS, CODE_DISTRIBUTION_NON_PRISE_EN_CHARGE, CODE_DOSSIER_JEU_INVALIDE, CODE_PORT_ABSENT, CODE_PREPARATION_IMPOSSIBLE,
    ErreurService, Resultat,
};

pub const FICHIER_JEU: &str = "Wow.exe";
pub const DOSSIER_DONNEES_JEU: &str = "Data";

pub const ID_ETAPE_JEU: &str = "jeu";
pub const ID_ETAPE_LOGICIELS: &str = "logiciels";
pub const ID_ETAPE_PREPARATION: &str = "preparation";
pub const TITRE_ETAPE_JEU: &str = "Dossier du jeu";
pub const TITRE_ETAPE_LOGICIELS: &str = "Logiciels nécessaires";
pub const TITRE_ETAPE_PREPARATION: &str = "Préparation du jeu";

pub const ETAT_FAIT: &str = "fait";
pub const ETAT_A_FAIRE: &str = "a_faire";
pub const ETAT_BLOQUE: &str = "bloque";

const DISTRIBUTION_INCONNUE: &str = "Inconnue";
const STATUT_REUSSIE: &str = "reussie";
const STATUT_SIMULEE: &str = "simulee";
const MANQUE_JEU: &str = "le dossier du jeu";
const MANQUE_WINE: &str = "Wine";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WineAffiche {
    pub present: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EtapeInstallation {
    pub id: String,
    pub titre: String,
    pub etat: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseInstallation {
    pub pret: bool,
    pub jeu_configure: bool,
    pub dossier_jeu: Option<String>,
    pub dossier_jeu_valide: bool,
    pub distribution: String,
    pub installation_possible: bool,
    pub wine: WineAffiche,
    pub prefixe_pret: bool,
    pub dxvk_pose: bool,
    pub awesomewotlk_pret: bool,
    pub etapes: Vec<EtapeInstallation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EtapeExecutee {
    pub description: String,
    pub statut: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseLogiciels {
    pub etapes: Vec<EtapeExecutee>,
    pub message: String,
}

/// Faits sondés dont dépendent les états des étapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Faits {
    pub dossier_jeu: Option<String>,
    /// Pourquoi le dossier configuré n'est pas utilisable (`None` = utilisable ou non configuré).
    pub raison_jeu_invalide: Option<String>,
    pub distribution: Option<String>,
    pub installation_possible: bool,
    pub wine_present: bool,
    pub wine_version: Option<String>,
    pub prefixe_pret: bool,
    pub dxvk_pose: bool,
    pub awesomewotlk_pret: bool,
}

impl Faits {
    pub fn jeu_valide(&self) -> bool {
        self.dossier_jeu.is_some() && self.raison_jeu_invalide.is_none()
    }
}

// ---------- fonctions pures ----------

fn etape(id: &str, titre: &str, etat: &str, detail: String) -> EtapeInstallation {
    EtapeInstallation { id: id.to_string(), titre: titre.to_string(), etat: etat.to_string(), detail }
}

/// Ce qui empêche la préparation : le dossier du jeu et/ou Wine.
pub fn manquants_preparation(faits: &Faits) -> Vec<&'static str> {
    let mut manquants = Vec::new();
    if !faits.jeu_valide() {
        manquants.push(MANQUE_JEU);
    }
    if !faits.wine_present {
        manquants.push(MANQUE_WINE);
    }
    manquants
}

pub fn etapes_depuis(faits: &Faits) -> Vec<EtapeInstallation> {
    let jeu = match (&faits.dossier_jeu, &faits.raison_jeu_invalide) {
        (Some(dossier), None) => etape(ID_ETAPE_JEU, TITRE_ETAPE_JEU, ETAT_FAIT, format!("Dossier du jeu : {dossier}.")),
        (Some(_), Some(raison)) => {
            etape(ID_ETAPE_JEU, TITRE_ETAPE_JEU, ETAT_A_FAIRE, format!("Le dossier enregistré n'est plus utilisable : {raison}"))
        }
        (None, _) => etape(ID_ETAPE_JEU, TITRE_ETAPE_JEU, ETAT_A_FAIRE, "Indiquez le dossier où le jeu est installé.".to_string()),
    };
    let logiciels = if faits.wine_present {
        let detail = match &faits.wine_version {
            Some(version) => format!("Wine {version} est installé."),
            None => "Wine est installé.".to_string(),
        };
        etape(ID_ETAPE_LOGICIELS, TITRE_ETAPE_LOGICIELS, ETAT_FAIT, detail)
    } else if faits.installation_possible {
        etape(
            ID_ETAPE_LOGICIELS,
            TITRE_ETAPE_LOGICIELS,
            ETAT_A_FAIRE,
            "Wine n'est pas installé : il sera installé par le gestionnaire de paquets de votre système.".to_string(),
        )
    } else {
        etape(
            ID_ETAPE_LOGICIELS,
            TITRE_ETAPE_LOGICIELS,
            ETAT_BLOQUE,
            "Wine n'est pas installé et votre distribution n'est pas prise en charge pour l'installer automatiquement.".to_string(),
        )
    };
    let manquants = manquants_preparation(faits);
    let preparation = if !manquants.is_empty() {
        etape(ID_ETAPE_PREPARATION, TITRE_ETAPE_PREPARATION, ETAT_BLOQUE, format!("Il manque : {}.", manquants.join(" et ")))
    } else if faits.prefixe_pret && faits.dxvk_pose && faits.awesomewotlk_pret {
        etape(ID_ETAPE_PREPARATION, TITRE_ETAPE_PREPARATION, ETAT_FAIT, "Le jeu est prêt à être lancé.".to_string())
    } else {
        let mut reste = Vec::new();
        if !faits.prefixe_pret {
            reste.push("l'environnement Wine");
        }
        if !faits.dxvk_pose {
            reste.push("les composants graphiques (DXVK)");
        }
        if !faits.awesomewotlk_pret {
            reste.push("la connexion automatique");
        }
        etape(ID_ETAPE_PREPARATION, TITRE_ETAPE_PREPARATION, ETAT_A_FAIRE, format!("Reste à préparer : {}.", reste.join(" et ")))
    };
    vec![jeu, logiciels, preparation]
}

pub fn reponse_depuis(faits: &Faits) -> ReponseInstallation {
    let etapes = etapes_depuis(faits);
    ReponseInstallation {
        pret: etapes.iter().all(|e| e.etat == ETAT_FAIT),
        jeu_configure: faits.dossier_jeu.is_some(),
        dossier_jeu: faits.dossier_jeu.clone(),
        dossier_jeu_valide: faits.jeu_valide(),
        distribution: faits.distribution.clone().unwrap_or_else(|| DISTRIBUTION_INCONNUE.to_string()),
        installation_possible: faits.installation_possible,
        wine: WineAffiche { present: faits.wine_present, version: faits.wine_version.clone() },
        prefixe_pret: faits.prefixe_pret,
        dxvk_pose: faits.dxvk_pose,
        awesomewotlk_pret: faits.awesomewotlk_pret,
        etapes,
    }
}

/// La distribution est détectée ET le plan du cœur la prend en charge.
pub fn distribution_prise_en_charge(systeme: &Systeme) -> bool {
    systeme.distribution.as_ref().is_some_and(|d| d.famille != FamillePaquets::Inconnue)
}

/// Les noms de fichiers du jeu se comparent sans tenir compte de la casse (Wine fait de même).
fn meme_nom(trouve: &std::ffi::OsStr, attendu: &str) -> bool {
    trouve.to_string_lossy().eq_ignore_ascii_case(attendu)
}

/// Dossier du jeu utilisable : absolu, existant, avec `Wow.exe` et un dossier `Data`. L'erreur dit
/// ce qui manque ; le chemin n'est cité que tel que l'utilisateur l'a saisi.
pub fn verifier_dossier_jeu(chemin: &Path) -> std::result::Result<(), String> {
    let saisi = chemin.to_string_lossy();
    if saisi.trim().is_empty() {
        return Err("Aucun dossier n'a été indiqué.".to_string());
    }
    if saisi.chars().any(char::is_control) {
        return Err("Le chemin contient un caractère interdit (caractère de contrôle ou NUL).".to_string());
    }
    if !chemin.is_absolute() {
        return Err(format!("Le chemin « {saisi} » n'est pas absolu : indiquez le chemin complet du dossier du jeu."));
    }
    match fs::metadata(chemin) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(format!("« {saisi} » n'est pas un dossier.")),
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => return Err(format!("Le dossier « {saisi} » n'existe pas.")),
        Err(erreur) => return Err(format!("Le dossier « {saisi} » n'est pas lisible ({erreur}).")),
    }
    let entrees = fs::read_dir(chemin).map_err(|erreur| format!("Le contenu du dossier « {saisi} » n'est pas lisible ({erreur})."))?;
    let (mut executable, mut donnees) = (false, false);
    for entree in entrees {
        let entree = entree.map_err(|erreur| format!("Le contenu du dossier « {saisi} » n'est pas lisible ({erreur})."))?;
        let nom = entree.file_name();
        // Un lien cassé compte comme absent : il ne mène à rien d'utilisable.
        let (est_fichier, est_dossier) = match fs::metadata(entree.path()) {
            Ok(meta) => (meta.is_file(), meta.is_dir()),
            Err(_) => (false, false),
        };
        if meme_nom(&nom, FICHIER_JEU) && est_fichier {
            executable = true;
        }
        if meme_nom(&nom, DOSSIER_DONNEES_JEU) && est_dossier {
            donnees = true;
        }
    }
    match (executable, donnees) {
        (true, true) => Ok(()),
        (false, true) => Err(format!("Ce dossier n'est pas celui du jeu : {FICHIER_JEU} est introuvable.")),
        (true, false) => Err(format!("Ce dossier n'est pas celui du jeu : le dossier {DOSSIER_DONNEES_JEU} est introuvable.")),
        (false, false) => {
            Err(format!("Ce dossier n'est pas celui du jeu : {FICHIER_JEU} et le dossier {DOSSIER_DONNEES_JEU} sont introuvables."))
        }
    }
}

// ---------- lecture du préfixe (sans rien écrire) ----------

fn fichier_present(chemin: &Path) -> std::result::Result<bool, Erreur> {
    match fs::symlink_metadata(chemin) {
        Ok(meta) => Ok(meta.is_file()),
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => Ok(false),
        Err(erreur) => Err(Erreur::io("lire le préfixe Wine", chemin, erreur)),
    }
}

/// Préfixe prêt = le repère `system.reg` de Wine existe (même critère que `prefixe::creer`).
pub fn prefixe_est_pret(prefixe: &Path) -> std::result::Result<bool, Erreur> {
    fichier_present(&prefixe.join(REPERE_PREFIXE_PRET))
}

/// DXVK posé = les cinq DLL sont des fichiers dans `syswow64` (x32) et `system32` (x64). Seule la
/// présence est vérifiée, pas le contenu : `poser_dxvk` compare le contenu quand il pose.
pub fn dxvk_est_pose(prefixe: &Path) -> std::result::Result<bool, Erreur> {
    for sous_chemin in [SOUS_CHEMIN_SYSWOW64, SOUS_CHEMIN_SYSTEM32] {
        let mut dossier: PathBuf = prefixe.to_path_buf();
        dossier.extend(sous_chemin);
        for dll in DLL_REQUISES {
            if !fichier_present(&dossier.join(format!("{dll}.{EXTENSION_DLL}")))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

// ---------- service ----------

impl Service {
    fn executeur_requis(&self, objet: &str) -> Resultat<Arc<dyn Executeur + Send + Sync>> {
        self.executeur
            .clone()
            .ok_or_else(|| ErreurService::nouvelle(CODE_PORT_ABSENT, format!("{objet} est indisponible dans cette installation.")))
    }

    fn sonder_systeme(&self, executeur: &Arc<dyn Executeur + Send + Sync>) -> Resultat<Systeme> {
        self.lecture_silencieuse(|journal| sonder(&self.racine_systeme, &self.environnement, executeur.as_ref(), journal))
    }

    fn faits(&self, config: &Config, systeme: &Systeme) -> Resultat<Faits> {
        let raison_jeu_invalide = match &config.dossier_jeu {
            Some(dossier) => verifier_dossier_jeu(dossier).err(),
            None => None,
        };
        let dossier_prefixe = config.dossier_prefixe_effectif(&self.dossiers);
        Ok(Faits {
            dossier_jeu: config.dossier_jeu.as_ref().map(|d| d.to_string_lossy().into_owned()),
            raison_jeu_invalide,
            distribution: systeme.distribution.as_ref().map(|d| d.nom.clone()),
            installation_possible: distribution_prise_en_charge(systeme),
            wine_present: systeme.wine.is_some(),
            wine_version: systeme.wine.as_ref().and_then(|w| w.version.clone()),
            prefixe_pret: prefixe_est_pret(&dossier_prefixe)?,
            dxvk_pose: dxvk_est_pose(&dossier_prefixe)?,
            awesomewotlk_pret: awesomewotlk::dll_prete(
                &awesomewotlk::chemin_dll(&self.dossiers.donnees, self.awesomewotlk_archive.version, &self.awesomewotlk_dll),
                &self.awesomewotlk_dll,
            )?,
        })
    }

    fn reponse_installation(&self, config: &Config, systeme: &Systeme) -> Resultat<ReponseInstallation> {
        Ok(reponse_depuis(&self.faits(config, systeme)?))
    }

    /// Journalise l'échec avec son code et sa cause, et rend l'erreur du contrat.
    fn echec(&self, message: &str, erreur: Erreur) -> Resultat<ErreurService> {
        let rendue = ErreurService::from(erreur);
        let contexte = BTreeMap::from([("code".to_string(), rendue.code.clone())]);
        self.journal.evenement(Niveau::Erreur, ETAPE_SERVICE, message, &contexte, Some(rendue.message.clone()))?;
        Ok(rendue)
    }

    /// Commande `installation`.
    pub fn installation(&self) -> Resultat<ReponseInstallation> {
        let executeur = self.executeur_requis("La détection des logiciels")?;
        let config = self.charger_config()?;
        let systeme = self.sonder_systeme(&executeur)?;
        self.reponse_installation(&config, &systeme)
    }

    /// Commande `definir_dossier_jeu`.
    pub fn definir_dossier_jeu(&self, chemin: &str) -> Resultat<ReponseInstallation> {
        let _garde = self.garde();
        let dossier = PathBuf::from(chemin);
        if let Err(raison) = verifier_dossier_jeu(&dossier) {
            let contexte = BTreeMap::from([("raison".to_string(), raison.clone())]);
            self.journal.evenement(
                Niveau::Avertissement,
                ETAPE_SERVICE,
                "Dossier du jeu refusé",
                &contexte,
                Some(CODE_DOSSIER_JEU_INVALIDE.to_string()),
            )?;
            return Err(ErreurService::nouvelle(CODE_DOSSIER_JEU_INVALIDE, raison));
        }
        let mut config = self.charger_config()?;
        config.dossier_jeu = Some(dossier);
        if let Err(erreur) = config.enregistrer(&self.dossiers) {
            self.journal.journaliser_erreur(ETAPE_SERVICE, &erreur)?;
            return Err(erreur.into());
        }
        let contexte = BTreeMap::from([("dossier".to_string(), chemin.to_string())]);
        self.journal.evenement(Niveau::Info, ETAPE_SERVICE, "Dossier du jeu enregistré", &contexte, None)?;
        let executeur = self.executeur_requis("La détection des logiciels")?;
        let systeme = self.sonder_systeme(&executeur)?;
        self.reponse_installation(&config, &systeme)
    }

    /// Commande `installer_logiciels` : exécute le plan du cœur (une seule commande élevée groupée).
    pub fn installer_logiciels(&self) -> Resultat<ReponseLogiciels> {
        let executeur = self.executeur_requis("L'installation des logiciels")?;
        let Some(_verrou) = self.garde_installation() else {
            return Err(ErreurService::nouvelle(CODE_DEJA_EN_COURS, "Une installation ou une préparation est déjà en cours."));
        };
        let systeme = self.sonder_systeme(&executeur)?;
        if !distribution_prise_en_charge(&systeme) {
            self.journal.evenement(
                Niveau::Avertissement,
                ETAPE_SERVICE,
                "Installation des logiciels refusée : distribution non prise en charge",
                &BTreeMap::new(),
                Some(CODE_DISTRIBUTION_NON_PRISE_EN_CHARGE.to_string()),
            )?;
            return Err(ErreurService::nouvelle(
                CODE_DISTRIBUTION_NON_PRISE_EN_CHARGE,
                "Votre distribution n'est pas prise en charge : installez Wine avec le gestionnaire de paquets de votre système.",
            ));
        }
        match installation::installer(&systeme, Mode::Reel, executeur.as_ref(), &self.journal) {
            Ok(rapport) => {
                let mut etapes = Vec::new();
                for resultat in &rapport.resultats {
                    let statut = match resultat.statut {
                        Statut::Reussie => STATUT_REUSSIE,
                        Statut::Simulee => STATUT_SIMULEE,
                    };
                    let contexte = BTreeMap::from([
                        ("etape".to_string(), resultat.description.clone()),
                        ("statut".to_string(), statut.to_string()),
                        ("duree_ms".to_string(), resultat.duree_ms.to_string()),
                    ]);
                    self.journal.evenement(Niveau::Info, ETAPE_SERVICE, "Étape d'installation terminée", &contexte, None)?;
                    etapes.push(EtapeExecutee { description: resultat.description.clone(), statut: statut.to_string() });
                }
                Ok(ReponseLogiciels { etapes, message: rapport.message })
            }
            Err(erreur) => {
                Err(self.echec("Installation des logiciels échouée", erreur)?)
            }
        }
    }

    /// Commande `preparer_jeu` : préfixe Wine puis DXVK ; ne refait rien si tout est déjà en place.
    pub fn preparer_jeu(&self) -> Resultat<ReponseInstallation> {
        let executeur = self.executeur_requis("La préparation du jeu")?;
        let telechargeur = self
            .telechargeur
            .clone()
            .ok_or_else(|| ErreurService::nouvelle(CODE_PORT_ABSENT, "Le téléchargement est indisponible dans cette installation."))?;
        let Some(_verrou) = self.garde_installation() else {
            return Err(ErreurService::nouvelle(CODE_DEJA_EN_COURS, "Une installation ou une préparation est déjà en cours."));
        };
        let config = self.charger_config()?;
        let systeme = self.sonder_systeme(&executeur)?;
        let faits = self.faits(&config, &systeme)?;
        let manquants = manquants_preparation(&faits);
        if !manquants.is_empty() {
            let message = format!("Impossible de préparer le jeu : il manque {}.", manquants.join(" et "));
            self.journal.evenement(
                Niveau::Avertissement,
                ETAPE_SERVICE,
                &message,
                &BTreeMap::new(),
                Some(CODE_PREPARATION_IMPOSSIBLE.to_string()),
            )?;
            return Err(ErreurService::nouvelle(CODE_PREPARATION_IMPOSSIBLE, message));
        }
        if faits.prefixe_pret && faits.dxvk_pose && faits.awesomewotlk_pret {
            self.journal.info(ETAPE_SERVICE, "Préparation du jeu : tout est déjà en place, rien à refaire")?;
            return Ok(reponse_depuis(&faits));
        }
        let dossier_prefixe = config.dossier_prefixe_effectif(&self.dossiers);
        let resultat = prefixe::creer(&dossier_prefixe, executeur.as_ref(), &self.journal)
            .and_then(|etat| {
                let source = dxvk::obtenir(&self.dossiers.donnees, telechargeur.as_ref(), &self.journal)?;
                let bilan = prefixe::poser_dxvk(&dossier_prefixe, &source, &self.journal)?;
                awesomewotlk::obtenir_verifiee(
                    &self.dossiers.donnees,
                    &self.awesomewotlk_archive,
                    &self.awesomewotlk_dll,
                    telechargeur.as_ref(),
                    &self.journal,
                )?;
                Ok((etat, source, bilan.len()))
            });
        match resultat {
            Ok((etat, source, dll)) => {
                let contexte = BTreeMap::from([
                    ("prefixe".to_string(), format!("{etat:?}")),
                    ("dxvk".to_string(), source.version),
                    ("dll".to_string(), dll.to_string()),
                    ("awesomewotlk".to_string(), self.awesomewotlk_archive.version.to_string()),
                ]);
                self.journal.evenement(Niveau::Info, ETAPE_SERVICE, "Jeu préparé", &contexte, None)?;
            }
            Err(erreur) => {
                return Err(self.echec("Préparation du jeu échouée", erreur)?);
            }
        }
        self.reponse_installation(&config, &systeme)
    }
}
