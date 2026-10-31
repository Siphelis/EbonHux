use ebonhux_coeur::awesomewotlk::{VERSION_EPINGLEE, chemin_dll, dll_prete};
use ebonhux_coeur::config::Config;
use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::lancement::{
    ConfigWtf, LectureWtf, ParametresLancement, PROGRAMME_WINE_DEFAUT, PlanLancement, ResultatCache, ResultatLancement,
    plan_a_blanc, preparer_et_lancer,
};

use crate::commandes::jeu::{dossier_jeu, royaume};
use crate::contexte::Contexte;
use crate::sortie::ligne;

pub fn lancer(compte: &str, a_blanc: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let config = Config::charger(&contexte.dossiers)?;
    let jeu = dossier_jeu(&config)?;
    let royaume = royaume(&config)?;
    let prefixe = config.dossier_prefixe_effectif(&contexte.dossiers);
    let parametres = ParametresLancement {
        dossier_jeu: &jeu,
        compte,
        prefixe: &prefixe,
        programme_wine: PROGRAMME_WINE_DEFAUT,
        royaume,
        vider_cache: true,
        compte_deja_valide: false,
        donnees: &contexte.dossiers.donnees,
        awesomewotlk: *contexte.awesomewotlk,
    };
    if !a_blanc {
        verifier_connexion_automatique(&parametres)?;
    }
    let coffre = contexte.coffre;
    let resultat = if a_blanc {
        plan_a_blanc(&parametres, coffre, journal).map(Issue::Plan)
    } else {
        preparer_et_lancer(&parametres, coffre, contexte.lanceur, journal).map(Issue::Lance)
    };
    match resultat {
        Ok(Issue::Plan(plan)) => afficher_plan(&plan, contexte),
        Ok(Issue::Lance(lance)) => afficher_lancement(&lance, contexte),
        Err(erreur) => Err(preciser_secret_absent(erreur, compte, contexte)),
    }
}

const COMMANDE_PREPARATION: &str = "ebonhux prefixe awesomewotlk";

/// Le cœur refuse déjà sans la v37, mais son message renvoie à l'interface : ici on nomme la commande.
fn verifier_connexion_automatique(parametres: &ParametresLancement<'_>) -> Result<()> {
    let chemin = chemin_dll(parametres.donnees, VERSION_EPINGLEE, &parametres.awesomewotlk);
    if dll_prete(&chemin, &parametres.awesomewotlk)? {
        return Ok(());
    }
    Err(Erreur::configuration(format!(
        "La connexion automatique (AwesomeWotlk v37) n'est pas installée : lancez « {COMMANDE_PREPARATION} » avant de jouer."
    )))
}

enum Issue {
    Plan(PlanLancement),
    Lance(ResultatLancement),
}

/// Un échec du coffre est précisé quand le secret manque vraiment : l'utilisateur apprend la
/// commande à lancer. Toute autre cause (coffre inaccessible, par exemple) reste telle quelle.
fn preciser_secret_absent(erreur: Erreur, compte: &str, contexte: &Contexte<'_>) -> Erreur {
    if !matches!(erreur, Erreur::Coffre { .. }) {
        return erreur;
    }
    match contexte.coffre.lire(compte) {
        Ok(None) => absent(compte),
        Ok(Some(secret)) if secret.est_vide() => absent(compte),
        _ => erreur,
    }
}

fn absent(compte: &str) -> Erreur {
    Erreur::Coffre {
        raison: format!(
            "aucun mot de passe n'est enregistré pour le compte « {compte} » ; enregistrez-le avec « ebonhux comptes enregistrer {compte} »"
        ),
    }
}

fn afficher_lancement(lance: &ResultatLancement, contexte: &mut Contexte<'_>) -> Result<()> {
    ligne(contexte.sortie, &lance.resume())?;
    match lance.cache {
        ResultatCache::Absent => ligne(contexte.sortie, "Dossier Cache absent : rien à vider.")?,
        ResultatCache::Vide { retirees } => ligne(contexte.sortie, &format!("Cache vidé ({retirees} entrée(s) retirée(s))."))?,
        ResultatCache::Conserve => ligne(contexte.sortie, "Cache conservé.")?,
    }
    signaler_config_wtf(&lance.config_wtf, contexte)
}

fn signaler_config_wtf(lecture: &LectureWtf, contexte: &mut Contexte<'_>) -> Result<()> {
    match lecture {
        LectureWtf::Absent => ligne(contexte.sortie, "Config.wtf absent : le jeu utilisera ses réglages par défaut."),
        LectureWtf::Presente(ConfigWtf { lignes_malformees, .. }) if !lignes_malformees.is_empty() => ligne(
            contexte.sortie,
            &format!("Attention : {} ligne(s) de Config.wtf sont malformées (le fichier n'est pas modifié).", lignes_malformees.len()),
        ),
        LectureWtf::Presente(_) => Ok(()),
    }
}

fn afficher_plan(plan: &PlanLancement, contexte: &mut Contexte<'_>) -> Result<()> {
    ligne(contexte.sortie, &format!("Plan à blanc pour le compte « {} » : rien n'est vidé ni lancé.", plan.compte))?;
    ligne(contexte.sortie, &format!("  Dossier du jeu : {}", plan.dossier_jeu.display()))?;
    ligne(contexte.sortie, &format!("  Préfixe Wine : {}", plan.prefixe.display()))?;
    ligne(contexte.sortie, &format!("  Programme Wine : {}", plan.programme_wine))?;
    let secret = if plan.secret_present { "oui" } else { "non" };
    ligne(contexte.sortie, &format!("  Mot de passe enregistré : {secret}"))?;
    let cache = match plan.entrees_cache {
        Some(nombre) => format!("{nombre} entrée(s) seraient retirées"),
        None => "dossier absent, rien à vider".to_string(),
    };
    ligne(contexte.sortie, &format!("  Cache : {cache}"))?;
    ligne(contexte.sortie, &format!("  Dossier de lancement : {}", plan.dossier_lancement.display()))?;
    let prete = if plan.awesomewotlk_prete { "oui" } else { "non : lancez « ebonhux prefixe awesomewotlk »" };
    ligne(contexte.sortie, &format!("  Connexion automatique prête : {prete}"))?;
    signaler_config_wtf(&plan.config_wtf, contexte)
}
