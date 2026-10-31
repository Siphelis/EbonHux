//! Commande `ouvrir_site` (docs/interface.md, décision 11) : ouvre l'adresse https de la
//! configuration avec le programme du bureau. L'ouverture passe par un port pour les essais.

use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use ebonhux_coeur::config::valider_site;
use ebonhux_coeur::executeur::nettoyer_environnement;
use serde::Serialize;

use crate::comptes::Service;
use crate::erreur::{CODE_LANCEMENT_ECHOUE, CODE_PORT_ABSENT, CODE_SITE_NON_CONFIGURE, ErreurService, Resultat};

pub const PROGRAMME_OUVERTURE: &str = "xdg-open";
const ETAPE_SITE: &str = "site";

pub trait Ouvreur: Send + Sync {
    fn ouvrir(&self, adresse: &str) -> Resultat<()>;
}

/// `xdg-open <adresse>` lancé détaché, sans shell : l'adresse est un seul argument.
#[derive(Debug, Default, Clone, Copy)]
pub struct OuvreurReel;

impl Ouvreur for OuvreurReel {
    fn ouvrir(&self, adresse: &str) -> Resultat<()> {
        // Une adresse qui ne passe pas la validation (https, sans espace) ne devient jamais une option du programme.
        valider_site(adresse)?;
        let mut processus = std::process::Command::new(PROGRAMME_OUVERTURE);
        nettoyer_environnement(&mut processus);
        let mut enfant = processus
            .arg(adresse)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|erreur| {
                ErreurService::nouvelle(
                    CODE_LANCEMENT_ECHOUE,
                    format!("Impossible d'ouvrir le site avec « {PROGRAMME_OUVERTURE} » : {erreur}"),
                )
            })?;
        thread::spawn(move || {
            if let Err(erreur) = enfant.wait() {
                eprintln!("Attente de « {PROGRAMME_OUVERTURE} » impossible : {erreur}");
            }
        });
        Ok(())
    }
}

/// Ouvreur des essais : garde les adresses reçues, n'ouvre rien.
#[derive(Debug, Default)]
pub struct OuvreurSimule {
    ouvertures: Mutex<Vec<String>>,
    raison_echec: Option<String>,
}

fn verrouiller<T>(verrou: &Mutex<T>) -> MutexGuard<'_, T> {
    verrou.lock().unwrap_or_else(PoisonError::into_inner)
}

impl OuvreurSimule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ouvreur dont toute ouverture échoue avec cette raison.
    pub fn qui_echoue(raison: &str) -> Self {
        Self { ouvertures: Mutex::new(Vec::new()), raison_echec: Some(raison.to_string()) }
    }

    pub fn ouvertures(&self) -> Vec<String> {
        verrouiller(&self.ouvertures).clone()
    }
}

impl Ouvreur for OuvreurSimule {
    fn ouvrir(&self, adresse: &str) -> Resultat<()> {
        if let Some(raison) = &self.raison_echec {
            return Err(ErreurService::nouvelle(CODE_LANCEMENT_ECHOUE, raison.clone()));
        }
        verrouiller(&self.ouvertures).push(adresse.to_string());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseVide {}

impl Service {
    /// Commande `ouvrir_site`.
    pub fn ouvrir_site(&self) -> Resultat<ReponseVide> {
        let config = self.charger_config()?;
        let Some(site) = config.site else {
            return Err(ErreurService::nouvelle(CODE_SITE_NON_CONFIGURE, "Adresse du site non configurée"));
        };
        let Some(ouvreur) = &self.ouvreur else {
            return Err(ErreurService::nouvelle(CODE_PORT_ABSENT, "Ouverture d'adresse indisponible dans cette installation."));
        };
        ouvreur.ouvrir(&site)?;
        self.journal.info(ETAPE_SITE, "Site du projet ouvert")?;
        Ok(ReponseVide {})
    }
}
