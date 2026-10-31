use std::collections::BTreeMap;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ebonhux_coeur::awesomewotlk::{ARCHIVE_EPINGLEE, DLL_EPINGLEE};
use ebonhux_cli::coffre_systeme::CoffreSysteme;
use ebonhux_cli::contexte::{Entree, Ports};
use ebonhux_cli::{Contexte, echec_demarrage, executer};
use ebonhux_coeur::config::Dossiers;
use ebonhux_coeur::executeur::ExecuteurReel;
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::lancement::LanceurReel;
use ebonhux_coeur::reseau::TelechargeurReel;

const RACINE_SYSTEME: &str = "/";

fn variables_d_environnement() -> BTreeMap<String, String> {
    std::env::vars_os()
        .map(|(nom, valeur)| (nom.to_string_lossy().into_owned(), valeur.to_string_lossy().into_owned()))
        .collect()
}

fn main() -> ExitCode {
    let mut sortie = io::stdout().lock();
    let mut erreur = io::stderr().lock();
    let code = match Dossiers::depuis_environnement() {
        Ok(dossiers) => {
            let executeur = ExecuteurReel;
            let telechargeur = TelechargeurReel::new();
            let coffre = CoffreSysteme;
            let journal = Arc::new(Journal::vers_fichier(&dossiers));
            let lanceur = LanceurReel::pour_production(&dossiers, journal);
            let interactif = io::stdin().is_terminal();
            let mut lecteur = io::stdin().lock();
            let mut contexte = Contexte::new(
                Ports { executeur: &executeur, telechargeur: &telechargeur, coffre: &coffre, lanceur: &lanceur, awesomewotlk: &DLL_EPINGLEE, awesomewotlk_archive: &ARCHIVE_EPINGLEE },
                dossiers,
                PathBuf::from(RACINE_SYSTEME),
                variables_d_environnement(),
                Entree { lecteur: &mut lecteur, interactif },
                &mut sortie,
                &mut erreur,
            );
            executer(std::env::args_os().collect(), &mut contexte)
        }
        Err(cause) => echec_demarrage(&mut erreur, &cause),
    };
    ExitCode::from(code)
}
