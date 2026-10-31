//! Une commande = un fichier.

pub mod addons;
pub mod comptes;
pub mod config;
pub mod diag;
pub mod installer;
pub mod jeu;
pub mod jouer;
pub mod prefixe;
pub mod rapport;

use ebonhux_coeur::erreur::Result;
use ebonhux_coeur::journal::Journal;

use crate::contexte::Contexte;
use crate::interface::Commande;

pub fn lancer(commande: &Commande, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    match commande {
        Commande::Config { action } => config::lancer(action, contexte, journal),
        Commande::Diag(options) => diag::lancer(options.json, contexte, journal),
        Commande::Rapport(options) => rapport::lancer(options.json, contexte, journal),
        Commande::Installer { executer } => installer::lancer(*executer, contexte, journal),
        Commande::Prefixe { action } => prefixe::lancer(action, contexte, journal),
        Commande::Comptes { action } => comptes::lancer(action, contexte, journal),
        Commande::Jouer { compte, a_blanc } => jouer::lancer(compte, *a_blanc, contexte, journal),
        Commande::Addons { action } => addons::lancer(action, contexte, journal),
    }
}
