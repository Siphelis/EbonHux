use ebonhux_coeur::erreur::{Erreur, Result};
use ebonhux_coeur::journal::Journal;
use ebonhux_coeur::systeme::{
    EtatNtsync, FamillePaquets, FamillePilote, ModeWine, Session, Systeme, sonder,
};

use crate::contexte::Contexte;
use crate::sortie::ligne;

const INCONNU: &str = "inconnu";

/// Sonde la machine sous la racine du contexte ; partagé avec `installer`.
pub fn sonder_machine(contexte: &Contexte<'_>, journal: &Journal) -> Result<Systeme> {
    sonder(&contexte.racine_systeme, &contexte.environnement, contexte.executeur, journal)
}

pub fn lancer(json: bool, contexte: &mut Contexte<'_>, journal: &Journal) -> Result<()> {
    let systeme = sonder_machine(contexte, journal)?;
    if json {
        let texte = serde_json::to_string_pretty(&systeme)
            .map_err(|cause| Erreur::analyse("le diagnostic au format JSON", cause))?;
        return ligne(contexte.sortie, &texte);
    }
    for ligne_texte in decrire(&systeme) {
        ligne(contexte.sortie, &ligne_texte)?;
    }
    Ok(())
}

fn famille(famille: &FamillePaquets) -> &'static str {
    match famille {
        FamillePaquets::Eopkg => "eopkg",
        FamillePaquets::Apt => "apt",
        FamillePaquets::Dnf => "dnf",
        FamillePaquets::Pacman => "pacman",
        FamillePaquets::Inconnue => INCONNU,
    }
}

fn mode_wine(mode: &ModeWine) -> &'static str {
    match mode {
        ModeWine::Wow64 => "WoW64",
        ModeWine::Ancien => "ancien (32 bits séparé)",
        ModeWine::Inconnu => INCONNU,
    }
}

fn pilote(pilote: &FamillePilote) -> String {
    match pilote {
        FamillePilote::Nvidia { version: Some(version) } => format!("NVIDIA (version {version})"),
        FamillePilote::Nvidia { version: None } => "NVIDIA (version inconnue)".to_string(),
        FamillePilote::Amd => "AMD".to_string(),
        FamillePilote::Intel => "Intel".to_string(),
        FamillePilote::Logiciel => "rendu logiciel".to_string(),
        FamillePilote::Inconnue => INCONNU.to_string(),
    }
}

fn session(session: &Session) -> &'static str {
    match session {
        Session::Wayland => "Wayland",
        Session::X11 => "X11",
        Session::Inconnue => INCONNU,
    }
}

fn ntsync(etat: &EtatNtsync) -> &'static str {
    match etat {
        EtatNtsync::Absent => "absent",
        EtatNtsync::PresentInaccessible => "présent mais inaccessible",
        EtatNtsync::Utilisable => "utilisable",
    }
}

fn nombre(valeur: Option<u64>) -> String {
    valeur.map_or_else(|| INCONNU.to_string(), |n| n.to_string())
}

fn oui_non(vrai: bool) -> &'static str {
    if vrai { "oui" } else { "non" }
}

/// Fonction pure : les lignes du diagnostic, dans l'ordre d'affichage.
pub fn decrire(systeme: &Systeme) -> Vec<String> {
    let mut lignes = Vec::new();
    lignes.push(match &systeme.distribution {
        Some(d) => format!("Distribution : {} (famille de paquets : {})", d.nom, famille(&d.famille)),
        None => format!("Distribution : {INCONNU}"),
    });
    lignes.push(match &systeme.wine {
        Some(w) => format!(
            "Wine : {} (version {}, mode {})",
            w.chemin.display(),
            w.version.as_deref().unwrap_or(INCONNU),
            mode_wine(&w.mode)
        ),
        None => "Wine : absent".to_string(),
    });
    lignes.push(match &systeme.lutris {
        Some(l) => format!("Lutris : {} (version {})", l.chemin.display(), l.version.as_deref().unwrap_or(INCONNU)),
        None => "Lutris : absent".to_string(),
    });
    let vulkan = &systeme.vulkan;
    lignes.push(format!(
        "Vulkan : outil de diagnostic {}, {} périphérique(s)",
        if vulkan.outil_present { "présent" } else { "absent" },
        vulkan.peripheriques.len()
    ));
    lignes.push(format!("Vulkan 64 bits : {} manifeste(s)", vulkan.manifestes_64.len()));
    lignes.push(format!(
        "Vulkan 32 bits : {} manifeste(s), pilote 32 bits présent : {}",
        vulkan.manifestes_32.len(),
        oui_non(vulkan.pilote_32_present)
    ));
    lignes.push(format!("Pilote graphique : {}", pilote(&systeme.pilote)));
    lignes.push(format!("Session : {}", session(&systeme.session)));
    lignes.push(format!(
        "Limite de fichiers ouverts : souple {}, dure {}",
        nombre(systeme.limite_fichiers_souple),
        nombre(systeme.limite_fichiers_dure)
    ));
    lignes.push(format!("vm.max_map_count : {}", nombre(systeme.max_map_count)));
    lignes.push(format!("ntsync : {}", ntsync(&systeme.ntsync)));
    if systeme.sondes_en_echec.is_empty() {
        lignes.push("Sondes en échec : aucune".to_string());
    } else {
        lignes.push(format!("Sondes en échec : {}", systeme.sondes_en_echec.len()));
        for echec in &systeme.sondes_en_echec {
            lignes.push(format!("  - {} : {}", echec.sonde, echec.raison));
        }
    }
    lignes
}
