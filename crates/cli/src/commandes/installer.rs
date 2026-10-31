use ebonhux_coeur::erreur::Result;
use ebonhux_coeur::installation::{Mode, Statut, executer, planifier};
use ebonhux_coeur::journal::Journal;

use crate::commandes::diag::sonder_machine;
use crate::contexte::Contexte;
use crate::sortie::ligne;

const AVERTISSEMENT_ELEVATION: &str =
    "Le système va demander le mot de passe administrateur (une seule fois, par pkexec).";
const INDICE_SIMULATION: &str =
    "Simulation : rien n'a été lancé. Ajoutez --executer pour installer réellement.";

pub fn lancer(reel: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let systeme = sonder_machine(contexte, journal)?;
    let plan = planifier(&systeme)?;
    ligne(contexte.sortie, &format!("Plan d'installation :\n{}", plan.resume()))?;
    for note in &plan.notes {
        ligne(contexte.sortie, &format!("Note : {note}"))?;
    }
    let mode = if reel { Mode::Reel } else { Mode::Simulation };
    if reel && plan.etapes.iter().any(|etape| etape.elevation) {
        ligne(contexte.sortie, AVERTISSEMENT_ELEVATION)?;
    }
    let rapport = executer(plan, mode, contexte.executeur, journal)?;
    for resultat in &rapport.resultats {
        let statut = match resultat.statut {
            Statut::Simulee => "simulée",
            Statut::Reussie => "réussie",
        };
        ligne(contexte.sortie, &format!("  [{statut}] {}", resultat.description))?;
    }
    ligne(contexte.sortie, &rapport.message)?;
    if !reel && !rapport.resultats.is_empty() {
        ligne(contexte.sortie, INDICE_SIMULATION)?;
    }
    Ok(())
}
