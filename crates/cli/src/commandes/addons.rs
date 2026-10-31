use std::path::Path;

use ebonhux_coeur::addons::{
    Catalogue, OptionsInstallation, ResultatInstallation, catalogue_embarque, installer_depuis_url, installer_entree, lister,
};
use ebonhux_coeur::config::Config;
use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::reseau::Telechargeur;

use crate::commandes::jeu::dossier_jeu;
use crate::contexte::Contexte;
use crate::interface::ActionAddons;
use crate::sortie::ligne;

pub const CATALOGUE_VIDE: &str = "Le catalogue est vide : aucun addon communautaire n'est encore référencé.";

pub fn lancer(action: &ActionAddons, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    match action {
        ActionAddons::Lister => lister_installes(contexte, journal),
        ActionAddons::Catalogue => catalogue(contexte, journal),
        ActionAddons::Installer { nom, url, remplacer } => {
            let options = OptionsInstallation { remplacer: *remplacer, ..OptionsInstallation::default() };
            installer(nom.as_deref(), url.as_deref(), &options, contexte, journal)
        }
    }
}

fn lister_installes(contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let config = Config::charger(&contexte.dossiers)?;
    let jeu = dossier_jeu(&config)?;
    let liste = lister(&jeu, journal)?;
    if liste.dossier_absent {
        return ligne(
            contexte.sortie,
            &format!("Le dossier des addons n'existe pas encore ({}) : aucun addon installé.", ebonhux_coeur::addons::dossier_addons(&jeu).display()),
        );
    }
    ligne(contexte.sortie, &format!("Addons installés (hors Blizzard) : {}", liste.addons.len()))?;
    for addon in &liste.addons {
        let version = addon.fiche.version.as_deref().map_or(String::new(), |v| format!(" (version {v})"));
        ligne(contexte.sortie, &format!("  {} : {}{version}", addon.dossier, addon.fiche.titre))?;
    }
    ligne(contexte.sortie, &format!("Problèmes signalés : {}", liste.problemes.len()))?;
    for probleme in &liste.problemes {
        ligne(contexte.sortie, &format!("  {} : {}", probleme.dossier, probleme.raison))?;
    }
    Ok(())
}

fn catalogue(contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let catalogue = catalogue_embarque(journal)?;
    if catalogue.entrees.is_empty() && catalogue.invalides.is_empty() {
        return ligne(contexte.sortie, CATALOGUE_VIDE);
    }
    ligne(contexte.sortie, &format!("Catalogue d'addons : {} entrée(s)", catalogue.entrees.len()))?;
    for entree in &catalogue.entrees {
        ligne(contexte.sortie, &format!("  {} : {}", entree.nom, entree.description))?;
        ligne(contexte.sortie, &format!("    adresse : {} (dossier : {})", entree.url, entree.dossier.as_deref().unwrap_or("inconnu")))?;
    }
    if !catalogue.invalides.is_empty() {
        ligne(contexte.sortie, &format!("Entrées refusées du catalogue : {}", catalogue.invalides.len()))?;
        for invalide in &catalogue.invalides {
            ligne(contexte.sortie, &format!("  entrée n° {} : {}", invalide.position, invalide.raison))?;
        }
    }
    Ok(())
}

fn installer(
    nom: Option<&str>,
    url: Option<&str>,
    options: &OptionsInstallation,
    contexte: &mut Contexte<'_>,
    journal: &Journal,
) -> Result<()> {
    let config = Config::charger(&contexte.dossiers)?;
    let jeu = dossier_jeu(&config)?;
    let resultat = match (nom, url) {
        (_, Some(url)) => installer_depuis_url(&jeu, url, None, options, contexte.telechargeur, journal)?,
        (Some(nom), None) => {
            let catalogue = catalogue_embarque(journal)?;
            installer_nom(&catalogue, nom, &jeu, options, contexte.telechargeur, journal)?
        }
        (None, None) => return Err(Erreur::configuration("ni nom d'addon ni adresse (--url) n'a été donné")),
    };
    afficher_resultat(&resultat, contexte)
}

/// Installe l'entrée du catalogue de ce nom (casse ignorée) ; un nom inconnu liste les noms connus.
pub fn installer_nom(
    catalogue: &Catalogue,
    nom: &str,
    jeu: &Path,
    options: &OptionsInstallation,
    telechargeur: &dyn Telechargeur,
    journal: &Journal,
) -> Result<ResultatInstallation> {
    let Some(entree) = catalogue.entrees.iter().find(|e| e.nom.eq_ignore_ascii_case(nom)) else {
        let connus = if catalogue.entrees.is_empty() {
            "le catalogue est vide, aucun nom n'est connu".to_string()
        } else {
            let noms: Vec<&str> = catalogue.entrees.iter().map(|e| e.nom.as_str()).collect();
            format!("noms connus : {}", noms.join(", "))
        };
        return Err(Erreur::configuration(format!("« {nom} » n'est pas dans le catalogue d'addons ; {connus}")));
    };
    installer_entree(jeu, entree, options, telechargeur, journal)
}

fn afficher_resultat(resultat: &ResultatInstallation, contexte: &mut Contexte<'_>) -> Result<()> {
    ligne(contexte.sortie, &format!("Addons installés : {}", resultat.installes.join(", ")))?;
    Ok(())
}
