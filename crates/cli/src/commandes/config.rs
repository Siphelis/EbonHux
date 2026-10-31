use std::path::PathBuf;

use ebonhux_coeur::config::{Config, Royaume, valider_site};
use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::Journal;

use crate::contexte::Contexte;
use crate::interface::ActionConfig;
use crate::sortie::ligne;

pub const CLEF_DOSSIER_JEU: &str = "dossier_jeu";
pub const CLEF_DOSSIER_PREFIXE: &str = "dossier_prefixe";
pub const CLEF_REALMLIST: &str = "royaume.realmlist";
pub const CLEF_ROYAUME_NOM: &str = "royaume.nom";
pub const CLEF_SITE: &str = "site";
pub const CLEFS: [&str; 5] = [CLEF_DOSSIER_JEU, CLEF_DOSSIER_PREFIXE, CLEF_REALMLIST, CLEF_ROYAUME_NOM, CLEF_SITE];

const NON_DEFINI: &str = "(non défini)";
const ETAPE: &str = "cli.config";

pub fn lancer(action: &ActionConfig, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    match action {
        ActionConfig::Afficher => afficher(contexte),
        ActionConfig::Definir { clef, valeur } => definir(clef, valeur, contexte, journal),
    }
}

fn afficher(contexte: &mut Contexte<'_>) -> Result<()> {
    let config = Config::charger(&contexte.dossiers)?;
    let fichier = contexte.dossiers.fichier_config();
    ligne(contexte.sortie, &format!("Fichier de configuration : {}", fichier.display()))?;
    for clef in CLEFS {
        let valeur = valeur_de(&config, clef).unwrap_or_else(|| NON_DEFINI.to_string());
        ligne(contexte.sortie, &format!("{clef} = {valeur}"))?;
    }
    let comptes = if config.comptes.is_empty() { "(aucun)".to_string() } else { config.comptes.join(", ") };
    ligne(contexte.sortie, &format!("comptes = {comptes}"))
}

fn definir(clef: &str, valeur: &str, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let mut config = Config::charger(&contexte.dossiers)?;
    appliquer(&mut config, clef, valeur)?;
    config.enregistrer(&contexte.dossiers)?;
    journal.info(ETAPE, &format!("clef « {clef} » définie à « {valeur} »"))?;
    ligne(contexte.sortie, &format!("{clef} = {valeur}"))?;
    if let Some(royaume) = &config.royaume
        && (royaume.realmlist.is_empty() || royaume.nom.is_empty())
    {
        ligne(
            contexte.sortie,
            &format!("Le royaume est incomplet : définissez aussi {CLEF_REALMLIST} et {CLEF_ROYAUME_NOM}."),
        )?;
    }
    Ok(())
}

pub fn valeur_de(config: &Config, clef: &str) -> Option<String> {
    match clef {
        CLEF_DOSSIER_JEU => config.dossier_jeu.as_ref().map(|p| p.display().to_string()),
        CLEF_DOSSIER_PREFIXE => config.dossier_prefixe.as_ref().map(|p| p.display().to_string()),
        CLEF_REALMLIST => config.royaume.as_ref().map(|r| r.realmlist.clone()),
        CLEF_ROYAUME_NOM => config.royaume.as_ref().map(|r| r.nom.clone()),
        CLEF_SITE => config.site.clone(),
        _ => None,
    }
}

fn chemin_absolu(clef: &str, valeur: &str) -> Result<PathBuf> {
    let chemin = PathBuf::from(valeur);
    if valeur.trim().is_empty() || !chemin.is_absolute() {
        return Err(Erreur::configuration(format!(
            "la valeur de « {clef} » doit être un chemin absolu (reçu : « {valeur} »)"
        )));
    }
    Ok(chemin)
}

fn texte_valide(clef: &str, valeur: &str) -> Result<String> {
    let propre = valeur.trim();
    if propre.is_empty() || propre.chars().any(char::is_control) {
        return Err(Erreur::configuration(format!(
            "la valeur de « {clef} » doit être un texte non vide, sans caractère de contrôle"
        )));
    }
    Ok(propre.to_string())
}

/// Fonction pure : modifie la configuration en mémoire, sans rien écrire.
pub fn appliquer(config: &mut Config, clef: &str, valeur: &str) -> Result<()> {
    match clef {
        CLEF_DOSSIER_JEU => config.dossier_jeu = Some(chemin_absolu(clef, valeur)?),
        CLEF_DOSSIER_PREFIXE => config.dossier_prefixe = Some(chemin_absolu(clef, valeur)?),
        CLEF_REALMLIST => {
            let texte = texte_valide(clef, valeur)?;
            royaume(config).realmlist = texte;
        }
        CLEF_ROYAUME_NOM => {
            let texte = texte_valide(clef, valeur)?;
            royaume(config).nom = texte;
        }
        CLEF_SITE => {
            valider_site(valeur)?;
            config.site = Some(valeur.to_string());
        }
        _ => {
            return Err(Erreur::configuration(format!(
                "clef inconnue « {clef} » ; clefs valides : {}",
                CLEFS.join(", ")
            )));
        }
    }
    Ok(())
}

fn royaume(config: &mut Config) -> &mut Royaume {
    config.royaume.get_or_insert_with(|| Royaume { realmlist: String::new(), nom: String::new() })
}
