//! Préfixe Wine : environnement de lancement (pur), création, pose des DLL DXVK, arrêt.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

use crate::dxvk::{DLL_REQUISES, LIMITE_ENTREE_OCTETS, SOUS_DOSSIERS_ARCHITECTURE, SourceDxvk};
use crate::erreur::{Erreur, Result};
use crate::executeur::{Commande, Executeur};
use crate::journal::{Journal, Niveau};

pub const VARIABLE_PREFIXE: &str = "WINEPREFIX";
pub const VARIABLE_SURCHARGES: &str = "WINEDLLOVERRIDES";
/// Surcharges de la recette de l'essai réel du 2026-10-23 (hors DLL graphiques, ajoutées en `n`).
pub const SURCHARGES_DESACTIVEES: [&str; 3] = ["winemenubuilder.exe", "mscoree", "mshtml"];
pub const MODE_DESACTIVE: &str = "d";
/// DLL graphiques en natif : le jeu utilise DXVK. Avec AwesomeWotlk v37 lancé depuis le dossier de lancement,
/// la fermeture ne plante plus (essai réel du 2026-10-24) ; seule la v35 plantait (ERROR #132).
pub const MODE_NATIF: &str = "n";
pub const SEPARATEUR_SURCHARGES: &str = ";";

pub const PROGRAMME_WINEBOOT: &str = "wineboot";
pub const ARGUMENT_WINEBOOT: &str = "--init";
pub const PROGRAMME_WINESERVER: &str = "wineserver";
pub const ARGUMENT_WINESERVER_ARRET: &str = "-k";

/// Fichier créé par `wineboot` à la fin de l'initialisation : sa présence marque un préfixe prêt.
pub const REPERE_PREFIXE_PRET: &str = "system.reg";
pub const SOUS_CHEMIN_SYSTEM32: [&str; 3] = ["drive_c", "windows", "system32"];
pub const SOUS_CHEMIN_SYSWOW64: [&str; 3] = ["drive_c", "windows", "syswow64"];
pub const EXTENSION_DLL: &str = "dll";
pub const PREFIXE_TEMPORAIRE: &str = ".ebonhux-";
pub const SUFFIXE_TEMPORAIRE: &str = ".tmp";
pub const LONGUEUR_MAX_SORTIE_ERREUR: usize = 2000;

const ETAPE: &str = "prefixe";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatPrefixe {
    Cree,
    DejaPret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatDll {
    Posee,
    Remplacee,
    DejaIdentique,
}

/// Environnement de lancement de Wine : fonction pure.
pub fn environnement_lancement(prefixe: &Path) -> BTreeMap<String, String> {
    let desactivees = SURCHARGES_DESACTIVEES.iter().map(|nom| format!("{nom}={MODE_DESACTIVE}"));
    let natives = DLL_REQUISES.iter().map(|nom| format!("{nom}={MODE_NATIF}"));
    let surcharges = desactivees.chain(natives).collect::<Vec<_>>().join(SEPARATEUR_SURCHARGES);
    BTreeMap::from([
        (VARIABLE_PREFIXE.to_string(), prefixe.to_string_lossy().into_owned()),
        (VARIABLE_SURCHARGES.to_string(), surcharges),
    ])
}

fn refuser(chemin: &Path, raison: &str) -> Erreur {
    Erreur::Chemin { chemin: chemin.to_path_buf(), raison: raison.to_string() }
}

fn valider_prefixe(prefixe: &Path) -> Result<()> {
    if prefixe.as_os_str().is_empty() {
        return Err(refuser(prefixe, "le chemin du préfixe est vide"));
    }
    if prefixe.components().any(|c| c == Component::ParentDir) {
        return Err(refuser(prefixe, "le chemin du préfixe contient « .. »"));
    }
    Ok(())
}

fn tronquer(texte: &str) -> String {
    let texte = texte.trim();
    match texte.char_indices().nth(LONGUEUR_MAX_SORTIE_ERREUR) {
        Some((fin, _)) => format!("{} [sortie tronquée]", &texte[..fin]),
        None => texte.to_string(),
    }
}

fn lancer_wine(executeur: &dyn Executeur, commande: &Commande, logique: &str, journal: &Journal) -> Result<()> {
    let resultat = executeur.lancer(commande).and_then(|sortie| {
        if sortie.reussie() {
            return Ok(());
        }
        let detail = if sortie.stderr.trim().is_empty() { &sortie.stdout } else { &sortie.stderr };
        Err(Erreur::Commande {
            programme: logique.to_string(),
            code: Some(sortie.code),
            sortie: tronquer(detail),
        })
    });
    if let Err(erreur) = &resultat {
        journal.journaliser_erreur(ETAPE, erreur)?;
    }
    resultat
}

/// Crée et initialise le préfixe par `wineboot --init` ; ne fait rien si le repère existe déjà.
pub fn creer(prefixe: &Path, executeur: &dyn Executeur, journal: &Journal) -> Result<EtatPrefixe> {
    valider_prefixe(prefixe)?;
    let repere = prefixe.join(REPERE_PREFIXE_PRET);
    match fs::symlink_metadata(&repere) {
        Ok(meta) if meta.is_file() => {
            journal.info(ETAPE, &format!("Préfixe déjà initialisé : {}", prefixe.display()))?;
            return Ok(EtatPrefixe::DejaPret);
        }
        Ok(_) => return Err(refuser(&repere, "le repère du préfixe n'est pas un fichier ordinaire")),
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(Erreur::io("lire le repère du préfixe", &repere, e)),
    }
    journal.info(ETAPE, &format!("Initialisation du préfixe : {}", prefixe.display()))?;
    let mut commande = Commande::new(PROGRAMME_WINEBOOT).arg(ARGUMENT_WINEBOOT);
    commande.environnement = environnement_lancement(prefixe);
    let logique = format!("{PROGRAMME_WINEBOOT} {ARGUMENT_WINEBOOT}");
    lancer_wine(executeur, &commande, &logique, journal)?;
    journal.info(ETAPE, &format!("Préfixe initialisé : {}", prefixe.display()))?;
    Ok(EtatPrefixe::Cree)
}

/// Résultat de l'arrêt du serveur Wine d'un préfixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatArret {
    Arrete,
    AucunServeur,
}

/// Code de sortie de `wineserver -k` quand aucun serveur ne tourne (observé : code 1, sorties vides).
const CODE_AUCUN_SERVEUR: i32 = 1;

/// Arrête le serveur Wine du préfixe (`wineserver -k`). Sans serveur actif, rien à faire : succès.
pub fn arreter(prefixe: &Path, executeur: &dyn Executeur, journal: &Journal) -> Result<EtatArret> {
    valider_prefixe(prefixe)?;
    let commande = Commande::new(PROGRAMME_WINESERVER)
        .arg(ARGUMENT_WINESERVER_ARRET)
        .env(VARIABLE_PREFIXE, &prefixe.to_string_lossy());
    let logique = format!("{PROGRAMME_WINESERVER} {ARGUMENT_WINESERVER_ARRET}");
    let sortie = match executeur.lancer(&commande) {
        Ok(sortie) => sortie,
        Err(erreur) => {
            journal.journaliser_erreur(ETAPE, &erreur)?;
            return Err(erreur);
        }
    };
    if sortie.reussie() {
        journal.info(ETAPE, &format!("Serveur Wine arrêté pour le préfixe : {}", prefixe.display()))?;
        return Ok(EtatArret::Arrete);
    }
    if sortie.code == CODE_AUCUN_SERVEUR && sortie.stderr.trim().is_empty() {
        journal.info(ETAPE, &format!("Aucun serveur Wine actif pour le préfixe : {}", prefixe.display()))?;
        return Ok(EtatArret::AucunServeur);
    }
    let detail = if sortie.stderr.trim().is_empty() { &sortie.stdout } else { &sortie.stderr };
    let erreur = Erreur::Commande { programme: logique, code: Some(sortie.code), sortie: tronquer(detail) };
    journal.journaliser_erreur(ETAPE, &erreur)?;
    Err(erreur)
}

/// Dossier de destination sous le préfixe : chaque composant doit exister et être un vrai dossier.
fn dossier_sans_lien(prefixe: &Path, sous_chemin: &[&str]) -> Result<PathBuf> {
    let mut courant = prefixe.to_path_buf();
    for composant in sous_chemin {
        courant.push(composant);
        let meta = fs::symlink_metadata(&courant).map_err(|e| Erreur::io("lire le dossier du préfixe", &courant, e))?;
        if meta.file_type().is_symlink() {
            return Err(refuser(&courant, "lien symbolique refusé dans le préfixe"));
        }
        if !meta.is_dir() {
            return Err(refuser(&courant, "ce n'est pas un dossier"));
        }
    }
    Ok(courant)
}

fn lire_source(chemin: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(chemin).map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            Erreur::Chemin { chemin: chemin.to_path_buf(), raison: "DLL DXVK absente de la source".to_string() }
        } else {
            Erreur::io("lire la DLL DXVK source", chemin, e)
        }
    })?;
    if !meta.is_file() {
        return Err(refuser(chemin, "la DLL DXVK source n'est pas un fichier ordinaire (lien refusé)"));
    }
    if meta.len() > LIMITE_ENTREE_OCTETS {
        return Err(refuser(chemin, "la DLL DXVK source dépasse la taille permise"));
    }
    fs::read(chemin).map_err(|e| Erreur::io("lire la DLL DXVK source", chemin, e))
}

fn ecrire_atomique(destination: &Path, contenu: &[u8]) -> Result<()> {
    let nom = destination.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporaire = destination.with_file_name(format!("{PREFIXE_TEMPORAIRE}{nom}{SUFFIXE_TEMPORAIRE}"));
    let ecrire = || -> std::io::Result<()> {
        let mut fichier = fs::OpenOptions::new().write(true).create_new(true).open(&temporaire)?;
        fichier.write_all(contenu)?;
        fichier.sync_all()?;
        fs::rename(&temporaire, destination)
    };
    match ecrire() {
        Ok(()) => Ok(()),
        Err(e) => match fs::remove_file(&temporaire) {
            Ok(()) => Err(Erreur::io("écrire la DLL DXVK", destination, e)),
            Err(n) if n.kind() == ErrorKind::NotFound => Err(Erreur::io("écrire la DLL DXVK", destination, e)),
            Err(n) => Err(Erreur::io(
                &format!("écrire la DLL DXVK (erreur : {e} ; nettoyage du temporaire impossible)"),
                destination,
                n,
            )),
        },
    }
}

/// Pose les DLL DXVK dans le préfixe (x32 vers syswow64, x64 vers system32).
/// Tout est validé avant la première écriture.
pub fn poser_dxvk(prefixe: &Path, source: &SourceDxvk, journal: &Journal) -> Result<Vec<(PathBuf, ResultatDll)>> {
    valider_prefixe(prefixe)?;
    let cibles = [
        (SOUS_DOSSIERS_ARCHITECTURE[0], SOUS_CHEMIN_SYSWOW64),
        (SOUS_DOSSIERS_ARCHITECTURE[1], SOUS_CHEMIN_SYSTEM32),
    ];
    let mut plan = Vec::new();
    for (architecture, sous_chemin) in cibles {
        let dossier = dossier_sans_lien(prefixe, &sous_chemin)?;
        for dll in DLL_REQUISES {
            let fichier = format!("{dll}.{EXTENSION_DLL}");
            let contenu = lire_source(&source.dossier.join(architecture).join(&fichier))?;
            plan.push((dossier.join(&fichier), contenu));
        }
    }
    let mut bilan = Vec::new();
    for (destination, contenu) in plan {
        let resultat = match fs::symlink_metadata(&destination) {
            Ok(meta) if meta.is_file() => {
                let actuel = fs::read(&destination).map_err(|e| Erreur::io("lire la DLL du préfixe", &destination, e))?;
                if actuel == contenu {
                    ResultatDll::DejaIdentique
                } else {
                    ResultatDll::Remplacee
                }
            }
            Ok(_) => return Err(refuser(&destination, "la destination n'est pas un fichier ordinaire (lien refusé)")),
            Err(e) if e.kind() == ErrorKind::NotFound => ResultatDll::Posee,
            Err(e) => return Err(Erreur::io("lire la DLL du préfixe", &destination, e)),
        };
        if resultat != ResultatDll::DejaIdentique {
            ecrire_atomique(&destination, &contenu)?;
        }
        let (niveau, message) = match resultat {
            ResultatDll::Posee => (Niveau::Info, "DLL DXVK posée"),
            ResultatDll::Remplacee => (Niveau::Avertissement, "DLL DXVK différente remplacée"),
            ResultatDll::DejaIdentique => (Niveau::Info, "DLL DXVK déjà identique, rien à faire"),
        };
        let contexte = BTreeMap::from([("dll".to_string(), destination.display().to_string())]);
        journal.evenement(niveau, ETAPE, message, &contexte, None)?;
        bilan.push((destination, resultat));
    }
    Ok(bilan)
}
