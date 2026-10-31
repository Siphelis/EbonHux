//! Aiguillage unique des commandes du contrat (docs/interface.md, section 3) : `Service::executer`.
//! Les arguments sont vérifiés champ par champ pour dire en français lequel est en cause ; jamais
//! une valeur n'est recopiée dans un message (un mot de passe pourrait s'y trouver).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::comptes::Service;
use crate::erreur::{CODE_ARGUMENTS_INVALIDES, CODE_COMMANDE_INCONNUE, CODE_DONNEES_INVALIDES, ErreurService, Resultat};
use crate::lancer::IdentifiantsLancement;

const LIBELLE_LISTE: &str = "comptes";

pub const COMMANDES: [&str; 23] = [
    "comptes",
    "ajouter_compte",
    "enregistrer_mot_de_passe",
    "definir_enregistrement",
    "definir_compte_par_defaut",
    "se_deconnecter",
    "lancer",
    "tout_lancer",
    "addons",
    "definir_suivi",
    "catalogue",
    "rafraichir_catalogue",
    "installer_addon",
    "desinstaller_addon",
    "langue",
    "reverifier_langue",
    "diagnostic",
    "exporter_rapport",
    "ouvrir_site",
    "installation",
    "definir_dossier_jeu",
    "installer_logiciels",
    "preparer_jeu",
];

fn invalide(commande: &str, raison: String) -> ErreurService {
    ErreurService::nouvelle(CODE_ARGUMENTS_INVALIDES, format!("Arguments invalides pour « {commande} » : {raison}."))
}

/// Objet d'arguments d'une commande, avec lecture typée de chaque champ.
struct Arguments<'a> {
    commande: &'a str,
    champs: &'a Map<String, Value>,
}

impl<'a> Arguments<'a> {
    /// Les clés autorisées sont les seules acceptées : une clé inconnue est une erreur, jamais ignorée.
    fn ouvrir(commande: &'a str, valeur: &'a Value, autorisees: &[&str]) -> Resultat<Self> {
        let Some(champs) = valeur.as_object() else {
            return Err(invalide(commande, "l'argument doit être un objet JSON".to_string()));
        };
        if let Some(inconnue) = champs.keys().find(|cle| !autorisees.contains(&cle.as_str())) {
            return Err(invalide(commande, format!("champ « {inconnue} » inconnu")));
        }
        Ok(Self { commande, champs })
    }

    fn requis(&self, cle: &str) -> Resultat<&'a Value> {
        match self.champs.get(cle) {
            Some(valeur) if !valeur.is_null() => Ok(valeur),
            _ => Err(invalide(self.commande, format!("champ « {cle} » manquant"))),
        }
    }

    fn texte(&self, cle: &str) -> Resultat<String> {
        self.requis(cle)?
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| invalide(self.commande, format!("le champ « {cle} » doit être un texte")))
    }

    fn booleen(&self, cle: &str) -> Resultat<bool> {
        self.requis(cle)?
            .as_bool()
            .ok_or_else(|| invalide(self.commande, format!("le champ « {cle} » doit être vrai ou faux")))
    }

    /// Booléen ou `null` (ou absent) : `None`.
    fn booleen_optionnel(&self, cle: &str) -> Resultat<Option<bool>> {
        match self.champs.get(cle) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(valeur)) => Ok(Some(*valeur)),
            Some(_) => Err(invalide(self.commande, format!("le champ « {cle} » doit être vrai ou faux"))),
        }
    }

    /// Texte ou `null` (ou absent) : `None`.
    fn texte_optionnel(&self, cle: &str) -> Resultat<Option<String>> {
        match self.champs.get(cle) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(texte)) => Ok(Some(texte.clone())),
            Some(_) => Err(invalide(self.commande, format!("le champ « {cle} » doit être un texte ou null"))),
        }
    }
}

fn identifiants(commande: &str, valeur: &Value) -> Resultat<Vec<IdentifiantsLancement>> {
    let args = Arguments::ouvrir(commande, valeur, &["comptes"])?;
    let Some(liste) = args.requis("comptes")?.as_array() else {
        return Err(invalide(commande, "le champ « comptes » doit être une liste".to_string()));
    };
    liste
        .iter()
        .enumerate()
        .map(|(rang, element)| {
            let lieu = format!("{commande} ({LIBELLE_LISTE}[{rang}])");
            let compte = Arguments::ouvrir(&lieu, element, &["nom", "mot_de_passe"])?;
            Ok(IdentifiantsLancement { nom: compte.texte("nom")?, mot_de_passe: compte.texte_optionnel("mot_de_passe")? })
        })
        .collect()
}

fn vers_json<T: Serialize>(reponse: &T) -> Resultat<Value> {
    serde_json::to_value(reponse)
        .map_err(|erreur| ErreurService::nouvelle(CODE_DONNEES_INVALIDES, format!("Réponse du service illisible : {erreur}")))
}

impl Service {
    /// Point d'entrée unique de la coque : route la commande du contrat et rend la réponse en JSON.
    pub fn executer(&self, commande: &str, args: Value) -> Resultat<Value> {
        // `null` (aucun argument envoyé) vaut `{}`.
        let args = if args.is_null() { Value::Object(Map::new()) } else { args };
        match commande {
            "comptes" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.comptes()?)
            }
            "ajouter_compte" => {
                let a = Arguments::ouvrir(commande, &args, &["nom", "mot_de_passe", "enregistrer"])?;
                vers_json(&self.ajouter_compte(&a.texte("nom")?, &a.texte_optionnel("mot_de_passe")?.unwrap_or_default(), a.booleen("enregistrer")?)?)
            }
            "enregistrer_mot_de_passe" => {
                let a = Arguments::ouvrir(commande, &args, &["compte", "mot_de_passe"])?;
                vers_json(&self.enregistrer_mot_de_passe(&a.texte("compte")?, &a.texte("mot_de_passe")?)?)
            }
            "definir_enregistrement" => {
                let a = Arguments::ouvrir(commande, &args, &["compte", "actif"])?;
                vers_json(&self.definir_enregistrement(&a.texte("compte")?, a.booleen("actif")?)?)
            }
            "definir_compte_par_defaut" => {
                let a = Arguments::ouvrir(commande, &args, &["compte"])?;
                vers_json(&self.definir_compte_par_defaut(&a.texte("compte")?)?)
            }
            "se_deconnecter" => {
                let a = Arguments::ouvrir(commande, &args, &["compte"])?;
                vers_json(&self.se_deconnecter(&a.texte("compte")?)?)
            }
            "lancer" => {
                let a = Arguments::ouvrir(commande, &args, &["compte", "mot_de_passe"])?;
                vers_json(&self.lancer(&a.texte("compte")?, a.texte_optionnel("mot_de_passe")?)?)
            }
            "tout_lancer" => vers_json(&self.tout_lancer(identifiants(commande, &args)?)?),
            "addons" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.addons()?)
            }
            "definir_suivi" => {
                let a = Arguments::ouvrir(commande, &args, &["dossier", "suivi"])?;
                vers_json(&self.definir_suivi(&a.texte("dossier")?, a.booleen("suivi")?)?)
            }
            "catalogue" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.catalogue()?)
            }
            "rafraichir_catalogue" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.rafraichir_catalogue()?)
            }
            "installer_addon" => {
                let a = Arguments::ouvrir(commande, &args, &["nom", "remplacer"])?;
                vers_json(&self.installer_addon_avec(&a.texte("nom")?, a.booleen_optionnel("remplacer")?.unwrap_or(false))?)
            }
            "desinstaller_addon" => {
                let a = Arguments::ouvrir(commande, &args, &["dossier"])?;
                vers_json(&self.desinstaller_addon(&a.texte("dossier")?)?)
            }
            "langue" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.langue()?)
            }
            "reverifier_langue" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.reverifier_langue()?)
            }
            "diagnostic" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.diagnostic()?)
            }
            "exporter_rapport" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.exporter_rapport()?)
            }
            "ouvrir_site" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.ouvrir_site()?)
            }
            "installation" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.installation()?)
            }
            "definir_dossier_jeu" => {
                let a = Arguments::ouvrir(commande, &args, &["chemin"])?;
                vers_json(&self.definir_dossier_jeu(&a.texte("chemin")?)?)
            }
            "installer_logiciels" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.installer_logiciels()?)
            }
            "preparer_jeu" => {
                Arguments::ouvrir(commande, &args, &[])?;
                vers_json(&self.preparer_jeu()?)
            }
            inconnue => Err(ErreurService::nouvelle(CODE_COMMANDE_INCONNUE, format!("Commande inconnue : « {inconnue} »."))),
        }
    }
}
