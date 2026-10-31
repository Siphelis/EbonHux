use std::process::{Command, Stdio};

use ebonhux_coeur::coffre::normaliser_nom_compte;
use ebonhux_coeur::comptes::{CompteJeu, lister_comptes_jeu, trouver_compte};
use ebonhux_coeur::config::Config;
use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::secret::Secret;
use serde_json::json;

use crate::commandes::jeu::dossier_jeu;
use crate::contexte::Contexte;
use crate::interface::ActionComptes;
use crate::sortie::ligne;

const ETAPE: &str = "cli.comptes";
const NOM_ENTREE: &str = "<entrée standard>";
const PROGRAMME_SAISIE: &str = "sh";
const CODE_INTERRUPTION: i32 = 130;
/// Coupe l'écho du terminal, lit une ligne, rétablit l'état d'origine à la sortie normale comme
/// sur INT, TERM ou HUP. Le mot de passe ne transite que par le tube de sortie du processus :
/// ni argument, ni variable d'environnement.
const SCRIPT_SAISIE: &str = r#"etat=$(stty -g) || { echo "stty ne peut pas lire l'état du terminal" >&2; exit 3; }
trap 'stty "$etat"' EXIT
trap 'stty "$etat"; exit 130' INT TERM HUP
stty -echo || exit 3
IFS= read -r ligne
stty "$etat"
printf '%s' "$ligne"
"#;

pub fn lancer(action: &ActionComptes, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    match action {
        ActionComptes::Lister(options) => lister(options.json, contexte, journal),
        ActionComptes::Enregistrer { compte, nouveau } => enregistrer(compte, *nouveau, contexte, journal),
        ActionComptes::Oublier { compte } => oublier(compte, contexte, journal),
    }
}

fn mot_de_passe_enregistre(contexte: &Contexte<'_>, compte: &CompteJeu) -> Result<bool> {
    Ok(contexte.coffre.lire(&compte.nom)?.is_some_and(|secret| !secret.est_vide()))
}

fn lister(json: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let config = Config::charger(&contexte.dossiers)?;
    let jeu = dossier_jeu(&config)?;
    let comptes = lister_comptes_jeu(&jeu, journal)?;
    let mut lignes = Vec::new();
    for compte in &comptes {
        lignes.push((compte, mot_de_passe_enregistre(contexte, compte)?));
    }
    if json {
        let valeur = json!({
            "dossier_jeu": jeu.display().to_string(),
            "comptes": lignes.iter().map(|(compte, present)| json!({
                "nom": compte.nom,
                "mot_de_passe_enregistre": present,
                "royaumes": compte.royaumes.iter().map(|r| json!({"nom": r.nom, "personnages": r.personnages})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        let texte = serde_json::to_string_pretty(&valeur)
            .map_err(|cause| Erreur::analyse("la sortie JSON des comptes", cause))?;
        return ligne(contexte.sortie, &texte);
    }
    if lignes.is_empty() {
        return ligne(
            contexte.sortie,
            &format!("Aucun compte trouvé dans le dossier des comptes du jeu (WTF/Account) : {}", jeu.display()),
        );
    }
    ligne(contexte.sortie, &format!("Comptes du jeu ({}) :", jeu.display()))?;
    for (compte, present) in lignes {
        let etat = if present { "oui" } else { "non" };
        ligne(contexte.sortie, &format!("  {} : mot de passe enregistré : {etat}", compte.nom))?;
    }
    Ok(())
}

fn enregistrer(compte: &str, nouveau: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    normaliser_nom_compte(compte)?;
    let nom = if nouveau { compte.to_string() } else { compte_du_jeu(compte, contexte, journal)? };
    let secret = lire_mot_de_passe(&nom, contexte)?;
    contexte.coffre.enregistrer(&nom, &secret)?;
    journal.info(ETAPE, &format!("mot de passe enregistré dans le trousseau pour le compte « {nom} »"))?;
    ligne(contexte.sortie, &format!("Mot de passe enregistré dans le trousseau pour le compte « {nom} »."))
}

/// Nom du compte tel que le jeu le connaît ; refuse un compte sans dossier dans `WTF/Account`.
fn compte_du_jeu(compte: &str, contexte: &Contexte<'_>, journal: &Journal) -> Result<String> {
    let config = Config::charger(&contexte.dossiers)?;
    let jeu = dossier_jeu(&config)?;
    let comptes = lister_comptes_jeu(&jeu, journal)?;
    trouver_compte(compte, &comptes).map(|trouve| trouve.nom.clone()).ok_or_else(|| {
        Erreur::configuration(format!(
            "le compte « {compte} » n'existe pas dans le dossier des comptes du jeu (WTF/Account) : le jeu ne crée ce dossier qu'à la première connexion, et « ebonhux jouer » ne retrouverait pas ce compte ; vérifiez l'orthographe, ou ajoutez --nouveau pour enregistrer quand même le mot de passe d'un compte pas encore connu du jeu"
        ))
    })
}

fn lire_mot_de_passe(compte: &str, contexte: &mut Contexte<'_>) -> Result<Secret> {
    let mut texte = if contexte.entree.interactif {
        saisie_masquee(compte, contexte)?
    } else {
        let mut texte = String::new();
        let lus = contexte
            .entree
            .lecteur
            .read_line(&mut texte)
            .map_err(|cause| Erreur::io("lire le mot de passe", NOM_ENTREE, cause))?;
        if lus == 0 {
            return Err(Erreur::analyse(
                "le mot de passe saisi",
                "l'entrée standard est vide ; envoyez le mot de passe sur une ligne, par exemple avec « ebonhux comptes enregistrer <compte> < fichier » ou un tube",
            ));
        }
        texte
    };
    if texte.ends_with('\n') {
        texte.pop();
    }
    if texte.ends_with('\r') {
        texte.pop();
    }
    let secret = Secret::new(texte);
    if secret.est_vide() {
        return Err(Erreur::analyse("le mot de passe saisi", "il est vide ; un mot de passe vide n'est pas enregistré"));
    }
    Ok(secret)
}

fn saisie_masquee(compte: &str, contexte: &mut Contexte<'_>) -> Result<String> {
    let erreur_flux = |cause| Erreur::io("écrire sur la sortie d'erreur", "<flux d'erreur>", cause);
    write!(contexte.erreur, "Mot de passe du compte « {compte} » (la saisie reste invisible) : ").map_err(erreur_flux)?;
    contexte.erreur.flush().map_err(erreur_flux)?;
    let resultat = Command::new(PROGRAMME_SAISIE)
        .arg("-c")
        .arg(SCRIPT_SAISIE)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output();
    // La ligne saisie n'a pas été répétée à l'écran : on passe à la ligne suivante dans tous les cas.
    writeln!(contexte.erreur).map_err(erreur_flux)?;
    let sortie = resultat.map_err(|cause| Erreur::io("lancer la saisie masquée (sh)", PROGRAMME_SAISIE, cause))?;
    match sortie.status.code() {
        Some(0) => String::from_utf8(sortie.stdout)
            .map_err(|_| Erreur::analyse("le mot de passe saisi", "il n'est pas un texte UTF-8 valide")),
        Some(CODE_INTERRUPTION) => Err(Erreur::analyse("le mot de passe saisi", "la saisie a été interrompue")),
        code => Err(Erreur::Commande {
            programme: "stty".to_string(),
            code,
            sortie: "la saisie masquée est impossible (l'écho du terminal n'a pas pu être coupé) ; envoyez le mot de passe par l'entrée standard redirigée".to_string(),
        }),
    }
}

fn oublier(compte: &str, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    normaliser_nom_compte(compte)?;
    let present = contexte.coffre.lire(compte)?.is_some();
    if !present {
        return ligne(
            contexte.sortie,
            &format!("Aucun mot de passe n'était enregistré pour le compte « {compte} » : rien à retirer."),
        );
    }
    contexte.coffre.supprimer(compte)?;
    journal.info(ETAPE, &format!("mot de passe retiré du trousseau pour le compte « {compte} »"))?;
    ligne(contexte.sortie, &format!("Mot de passe retiré du trousseau pour le compte « {compte} »."))
}
