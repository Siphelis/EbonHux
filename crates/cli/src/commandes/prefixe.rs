use std::path::PathBuf;

use ebonhux_coeur::awesomewotlk;
use ebonhux_coeur::config::Config;
use ebonhux_coeur::dxvk::{Origine, SourceDxvk, obtenir};
use ebonhux_coeur::erreur::Result;
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::prefixe::{EtatArret, EtatPrefixe, ResultatDll, arreter, creer, poser_dxvk};

use crate::contexte::Contexte;
use crate::interface::ActionPrefixe;
use crate::sortie::ligne;

const PROVENANCE_CONFIGURE: &str = "configuré";
const PROVENANCE_DEFAUT: &str = "par défaut";

pub fn lancer(action: &ActionPrefixe, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let (prefixe, provenance) = dossier_prefixe(contexte)?;
    ligne(contexte.sortie, &format!("Préfixe Wine : {} ({provenance})", prefixe.display()))?;
    match action {
        ActionPrefixe::Creer => {
            let etat = creer(&prefixe, contexte.executeur, journal)?;
            let texte = match etat {
                EtatPrefixe::Cree => "Préfixe créé",
                EtatPrefixe::DejaPret => "Préfixe déjà prêt, rien à faire",
            };
            ligne(contexte.sortie, &format!("{texte} : {}", prefixe.display()))
        }
        ActionPrefixe::Dxvk => dxvk(&prefixe, contexte, journal),
        ActionPrefixe::Awesomewotlk => connexion_automatique(contexte, journal),
        ActionPrefixe::Arreter => {
            let texte = match arreter(&prefixe, contexte.executeur, journal)? {
                EtatArret::Arrete => format!("Serveur Wine arrêté pour le préfixe : {}", prefixe.display()),
                EtatArret::AucunServeur => "Aucun serveur Wine actif pour ce préfixe : rien à arrêter.".to_string(),
            };
            ligne(contexte.sortie, &texte)
        }
    }
}

/// Dossier du préfixe et sa provenance : la configuration, sinon le défaut du cœur.
fn dossier_prefixe(contexte: &Contexte<'_>) -> Result<(PathBuf, &'static str)> {
    let config = Config::charger(&contexte.dossiers)?;
    let provenance = if config.dossier_prefixe.is_some() { PROVENANCE_CONFIGURE } else { PROVENANCE_DEFAUT };
    Ok((config.dossier_prefixe_effectif(&contexte.dossiers), provenance))
}

fn origine(source: &SourceDxvk) -> String {
    match source.origine {
        Origine::RuntimeLutris => format!("runtime de Lutris, version {}", source.version),
        Origine::Telechargee => format!("version épinglée {} (téléchargée ou déjà en cache)", source.version),
    }
}

fn dxvk(prefixe: &std::path::Path, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let source = obtenir(&contexte.dossiers.donnees, contexte.telechargeur, journal)?;
    ligne(contexte.sortie, &format!("Source DXVK : {} ({})", origine(&source), source.dossier.display()))?;
    for (chemin, resultat) in poser_dxvk(prefixe, &source, journal)? {
        let texte = match resultat {
            ResultatDll::Posee => "posée",
            ResultatDll::Remplacee => "remplacée",
            ResultatDll::DejaIdentique => "déjà identique",
        };
        ligne(contexte.sortie, &format!("  {} : {texte}", chemin.display()))?;
    }
    ligne(contexte.sortie, "DXVK posé dans le préfixe.")
}

fn connexion_automatique(contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let chemin = awesomewotlk::obtenir_verifiee(
        &contexte.dossiers.donnees,
        contexte.awesomewotlk_archive,
        contexte.awesomewotlk,
        contexte.telechargeur,
        journal,
    )?;
    ligne(
        contexte.sortie,
        &format!("Connexion automatique (AwesomeWotlk {}) prête : {}", contexte.awesomewotlk_archive.version, chemin.display()),
    )
}
