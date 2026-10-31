use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use ebonhux_coeur::awesomewotlk::DllAttendue;
use ebonhux_coeur::coffre::CoffreSecrets;
use ebonhux_coeur::config::Dossiers;
use ebonhux_coeur::dxvk::ArchiveAttendue;
use ebonhux_coeur::executeur::Executeur;
use ebonhux_coeur::lancement::LanceurDetache;
use ebonhux_coeur::reseau::Telechargeur;

/// Les ports qui touchent la machine : réels dans le binaire, simulés dans les essais.
#[derive(Clone, Copy)]
pub struct Ports<'a> {
    pub executeur: &'a dyn Executeur,
    pub telechargeur: &'a dyn Telechargeur,
    pub coffre: &'a dyn CoffreSecrets,
    pub lanceur: &'a dyn LanceurDetache,
    /// DLL AwesomeWotlk attendue : la v37 en production, une DLL factice dans les essais.
    pub awesomewotlk: &'a DllAttendue<'a>,
    /// Archive qui contient cette DLL : la v37 en production, un zip factice dans les essais.
    pub awesomewotlk_archive: &'a ArchiveAttendue<'a>,
}

/// Entrée standard injectable. `interactif` : l'entrée est un terminal (l'écho est alors coupé
/// pendant la saisie d'un mot de passe) ; sinon elle est lue telle quelle (redirection, essais).
pub struct Entree<'a> {
    pub lecteur: &'a mut dyn BufRead,
    pub interactif: bool,
}

/// Tout ce que les commandes touchent du monde extérieur, injectable pour les essais.
/// `#[non_exhaustive]` : les appelants passent par `Contexte::new`.
#[non_exhaustive]
pub struct Contexte<'a> {
    pub executeur: &'a dyn Executeur,
    pub telechargeur: &'a dyn Telechargeur,
    pub coffre: &'a dyn CoffreSecrets,
    pub lanceur: &'a dyn LanceurDetache,
    pub awesomewotlk: &'a DllAttendue<'a>,
    pub awesomewotlk_archive: &'a ArchiveAttendue<'a>,
    pub dossiers: Dossiers,
    /// Racine sous laquelle les sondes lisent `/etc`, `/proc`, `/usr`.
    pub racine_systeme: PathBuf,
    /// Variables d'environnement visibles des sondes (PATH, session).
    pub environnement: BTreeMap<String, String>,
    pub entree: Entree<'a>,
    pub sortie: &'a mut dyn Write,
    pub erreur: &'a mut dyn Write,
}

impl<'a> Contexte<'a> {
    pub fn new(
        ports: Ports<'a>,
        dossiers: Dossiers,
        racine_systeme: PathBuf,
        environnement: BTreeMap<String, String>,
        entree: Entree<'a>,
        sortie: &'a mut dyn Write,
        erreur: &'a mut dyn Write,
    ) -> Self {
        Self {
            executeur: ports.executeur,
            telechargeur: ports.telechargeur,
            coffre: ports.coffre,
            lanceur: ports.lanceur,
            awesomewotlk: ports.awesomewotlk,
            awesomewotlk_archive: ports.awesomewotlk_archive,
            dossiers,
            racine_systeme,
            environnement,
            entree,
            sortie,
            erreur,
        }
    }
}
