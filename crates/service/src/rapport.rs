//! Commande `exporter_rapport` (docs/interface.md, décision 12) : le journal en texte, noms de
//! comptes remplacés par « Compte 1, 2, 3 », écrit dans `<données>/ebonhux/rapports/` (droits 0600).

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use ebonhux_coeur::erreur::Erreur;
use ebonhux_coeur::journal::{FormatRapport, masquer};
use serde::Serialize;
use time::OffsetDateTime;

use crate::comptes::Service;
use crate::erreur::{CODE_DONNEES_INVALIDES, ErreurService, Resultat};

pub const NOM_DOSSIER_RAPPORTS: &str = "rapports";
const PREFIXE_FICHIER: &str = "rapport-";
const EXTENSION_FICHIER: &str = "txt";
const MODE_FICHIER: u32 = 0o600;
const MODE_DOSSIER: u32 = 0o700;
const ESSAIS_NOM_MAX: u32 = 100;
const ETAPE_RAPPORT: &str = "rapport";
const LIBELLE_COMPTE: &str = "Compte";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseRapport {
    pub chemin: String,
}

fn egal_sans_casse(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Remplace chaque nom de compte (sans tenir compte de la casse) par « Compte N », N étant le rang
/// du nom dans `noms` à partir de 1. En un seul passage : un texte déjà remplacé n'est jamais
/// relu, et à une position donnée le nom le plus long l'emporte. Un nom vide est ignoré.
pub fn remplacer_noms(texte: &str, noms: &[String]) -> String {
    let mut candidats: Vec<(Vec<char>, usize)> =
        noms.iter().enumerate().filter(|(_, nom)| !nom.is_empty()).map(|(rang, nom)| (nom.chars().collect(), rang + 1)).collect();
    candidats.sort_by_key(|(nom, rang)| (std::cmp::Reverse(nom.len()), *rang));
    let caracteres: Vec<char> = texte.chars().collect();
    let mut sortie = String::with_capacity(texte.len());
    let mut position = 0;
    while position < caracteres.len() {
        let trouve = candidats.iter().find(|(nom, _)| {
            caracteres.len() - position >= nom.len() && nom.iter().zip(&caracteres[position..]).all(|(a, b)| egal_sans_casse(*a, *b))
        });
        match trouve {
            Some((nom, rang)) => {
                sortie.push_str(&format!("{LIBELLE_COMPTE} {rang}"));
                position += nom.len();
            }
            None => {
                sortie.push(caracteres[position]);
                position += 1;
            }
        }
    }
    sortie
}

/// `AAAAMMJJ-HHMMSS` en UTC (calcul pur).
pub fn horodatage_nom(secondes: u64) -> Resultat<String> {
    let invalide = |raison: String| ErreurService::nouvelle(CODE_DONNEES_INVALIDES, format!("Heure du rapport invalide : {raison}."));
    let secondes = i64::try_from(secondes).map_err(|e| invalide(e.to_string()))?;
    let instant = OffsetDateTime::from_unix_timestamp(secondes).map_err(|e| invalide(e.to_string()))?;
    Ok(format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second()
    ))
}

impl Service {
    fn dossier_rapports(&self) -> Resultat<PathBuf> {
        let dossier = self.dossiers.donnees_application().join(NOM_DOSSIER_RAPPORTS);
        DirBuilder::new()
            .recursive(true)
            .mode(MODE_DOSSIER)
            .create(&dossier)
            .map_err(|e| Erreur::io("créer le dossier des rapports", &dossier, e))?;
        // `mode` ne joue qu'à la création : un dossier existant plus ouvert est resserré.
        fs::set_permissions(&dossier, fs::Permissions::from_mode(MODE_DOSSIER))
            .map_err(|e| Erreur::io("restreindre les droits du dossier des rapports", &dossier, e))?;
        Ok(dossier)
    }

    /// Ouvre un fichier neuf (jamais un rapport existant écrasé) : en cas de même seconde, un numéro est ajouté.
    fn fichier_neuf(&self, dossier: &std::path::Path, base: &str) -> Resultat<(fs::File, PathBuf)> {
        for essai in 0..ESSAIS_NOM_MAX {
            let nom = if essai == 0 {
                format!("{PREFIXE_FICHIER}{base}.{EXTENSION_FICHIER}")
            } else {
                format!("{PREFIXE_FICHIER}{base}-{}.{EXTENSION_FICHIER}", essai + 1)
            };
            let chemin = dossier.join(nom);
            match OpenOptions::new().write(true).create_new(true).mode(MODE_FICHIER).open(&chemin) {
                Ok(fichier) => return Ok((fichier, chemin)),
                Err(erreur) if erreur.kind() == ErrorKind::AlreadyExists => {}
                Err(erreur) => return Err(Erreur::io("créer le rapport", &chemin, erreur).into()),
            }
        }
        Err(ErreurService::nouvelle(
            CODE_DONNEES_INVALIDES,
            format!("Trop de rapports exportés à la même seconde dans « {} ».", dossier.display()),
        ))
    }

    /// Commande `exporter_rapport`.
    pub fn exporter_rapport(&self) -> Resultat<ReponseRapport> {
        let config = self.charger_config()?;
        let noms = self.noms_comptes(&config)?;
        let brut = self.journal.rapport(FormatRapport::Texte, None)?;
        let texte = masquer(&remplacer_noms(&brut, &noms), &self.dossiers.maison);
        let dossier = self.dossier_rapports()?;
        let base = horodatage_nom(self.horloge.maintenant_s())?;
        let (mut fichier, chemin) = self.fichier_neuf(&dossier, &base)?;
        fichier.write_all(texte.as_bytes()).map_err(|e| Erreur::io("écrire le rapport", &chemin, e))?;
        fichier.sync_all().map_err(|e| Erreur::io("synchroniser le rapport", &chemin, e))?;
        self.journal.info(ETAPE_RAPPORT, "Rapport exporté")?;
        Ok(ReponseRapport { chemin: chemin.display().to_string() })
    }
}
