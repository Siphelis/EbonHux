//! Commandes `lancer` et `tout_lancer` (docs/interface.md, décisions 8 et 9).

use std::fmt;

use ebonhux_coeur::coffre::{CoffreMemoire, CoffreSecrets};
use ebonhux_coeur::comptes::cle_nom_compte;
use ebonhux_coeur::journal::Niveau;
use ebonhux_coeur::lancement::{ParametresLancement, lancer, preparer};
use ebonhux_coeur::secret::Secret;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::comptes::{ReponseCompte, Service, mot_de_passe_requis};
use crate::erreur::{
    CODE_AUCUN_COMPTE, CODE_COMPTE_INCONNU, CODE_DEJA_EN_COURS, CODE_DONNEES_INVALIDES, CODE_ENREGISTREMENT_ECHOUE, CODE_JEU_NON_CONFIGURE,
    CODE_MOT_DE_PASSE_REQUIS, ErreurService, Resultat,
};
use crate::etat;
use crate::sessions::{Demarrage, PAUSE_ENTRE_LANCEMENTS_S, Session};

const MESSAGE_LANCE: &str = "Jeu lancé.";

/// Un compte de « Tout lancer » avec son mot de passe saisi (ou `null`). Le mot de passe est masqué par `Debug`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentifiantsLancement {
    pub nom: String,
    #[serde(default)]
    pub mot_de_passe: Option<String>,
}

impl fmt::Debug for IdentifiantsLancement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentifiantsLancement")
            .field("nom", &self.nom)
            .field("mot_de_passe", &self.mot_de_passe.as_ref().map(|_| ebonhux_coeur::secret::MASQUE))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResultatLancementCompte {
    pub compte: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseToutLancer {
    pub resultats: Vec<ResultatLancementCompte>,
}

/// Un mot de passe vide équivaut à « non fourni ».
fn fourni(mot_de_passe: Option<&String>) -> Option<&str> {
    mot_de_passe.map(String::as_str).filter(|texte| !texte.is_empty())
}

fn horodatage_invalide(raison: impl fmt::Display) -> ErreurService {
    ErreurService::nouvelle(CODE_DONNEES_INVALIDES, format!("Horodatage de connexion invalide : {raison}."))
}

pub(crate) fn horodatage_iso(secondes: u64) -> Resultat<String> {
    let secondes = i64::try_from(secondes).map_err(horodatage_invalide)?;
    let instant = OffsetDateTime::from_unix_timestamp(secondes).map_err(horodatage_invalide)?;
    instant.format(&Rfc3339).map_err(horodatage_invalide)
}

impl Service {
    /// Commande `lancer`.
    pub fn lancer(&self, compte: &str, mot_de_passe: Option<String>) -> Resultat<ReponseCompte> {
        let _garde = self.garde();
        let config = self.charger_config()?;
        let Some(jeu) = config.dossier_jeu.clone() else {
            return Err(ErreurService::nouvelle(
                CODE_JEU_NON_CONFIGURE,
                "Le dossier du jeu n'est pas configuré : indiquez-le avant de lancer.",
            ));
        };
        let nom = self.exiger_compte(&config, compte)?;
        let cle = cle_nom_compte(&nom);
        let maintenant_s = self.horloge.maintenant_s();
        let actives = self.registre.actives(self.sonde.as_ref());
        if actives.contains_key(&cle) {
            return Err(ErreurService::nouvelle(
                CODE_DEJA_EN_COURS,
                format!("Le compte « {nom} » est déjà lancé ou en cours de lancement."),
            ));
        }
        let fichier = etat::lire(&self.dossiers)?.compte(&nom);
        let saisi = fourni(mot_de_passe.as_ref());
        // Mot de passe saisi : coffre mémoire pour ce seul lancement ; sinon le trousseau, si l'interrupteur le permet.
        let memoire = CoffreMemoire::new();
        let coffre: &dyn CoffreSecrets = match saisi {
            Some(texte) => {
                memoire.enregistrer(&nom, &Secret::new(texte.to_string()))?;
                &memoire
            }
            None => {
                let present = fichier.enregistrer_mot_de_passe
                    && self.coffre.lire(&nom)?.is_some_and(|secret| !secret.est_vide());
                if !present {
                    self.noter_attente(&nom)?;
                    return Err(mot_de_passe_requis(&nom));
                }
                self.coffre.as_ref()
            }
        };
        let prefixe = config.dossier_prefixe_effectif(&self.dossiers);
        let parametres = ParametresLancement {
            dossier_jeu: &jeu,
            compte: &nom,
            prefixe: &prefixe,
            programme_wine: &self.programme_wine,
            royaume: config.royaume.as_ref(),
            vider_cache: actives.is_empty(),
            // `exiger_compte` a validé le compte : liste du jeu ou configuration du launcher.
            compte_deja_valide: true,
            donnees: &self.dossiers.donnees,
            awesomewotlk: self.awesomewotlk_dll,
        };
        let preparation = preparer(&parametres, coffre, &self.journal)?;
        let resultat = lancer(preparation, self.lanceur.as_ref(), &self.journal)?;
        let demarrage = match self.sonde.demarrage(resultat.pid) {
            Demarrage::Heure(heure) => Some(heure),
            Demarrage::Absent | Demarrage::Inconnu => None,
        };
        self.registre.enregistrer(&cle, Session { pid: resultat.pid, debut_s: maintenant_s, demarrage });

        let mut etat = etat::lire(&self.dossiers)?;
        etat.compte_mut(&nom).derniere_connexion = Some(horodatage_iso(maintenant_s)?);
        etat::ecrire(&self.dossiers, &etat)?;
        if let (Some(texte), true) = (saisi, fichier.enregistrer_mot_de_passe) {
            let enregistre = self.coffre.enregistrer(&nom, &Secret::new(texte.to_string()));
            self.invalider_cache(&nom);
            enregistre.map_err(|erreur| {
                ErreurService::nouvelle(
                    CODE_ENREGISTREMENT_ECHOUE,
                    format!("Le jeu est lancé, mais le mot de passe n'a pas pu être enregistré : {erreur}"),
                )
            })?;
        }
        self.invalider_cache(&nom);
        self.reponse_compte(&nom)
    }

    /// Commande `tout_lancer` : aucun lancement tant qu'un mot de passe manque ; puis un lancement
    /// après l'autre, `PAUSE_ENTRE_LANCEMENTS_S` secondes entre deux. Un échec n'arrête pas les suivants.
    pub fn tout_lancer(&self, comptes: Vec<IdentifiantsLancement>) -> Resultat<ReponseToutLancer> {
        if comptes.is_empty() {
            return Err(ErreurService::nouvelle(CODE_AUCUN_COMPTE, "Aucun compte à lancer."));
        }
        self.verifier_mots_de_passe(&comptes)?;
        let mut resultats = Vec::with_capacity(comptes.len());
        for (rang, identifiants) in comptes.into_iter().enumerate() {
            if rang > 0 {
                self.horloge.attendre(PAUSE_ENTRE_LANCEMENTS_S);
            }
            let issue = self.lancer(&identifiants.nom, identifiants.mot_de_passe);
            resultats.push(self.resultat_de(&identifiants.nom, issue)?);
        }
        Ok(ReponseToutLancer { resultats })
    }

    fn resultat_de(&self, nom: &str, issue: Resultat<ReponseCompte>) -> Resultat<ResultatLancementCompte> {
        match issue {
            Ok(_) => Ok(ResultatLancementCompte { compte: nom.to_string(), ok: true, message: MESSAGE_LANCE.to_string() }),
            Err(erreur) => {
                self.noter(Niveau::Erreur, &erreur.message, nom, Some(erreur.code.clone()))?;
                Ok(ResultatLancementCompte { compte: nom.to_string(), ok: false, message: erreur.message })
            }
        }
    }

    /// Les comptes sans mot de passe (ni saisi ni enregistré) sont tous nommés dans UNE erreur. Un
    /// compte déjà lancé n'en a pas besoin : son refus sera un résultat, pas un blocage.
    fn verifier_mots_de_passe(&self, comptes: &[IdentifiantsLancement]) -> Resultat<()> {
        let config = self.charger_config()?;
        let etat = etat::lire(&self.dossiers)?;
        let actives = self.registre.actives(self.sonde.as_ref());
        let mut manquants = Vec::new();
        for identifiants in comptes {
            if fourni(identifiants.mot_de_passe.as_ref()).is_some() {
                continue;
            }
            let nom = match self.exiger_compte(&config, &identifiants.nom) {
                Ok(nom) => nom,
                // Inconnu : `lancer` le refusera avec son propre message, dans les résultats.
                Err(erreur) if erreur.code == CODE_COMPTE_INCONNU => continue,
                Err(erreur) => return Err(erreur),
            };
            if actives.contains_key(&cle_nom_compte(&nom)) {
                continue;
            }
            let present = etat.compte(&nom).enregistrer_mot_de_passe
                && self.coffre.lire(&nom)?.is_some_and(|secret| !secret.est_vide());
            if !present {
                self.noter_attente(&nom)?;
                manquants.push(format!("« {nom} »"));
            }
        }
        if manquants.is_empty() {
            return Ok(());
        }
        Err(ErreurService::nouvelle(
            CODE_MOT_DE_PASSE_REQUIS,
            format!("Mot de passe requis pour : {}. Aucun compte n'a été lancé.", manquants.join(", ")),
        ))
    }
}
