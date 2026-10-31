use std::ffi::OsString;

use clap::{Arg, ArgAction, Args, Command, CommandFactory, FromArgMatches, Parser, Subcommand};

const GABARIT_AIDE: &str = "{about}\n\nUtilisation : {usage}\n\n{all-args}{after-help}";
const TITRE_COMMANDES: &str = "Commandes";
const IDENTIFIANT_AIDE: &str = "aide";

/// EbonHux : lanceur Linux pour Project Ebonhold (ligne de commande d'essai)
#[derive(Parser, Debug)]
#[command(
    name = "ebonhux",
    disable_help_flag = true,
    disable_help_subcommand = true,
    disable_version_flag = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub commande: Commande,
}

#[derive(Subcommand, Debug)]
pub enum Commande {
    /// Afficher ou modifier la configuration
    Config {
        #[command(subcommand)]
        action: ActionConfig,
    },
    /// Sonder la machine (distribution, Wine, Lutris, Vulkan, pilote, session)
    Diag(OptionJson),
    /// Exporter le journal des opérations (sans secret)
    Rapport(OptionJson),
    /// Installer Lutris et Wine par le gestionnaire de paquets (simulation par défaut)
    Installer {
        /// Exécuter réellement l'installation (le système demandera le mot de passe administrateur)
        #[arg(long)]
        executer: bool,
    },
    /// Gérer le préfixe Wine
    Prefixe {
        #[command(subcommand)]
        action: ActionPrefixe,
    },
    /// Gérer les comptes du jeu et leur mot de passe (conservé dans le trousseau du système)
    Comptes {
        #[command(subcommand)]
        action: ActionComptes,
    },
    /// Lancer le jeu pour un compte
    #[command(after_help = NOTE_PS)]
    Jouer {
        /// Nom du compte (comparé sans tenir compte de la casse)
        compte: String,
        /// Montrer ce qui serait fait, sans rien vider ni lancer
        #[arg(long = "a-blanc")]
        a_blanc: bool,
    },
    /// Lister et installer les addons du jeu
    Addons {
        #[command(subcommand)]
        action: ActionAddons,
    },
}

/// Conséquence connue du passage du mot de passe en argument du jeu (décision du propriétaire).
const NOTE_PS: &str = "Note : tant que le jeu tourne, le mot de passe passé au jeu reste lisible par les autres utilisateurs de la machine dans la liste des processus (ps) ; monter /proc avec l'option hidepid=2 réduit ce risque.\n";

#[derive(Args, Debug)]
pub struct OptionJson {
    /// Sortie au format JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum ActionConfig {
    /// Afficher la configuration actuelle
    Afficher,
    /// Définir une clef (dossier_jeu, dossier_prefixe, royaume.realmlist, royaume.nom, site)
    Definir {
        /// Nom de la clef
        clef: String,
        /// Nouvelle valeur
        valeur: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ActionPrefixe {
    /// Créer et initialiser le préfixe (wineboot)
    Creer,
    /// Poser les DLL DXVK dans le préfixe
    Dxvk,
    /// Télécharger et vérifier AwesomeWotlk v37 (connexion automatique)
    Awesomewotlk,
    /// Arrêter le serveur Wine du préfixe
    Arreter,
}

#[derive(Subcommand, Debug)]
pub enum ActionComptes {
    /// Lister les comptes du jeu et dire si leur mot de passe est enregistré
    Lister(OptionJson),
    /// Enregistrer le mot de passe d'un compte dans le trousseau (lu sur l'entrée standard, une ligne)
    Enregistrer {
        /// Nom du compte
        compte: String,
        /// Accepter un compte qui n'a pas encore de dossier dans WTF/Account
        #[arg(long)]
        nouveau: bool,
    },
    /// Retirer le mot de passe d'un compte du trousseau
    Oublier {
        /// Nom du compte
        compte: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ActionAddons {
    /// Lister les addons installés (hors Blizzard) et les problèmes signalés
    Lister,
    /// Afficher le catalogue d'addons embarqué
    Catalogue,
    /// Installer un addon du catalogue ou depuis une adresse https vers un zip
    Installer {
        /// Nom de l'addon dans le catalogue
        #[arg(required_unless_present = "url", conflicts_with = "url")]
        nom: Option<String>,
        /// Adresse https:// d'un zip à installer (hors catalogue)
        #[arg(long)]
        url: Option<String>,
        /// Remplacer un addon déjà installé (l'ancien dossier est remplacé)
        #[arg(long)]
        remplacer: bool,
    },
}

impl Commande {
    /// Nom de l'étape dans le journal.
    pub fn nom(&self) -> &'static str {
        match self {
            Self::Config { .. } => "cli.config",
            Self::Diag(_) => "cli.diag",
            Self::Rapport(_) => "cli.rapport",
            Self::Installer { .. } => "cli.installer",
            Self::Prefixe { .. } => "cli.prefixe",
            Self::Comptes { .. } => "cli.comptes",
            Self::Jouer { .. } => "cli.jouer",
            Self::Addons { .. } => "cli.addons",
        }
    }
}

fn en_francais(commande: Command) -> Command {
    commande
        .help_template(GABARIT_AIDE)
        .subcommand_help_heading(TITRE_COMMANDES)
        .subcommand_value_name("COMMANDE")
        .mut_subcommands(en_francais)
}

/// La commande clap complète : l'aide (`-h`, `--help`) est déclarée ici pour être en français.
pub fn commande() -> Command {
    en_francais(Cli::command().arg(
        Arg::new(IDENTIFIANT_AIDE)
            .short('h')
            .long("help")
            .global(true)
            .action(ArgAction::Help)
            .help("Afficher l'aide"),
    ))
}

pub fn analyser(arguments: Vec<OsString>) -> Result<Cli, clap::Error> {
    let correspondances = commande().try_get_matches_from(arguments)?;
    Cli::from_arg_matches(&correspondances)
}
