use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::{FormatRapport, Journal};

use crate::contexte::Contexte;
use crate::sortie::ligne;

const MESSAGE_VIDE: &str = "Le journal est vide : aucun événement enregistré.";

pub fn lancer(json: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    if json {
        let texte = journal.rapport(FormatRapport::Json, None)?;
        return ligne(contexte.sortie, &texte);
    }
    if est_vide(&journal.rapport(FormatRapport::Json, None)?)? {
        return ligne(contexte.sortie, MESSAGE_VIDE);
    }
    let texte = journal.rapport(FormatRapport::Texte, None)?;
    ligne(contexte.sortie, texte.trim_end())
}

fn est_vide(rapport_json: &str) -> Result<bool> {
    let valeur: serde_json::Value =
        serde_json::from_str(rapport_json).map_err(|cause| Erreur::analyse("le rapport du journal", cause))?;
    let evenements = valeur
        .get("evenements")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Erreur::analyse("le rapport du journal", "champ « evenements » absent"))?;
    let illisibles = valeur.get("lignes_illisibles").and_then(serde_json::Value::as_u64).unwrap_or(0);
    Ok(evenements.is_empty() && illisibles == 0)
}
