//! Ligne de commande d'EbonHux : bibliothèque fine (analyse des arguments, commandes), le
//! binaire `ebonhux` ne fait que brancher les implémentations réelles.

pub mod coffre_systeme;
pub mod commandes;
pub mod contexte;
pub mod interface;
pub mod sortie;

use std::ffi::OsString;
use std::io::Write;

use clap::error::{ContextKind, ErrorKind};
use ebonhux_coeur::erreur::Erreur;
use ebonhux_coeur::journal::Journal;

pub use contexte::Contexte;

pub const CODE_SUCCES: u8 = 0;
pub const CODE_ERREUR_EXECUTION: u8 = 1;
pub const CODE_ERREUR_USAGE: u8 = 2;

const INVITE_AIDE: &str = "Essayez « ebonhux --help ».";

/// Point d'entrée : analyse les arguments (le premier est le nom du programme), lance la
/// commande et rend le code de sortie. Aucun `exit` ici : l'appelant sort une seule fois.
pub fn executer(arguments: Vec<OsString>, contexte: &mut Contexte<'_>) -> u8 {
    let cli = match interface::analyser(arguments) {
        Ok(cli) => cli,
        Err(erreur) => return rendre_usage(&erreur, contexte),
    };
    let journal = Journal::vers_fichier(&contexte.dossiers);
    match commandes::lancer(&cli.commande, contexte, &journal) {
        Ok(()) => CODE_SUCCES,
        Err(erreur) => signaler(cli.commande.nom(), &erreur, contexte, &journal),
    }
}

/// Erreur avant même la construction du contexte (dossier personnel inconnu).
pub fn echec_demarrage(erreur_flux: &mut dyn Write, erreur: &Erreur) -> u8 {
    // Si la sortie d'erreur est elle-même inutilisable, il n'existe aucun autre canal.
    if writeln!(erreur_flux, "Erreur : {erreur}").is_err() {
        return CODE_ERREUR_EXECUTION;
    }
    CODE_ERREUR_EXECUTION
}

fn signaler(etape: &str, erreur: &Erreur, contexte: &mut Contexte<'_>, journal: &Journal) -> u8 {
    let journalisation = journal.journaliser_erreur(etape, erreur);
    let mut texte = format!("Erreur : {erreur}");
    if let Err(panne) = journalisation {
        texte.push_str(&format!("\nErreur supplémentaire : le journal est inutilisable : {panne}"));
    }
    // Pas d'autre canal que la sortie d'erreur : si elle échoue, le code non nul reste.
    if sortie::ligne(contexte.erreur, &texte).is_err() {
        return CODE_ERREUR_EXECUTION;
    }
    CODE_ERREUR_EXECUTION
}

fn rendre_usage(erreur: &clap::Error, contexte: &mut Contexte<'_>) -> u8 {
    let (flux, code, texte) = match erreur.kind() {
        ErrorKind::DisplayHelp => (&mut *contexte.sortie, CODE_SUCCES, erreur.to_string()),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            (&mut *contexte.erreur, CODE_ERREUR_USAGE, erreur.to_string())
        }
        _ => (&mut *contexte.erreur, CODE_ERREUR_USAGE, message_usage(erreur)),
    };
    if write!(flux, "{texte}").is_err() {
        return CODE_ERREUR_EXECUTION;
    }
    code
}

fn contexte_de(erreur: &clap::Error, clef: ContextKind) -> String {
    erreur.get(clef).map_or_else(String::new, ToString::to_string)
}

fn message_usage(erreur: &clap::Error) -> String {
    let phrase = match erreur.kind() {
        ErrorKind::InvalidSubcommand => {
            format!("sous-commande inconnue : « {} »", contexte_de(erreur, ContextKind::InvalidSubcommand))
        }
        ErrorKind::UnknownArgument => {
            format!("option ou argument inconnu : « {} »", contexte_de(erreur, ContextKind::InvalidArg))
        }
        ErrorKind::MissingRequiredArgument => {
            format!("argument obligatoire manquant : {}", contexte_de(erreur, ContextKind::InvalidArg))
        }
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => format!(
            "valeur « {} » invalide pour {}",
            contexte_de(erreur, ContextKind::InvalidValue),
            contexte_de(erreur, ContextKind::InvalidArg)
        ),
        ErrorKind::MissingSubcommand => "sous-commande manquante".to_string(),
        _ => {
            let brut = erreur.to_string();
            let premiere = brut.lines().next().unwrap_or_default();
            premiere.trim_start_matches("error: ").to_string()
        }
    };
    format!("Utilisation incorrecte : {phrase}\n{INVITE_AIDE}\n")
}
