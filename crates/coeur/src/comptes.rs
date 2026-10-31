//! Comptes du jeu : lecture de `<dossier_jeu>/WTF/Account/*`. Rien n'est écrit ni lancé.

use std::fs;
use std::path::{Path, PathBuf};

use crate::erreur::{Erreur, Result};
use crate::journal::Journal;

const ETAPE: &str = "comptes";
const DOSSIER_COMPTES: [&str; 2] = ["WTF", "Account"];
/// Dossier de variables d'addons, présent dans un compte : jamais un royaume.
const DOSSIER_VARIABLES: &str = "SavedVariables";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompteJeu {
    pub nom: String,
    pub royaumes: Vec<RoyaumeJeu>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoyaumeJeu {
    pub nom: String,
    pub personnages: Vec<String>,
}

/// Clé de comparaison de deux noms de compte : minuscules Unicode (`str::to_lowercase`),
/// sans normalisation NFC/NFD. Un « é » précomposé et un « e » suivi d'un accent combinant
/// restent donc distincts : la normaliser demanderait une table Unicode que le jeu, lui,
/// n'applique pas, et pourrait confondre deux dossiers réellement différents.
pub fn cle_nom_compte(nom: &str) -> String {
    nom.to_lowercase()
}

/// Premier compte dont le nom est égal à `nom` sans tenir compte de la casse.
pub fn trouver_compte<'a>(nom: &str, comptes: &'a [CompteJeu]) -> Option<&'a CompteJeu> {
    let cle = cle_nom_compte(nom);
    comptes.iter().find(|compte| cle_nom_compte(&compte.nom) == cle)
}

pub fn dossier_comptes(dossier_jeu: &Path) -> PathBuf {
    DOSSIER_COMPTES.iter().fold(dossier_jeu.to_path_buf(), |chemin, partie| chemin.join(partie))
}

/// Lit les comptes, leurs royaumes et leurs personnages. Les entrées refusées (nom à caractère
/// de contrôle ou non UTF-8, lien symbolique) sont ignorées et signalées au journal.
pub fn lister_comptes_jeu(dossier_jeu: &Path, journal: &Journal) -> Result<Vec<CompteJeu>> {
    let racine = dossier_comptes(dossier_jeu);
    if !racine.is_dir() {
        let erreur = Erreur::Chemin {
            chemin: racine,
            raison: "le dossier des comptes du jeu (WTF/Account) est absent ou n'est pas un dossier"
                .to_string(),
        };
        journal.journaliser_erreur(ETAPE, &erreur)?;
        return Err(erreur);
    }
    let mut comptes = Vec::new();
    for (nom, chemin) in sous_dossiers(&racine, journal)? {
        let mut royaumes = Vec::new();
        for (nom_royaume, chemin_royaume) in sous_dossiers(&chemin, journal)? {
            if nom_royaume.eq_ignore_ascii_case(DOSSIER_VARIABLES) {
                continue;
            }
            let personnages: Vec<String> = sous_dossiers(&chemin_royaume, journal)?
                .into_iter()
                .map(|(nom_personnage, _)| nom_personnage)
                .collect();
            if !personnages.is_empty() {
                royaumes.push(RoyaumeJeu { nom: nom_royaume, personnages });
            }
        }
        comptes.push(CompteJeu { nom, royaumes });
    }
    journal.info(ETAPE, &format!("{} compte(s) du jeu lu(s)", comptes.len()))?;
    Ok(comptes)
}

fn trier(entrees: &mut [(String, PathBuf)]) {
    entrees.sort_by(|a, b| cle_nom_compte(&a.0).cmp(&cle_nom_compte(&b.0)).then_with(|| a.0.cmp(&b.0)));
}

/// Sous-dossiers directs (nom, chemin), triés. Les fichiers sont ignorés (attendus) ; les liens
/// symboliques et les noms refusés sont ignorés avec un avertissement.
fn sous_dossiers(parent: &Path, journal: &Journal) -> Result<Vec<(String, PathBuf)>> {
    let lecture = fs::read_dir(parent).map_err(|e| Erreur::io("lire le dossier", parent, e))?;
    let mut trouves = Vec::new();
    for entree in lecture {
        let entree = entree.map_err(|e| Erreur::io("lire une entrée du dossier", parent, e))?;
        let chemin = entree.path();
        let type_fichier = entree
            .file_type()
            .map_err(|e| Erreur::io("lire le type de l'entrée", &chemin, e))?;
        if type_fichier.is_symlink() {
            journal.avertir(
                ETAPE,
                &format!("lien symbolique ignoré : {:?}", chemin.display().to_string()),
            )?;
            continue;
        }
        if !type_fichier.is_dir() {
            continue;
        }
        let Some(nom) = entree.file_name().to_str().map(str::to_string) else {
            journal.avertir(
                ETAPE,
                &format!("nom de dossier non UTF-8 refusé : {:?}", chemin.display().to_string()),
            )?;
            continue;
        };
        if nom.chars().any(char::is_control) {
            journal.avertir(ETAPE, &format!("nom à caractère de contrôle refusé : {nom:?}"))?;
            continue;
        }
        trouves.push((nom, chemin));
    }
    trier(&mut trouves);
    Ok(trouves)
}
