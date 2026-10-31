//! Comptes : liste fusionnée, états (docs/interface.md, décision 7), interrupteur d'enregistrement
//! du mot de passe, connexion et déconnexion. Les mots de passe ne vont que dans le coffre.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ebonhux_coeur::addons::CATALOGUE_EMBARQUE;
use ebonhux_coeur::awesomewotlk::{self, DllAttendue};
use ebonhux_coeur::coffre::{CoffreSecrets, normaliser_nom_compte};
use ebonhux_coeur::comptes::{cle_nom_compte, dossier_comptes, lister_comptes_jeu};
use ebonhux_coeur::config::{Config, Dossiers};
use ebonhux_coeur::erreur::Erreur;
use ebonhux_coeur::dxvk::ArchiveAttendue;
use ebonhux_coeur::executeur::Executeur;
use ebonhux_coeur::journal::{Journal, Niveau};
use ebonhux_coeur::lancement::{LanceurDetache, PROGRAMME_WINE_DEFAUT};
use ebonhux_coeur::reseau::Telechargeur;
use ebonhux_coeur::secret::Secret;
use serde::Serialize;

use crate::erreur::{
    CODE_COMPTE_EXISTANT, CODE_COMPTE_INCONNU, CODE_ENREGISTREMENT_DESACTIVE, CODE_MOT_DE_PASSE_REQUIS, CODE_NOM_INVALIDE,
    ErreurService, Resultat,
};
use crate::etat::{self, EtatCompteFichier, EtatInterface};
use crate::sessions::{Horloge, PhaseSession, Registre, SondeProcessus};
use crate::site::Ouvreur;

/// Durée pendant laquelle la présence d'un mot de passe lue dans le trousseau est gardée : la
/// commande `comptes` est interrogée chaque seconde et le trousseau est lent (docs/interface.md, décision 15).
pub const DUREE_CACHE_TROUSSEAU_S: u64 = 30;
const RACINE_SYSTEME_DEFAUT: &str = "/";

pub(crate) const ETAPE_SERVICE: &str = "service";
pub(crate) const CONTEXTE_COMPTE: &str = "compte";
pub(crate) const CONTEXTE_RESULTAT: &str = "resultat";
pub(crate) const VALEUR_ATTENTE: &str = "attente";
pub(crate) const MESSAGE_MOT_DE_PASSE_DEMANDE: &str = "Mot de passe demandé";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EtatAffiche {
    Deconnecte,
    Connecte,
    MotDePasseDemande,
    Lancement,
    EnJeu,
}

/// Compte tel que le contrat le décrit (docs/interface.md, section 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Compte {
    pub nom: String,
    pub initiale: String,
    pub etat: EtatAffiche,
    pub progression: Option<u8>,
    pub en_jeu_depuis_s: Option<u64>,
    pub enregistrer_mot_de_passe: bool,
    pub mot_de_passe_enregistre: bool,
    pub derniere_connexion: Option<String>,
    /// Compte pré-sélectionné pour « Tout lancer » : un seul `true` parmi les comptes.
    pub par_defaut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseCompte {
    pub compte: Compte,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseComptes {
    pub comptes: Vec<Compte>,
    pub en_jeu: usize,
    pub trousseau_disponible: bool,
    pub jeu_configure: bool,
}

fn initiale(nom: &str) -> String {
    nom.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// Calcul pur d'un compte : une session vivante l'emporte, sinon l'interrupteur et le mot de passe décident.
pub fn compte_depuis(nom: &str, fichier: &EtatCompteFichier, mot_de_passe_present: bool, phase: Option<PhaseSession>) -> Compte {
    let (etat, progression, en_jeu_depuis_s) = match phase {
        Some(PhaseSession::Lancement { progression }) => (EtatAffiche::Lancement, Some(progression), None),
        Some(PhaseSession::EnJeu { depuis_s }) => (EtatAffiche::EnJeu, None, Some(depuis_s)),
        None if !fichier.enregistrer_mot_de_passe => (EtatAffiche::MotDePasseDemande, None, None),
        None if mot_de_passe_present => (EtatAffiche::Connecte, None, None),
        None => (EtatAffiche::Deconnecte, None, None),
    };
    Compte {
        nom: nom.to_string(),
        initiale: initiale(nom),
        etat,
        progression,
        en_jeu_depuis_s,
        enregistrer_mot_de_passe: fichier.enregistrer_mot_de_passe,
        mot_de_passe_enregistre: mot_de_passe_present,
        derniere_connexion: fichier.derniere_connexion.clone(),
        par_defaut: false,
    }
}

pub struct Service {
    pub(crate) coffre: Arc<dyn CoffreSecrets + Send + Sync>,
    pub(crate) lanceur: Arc<dyn LanceurDetache + Send + Sync>,
    pub(crate) horloge: Arc<dyn Horloge>,
    pub(crate) sonde: Arc<dyn SondeProcessus>,
    pub(crate) dossiers: Dossiers,
    pub(crate) journal: Arc<Journal>,
    pub(crate) registre: Registre,
    pub(crate) executeur: Option<Arc<dyn Executeur + Send + Sync>>,
    pub(crate) telechargeur: Option<Arc<dyn Telechargeur + Send + Sync>>,
    pub(crate) ouvreur: Option<Arc<dyn Ouvreur>>,
    /// Archive AwesomeWotlk attendue : sert à la préparation du jeu.
    pub(crate) awesomewotlk_archive: ArchiveAttendue<'static>,
    /// DLL AwesomeWotlk attendue : le lancement s'arrête si elle n'est pas prête.
    pub(crate) awesomewotlk_dll: DllAttendue<'static>,
    pub(crate) racine_systeme: PathBuf,
    pub(crate) environnement: BTreeMap<String, String>,
    pub(crate) programme_wine: String,
    pub(crate) catalogue: String,
    /// Présence du mot de passe par compte (clé en minuscules) : (instant de lecture en secondes, présent).
    cache_trousseau: Mutex<BTreeMap<String, (u64, bool)>>,
    /// Sérialise les commandes qui modifient des données (fichier d'état, configuration, sessions).
    verrou: Mutex<()>,
    /// Une seule installation de logiciels ou préparation du jeu à la fois (commandes longues).
    verrou_installation: Mutex<()>,
    trousseau_signale_ok: AtomicBool,
}

impl Service {
    pub fn new(
        coffre: Arc<dyn CoffreSecrets + Send + Sync>,
        lanceur: Arc<dyn LanceurDetache + Send + Sync>,
        horloge: Arc<dyn Horloge>,
        sonde: Arc<dyn SondeProcessus>,
        dossiers: Dossiers,
        journal: Arc<Journal>,
    ) -> Self {
        Self {
            coffre,
            lanceur,
            horloge,
            sonde,
            dossiers,
            journal,
            registre: Registre::new(),
            executeur: None,
            telechargeur: None,
            ouvreur: None,
            awesomewotlk_archive: awesomewotlk::ARCHIVE_EPINGLEE,
            awesomewotlk_dll: awesomewotlk::DLL_EPINGLEE,
            racine_systeme: PathBuf::from(RACINE_SYSTEME_DEFAUT),
            environnement: BTreeMap::new(),
            programme_wine: PROGRAMME_WINE_DEFAUT.to_string(),
            catalogue: CATALOGUE_EMBARQUE.to_string(),
            cache_trousseau: Mutex::new(BTreeMap::new()),
            verrou: Mutex::new(()),
            verrou_installation: Mutex::new(()),
            trousseau_signale_ok: AtomicBool::new(true),
        }
    }

    /// Port des commandes du système (sonde du diagnostic).
    pub fn avec_executeur(mut self, executeur: Arc<dyn Executeur + Send + Sync>) -> Self {
        self.executeur = Some(executeur);
        self
    }

    /// Port de téléchargement (installation depuis le catalogue).
    pub fn avec_telechargeur(mut self, telechargeur: Arc<dyn Telechargeur + Send + Sync>) -> Self {
        self.telechargeur = Some(telechargeur);
        self
    }

    /// Archive et DLL AwesomeWotlk attendues (la v37 épinglée par défaut ; des valeurs factices dans les essais).
    pub fn avec_awesomewotlk(mut self, archive: ArchiveAttendue<'static>, dll: DllAttendue<'static>) -> Self {
        self.awesomewotlk_archive = archive;
        self.awesomewotlk_dll = dll;
        self
    }

    /// Port d'ouverture d'une adresse (globe du rail).
    pub fn avec_ouvreur(mut self, ouvreur: Arc<dyn Ouvreur>) -> Self {
        self.ouvreur = Some(ouvreur);
        self
    }

    /// Racine sous laquelle les sondes lisent `/etc`, `/proc`, `/usr`, et variables d'environnement qu'elles voient.
    pub fn avec_systeme(mut self, racine: impl Into<PathBuf>, environnement: BTreeMap<String, String>) -> Self {
        self.racine_systeme = racine.into();
        self.environnement = environnement;
        self
    }

    pub fn avec_programme_wine(mut self, programme: impl Into<String>) -> Self {
        self.programme_wine = programme.into();
        self
    }

    /// Catalogue d'addons (tableau JSON) à la place de celui qui est embarqué ; essais surtout.
    pub fn avec_catalogue(mut self, texte: impl Into<String>) -> Self {
        self.catalogue = texte.into();
        self
    }

    pub(crate) fn garde(&self) -> MutexGuard<'_, ()> {
        self.verrou.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Verrou non bloquant des commandes longues d'installation : `None` si l'une d'elles tourne déjà.
    pub(crate) fn garde_installation(&self) -> Option<MutexGuard<'_, ()>> {
        match self.verrou_installation.try_lock() {
            Ok(garde) => Some(garde),
            Err(std::sync::TryLockError::Poisoned(empoisonne)) => Some(empoisonne.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    pub(crate) fn noter(&self, niveau: Niveau, message: &str, compte: &str, code: Option<String>) -> Resultat<()> {
        let contexte = BTreeMap::from([(CONTEXTE_COMPTE.to_string(), compte.to_string())]);
        self.journal.evenement(niveau, ETAPE_SERVICE, message, &contexte, code)?;
        Ok(())
    }

    /// Le service a demandé le mot de passe de ce compte : le diagnostic l'affiche « en attente ».
    pub(crate) fn noter_attente(&self, compte: &str) -> Resultat<()> {
        let contexte = BTreeMap::from([
            (CONTEXTE_COMPTE.to_string(), compte.to_string()),
            (CONTEXTE_RESULTAT.to_string(), VALEUR_ATTENTE.to_string()),
        ]);
        self.journal.evenement(Niveau::Info, ETAPE_SERVICE, MESSAGE_MOT_DE_PASSE_DEMANDE, &contexte, None)?;
        Ok(())
    }

    /// Lit quelque chose dans le jeu avec un journal à part, pour ne pas écrire une ligne
    /// d'information à chaque interrogation ; avertissements et erreurs sont reportés dans le vrai journal.
    pub(crate) fn lecture_silencieuse<T>(&self, lire: impl FnOnce(&Journal) -> ebonhux_coeur::erreur::Result<T>) -> Resultat<T> {
        let lecture = Journal::en_memoire(self.dossiers.maison.clone());
        let resultat = lire(&lecture);
        for evenement in lecture.evenements_memoire() {
            if evenement.niveau != Niveau::Info {
                self.journal.evenement(
                    evenement.niveau,
                    &evenement.etape,
                    &evenement.message,
                    &evenement.contexte,
                    evenement.erreur,
                )?;
            }
        }
        Ok(resultat?)
    }

    pub(crate) fn charger_config(&self) -> Resultat<Config> {
        Ok(Config::charger(&self.dossiers)?)
    }

    /// Dossiers de `WTF/Account` (lecture silencieuse, voir `lecture_silencieuse`).
    fn comptes_du_jeu(&self, config: &Config) -> Resultat<Vec<String>> {
        let Some(jeu) = &config.dossier_jeu else {
            return Ok(Vec::new());
        };
        if !dossier_comptes(jeu).is_dir() {
            return Ok(Vec::new());
        }
        let comptes = self.lecture_silencieuse(|lecture| lister_comptes_jeu(jeu, lecture))?;
        Ok(comptes.into_iter().map(|c| c.nom).collect())
    }

    /// Dossiers du jeu ∪ comptes de la configuration, sans tenir compte de la casse (premier nom rencontré).
    pub(crate) fn noms_comptes(&self, config: &Config) -> Resultat<Vec<String>> {
        let mut vus = BTreeSet::new();
        let mut noms = Vec::new();
        for nom in self.comptes_du_jeu(config)?.into_iter().chain(config.comptes.iter().cloned()) {
            if vus.insert(cle_nom_compte(&nom)) {
                noms.push(nom);
            }
        }
        Ok(noms)
    }

    /// Nom affiché du compte demandé, ou `compte_inconnu`.
    pub(crate) fn exiger_compte(&self, config: &Config, nom: &str) -> Resultat<String> {
        let cle = cle_nom_compte(nom);
        self.noms_comptes(config)?
            .into_iter()
            .find(|candidat| cle_nom_compte(candidat) == cle)
            .ok_or_else(|| ErreurService::nouvelle(CODE_COMPTE_INCONNU, format!("Le compte « {nom} » est inconnu.")))
    }

    /// Le trousseau est-il utilisable ? Le changement d'état est journalisé une fois, pas à chaque interrogation.
    fn trousseau_disponible(&self) -> Resultat<bool> {
        match self.coffre.disponible() {
            Ok(()) => {
                if !self.trousseau_signale_ok.swap(true, Ordering::SeqCst) {
                    self.journal.info(ETAPE_SERVICE, "Le trousseau de secrets est de nouveau utilisable")?;
                }
                Ok(true)
            }
            Err(erreur) => {
                if self.trousseau_signale_ok.swap(false, Ordering::SeqCst) {
                    self.journal.journaliser_erreur(ETAPE_SERVICE, &erreur)?;
                }
                Ok(false)
            }
        }
    }

    fn cache(&self) -> MutexGuard<'_, BTreeMap<String, (u64, bool)>> {
        self.cache_trousseau.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Toute action sur le compte oublie la présence gardée : la prochaine interrogation relit le trousseau.
    pub(crate) fn invalider_cache(&self, nom: &str) {
        self.cache().remove(&cle_nom_compte(nom));
    }

    /// Présence du mot de passe, gardée `DUREE_CACHE_TROUSSEAU_S` secondes. Une erreur n'est jamais gardée.
    fn mot_de_passe_present(&self, nom: &str, maintenant_s: u64) -> Resultat<bool> {
        let cle = cle_nom_compte(nom);
        if let Some((lu_s, present)) = self.cache().get(&cle).copied()
            && maintenant_s.saturating_sub(lu_s) < DUREE_CACHE_TROUSSEAU_S
        {
            return Ok(present);
        }
        let present = self.coffre.lire(nom)?.is_some_and(|secret| !secret.est_vide());
        self.cache().insert(cle, (maintenant_s, present));
        Ok(present)
    }

    /// Compte par défaut effectif : celui de l'état s'il existe dans `noms`, sinon le premier. Jamais écrit dans l'état.
    fn nom_par_defaut<'a>(noms: &'a [String], etat: &EtatInterface) -> Option<&'a str> {
        etat.compte_par_defaut
            .as_deref()
            .and_then(|cle| noms.iter().find(|n| cle_nom_compte(n) == cle_nom_compte(cle)))
            .or_else(|| noms.first())
            .map(String::as_str)
    }

    fn construire(&self, nom: &str, etat: &EtatInterface, trousseau: bool, maintenant_s: u64) -> Resultat<Compte> {
        let fichier = etat.compte(nom);
        // Interrupteur désactivé = mot de passe oublié : inutile d'interroger le trousseau.
        let present = trousseau && fichier.enregistrer_mot_de_passe && self.mot_de_passe_present(nom, maintenant_s)?;
        let phase = self.registre.phase(&cle_nom_compte(nom), self.sonde.as_ref(), maintenant_s);
        Ok(compte_depuis(nom, &fichier, present, phase))
    }

    pub(crate) fn reponse_compte(&self, nom: &str) -> Resultat<ReponseCompte> {
        let etat = etat::lire(&self.dossiers)?;
        let trousseau = self.trousseau_disponible()?;
        let noms = self.noms_comptes(&self.charger_config()?)?;
        let defaut = Self::nom_par_defaut(&noms, &etat).map(cle_nom_compte);
        let mut compte = self.construire(nom, &etat, trousseau, self.horloge.maintenant_s())?;
        compte.par_defaut = defaut.is_some_and(|d| d == cle_nom_compte(nom));
        Ok(ReponseCompte { compte })
    }

    /// Commande `comptes`.
    pub fn comptes(&self) -> Resultat<ReponseComptes> {
        let config = self.charger_config()?;
        let jeu_configure = config.dossier_jeu.as_ref().is_some_and(|dossier| dossier.is_dir());
        let noms = self.noms_comptes(&config)?;
        let etat = etat::lire(&self.dossiers)?;
        let trousseau = self.trousseau_disponible()?;
        let maintenant_s = self.horloge.maintenant_s();
        let defaut = Self::nom_par_defaut(&noms, &etat).map(cle_nom_compte);
        let comptes = noms
            .iter()
            .map(|nom| {
                let mut compte = self.construire(nom, &etat, trousseau, maintenant_s)?;
                compte.par_defaut = defaut.as_deref() == Some(cle_nom_compte(nom).as_str());
                Ok(compte)
            })
            .collect::<Resultat<Vec<_>>>()?;
        // Décision 13 : un compte en lancement compte aussi.
        let en_jeu = comptes.iter().filter(|c| matches!(c.etat, EtatAffiche::EnJeu | EtatAffiche::Lancement)).count();
        Ok(ReponseComptes { comptes, en_jeu, trousseau_disponible: trousseau, jeu_configure })
    }

    /// Commande `ajouter_compte` : ajoute le nom à la configuration ; le mot de passe n'est rangé dans
    /// le trousseau que si `enregistrer` est vrai.
    pub fn ajouter_compte(&self, nom: &str, mot_de_passe: &str, enregistrer: bool) -> Resultat<ReponseCompte> {
        let _garde = self.garde();
        normaliser_nom_compte(nom).map_err(|erreur| match erreur {
            Erreur::Coffre { raison } => ErreurService::nouvelle(CODE_NOM_INVALIDE, format!("Nom de compte invalide : {raison}.")),
            autre => ErreurService::from(autre),
        })?;
        let mut config = self.charger_config()?;
        let cle = cle_nom_compte(nom);
        if self.noms_comptes(&config)?.iter().any(|existant| cle_nom_compte(existant) == cle) {
            return Err(ErreurService::nouvelle(CODE_COMPTE_EXISTANT, format!("Le compte « {nom} » existe déjà.")));
        }
        if enregistrer {
            if mot_de_passe.is_empty() {
                return Err(mot_de_passe_requis(nom));
            }
            let enregistre = self.coffre.enregistrer(nom, &Secret::new(mot_de_passe.to_string()));
            self.invalider_cache(nom);
            enregistre?;
        }
        if let Err(erreur) = self.ajouter_a_la_configuration(&mut config, nom, enregistrer) {
            if enregistrer {
                self.annuler_enregistrement(nom);
            }
            return Err(erreur);
        }
        self.noter(Niveau::Info, "Compte ajouté", nom, None)?;
        self.reponse_compte(nom)
    }

    fn ajouter_a_la_configuration(&self, config: &mut Config, nom: &str, enregistrer: bool) -> Resultat<()> {
        config.comptes.push(nom.to_string());
        config.enregistrer(&self.dossiers)?;
        let mut etat = etat::lire(&self.dossiers)?;
        etat.compte_mut(nom).enregistrer_mot_de_passe = enregistrer;
        etat::ecrire(&self.dossiers, &etat)
    }

    /// Défait l'enregistrement d'un compte dont l'ajout a échoué ; si cela échoue aussi, c'est journalisé.
    fn annuler_enregistrement(&self, nom: &str) {
        let supprime = self.coffre.supprimer(nom);
        self.invalider_cache(nom);
        if let Err(erreur) = supprime {
            let message = format!("Le mot de passe du compte « {nom} » n'a pas pu être retiré du trousseau après l'échec de l'ajout : {erreur}");
            if let Err(erreur_journal) = self.noter(Niveau::Erreur, &message, nom, None) {
                eprintln!("Journal indisponible : {erreur_journal}");
            }
        }
    }

    /// Commande `enregistrer_mot_de_passe` (« se connecter »). Refusée quand l'interrupteur du compte est désactivé.
    pub fn enregistrer_mot_de_passe(&self, compte: &str, mot_de_passe: &str) -> Resultat<ReponseCompte> {
        let _garde = self.garde();
        let config = self.charger_config()?;
        let nom = self.exiger_compte(&config, compte)?;
        if !etat::lire(&self.dossiers)?.compte(&nom).enregistrer_mot_de_passe {
            return Err(ErreurService::nouvelle(
                CODE_ENREGISTREMENT_DESACTIVE,
                format!("L'enregistrement du mot de passe est désactivé pour le compte « {nom} » : activez-le d'abord."),
            ));
        }
        if mot_de_passe.is_empty() {
            return Err(mot_de_passe_requis(&nom));
        }
        let enregistre = self.coffre.enregistrer(&nom, &Secret::new(mot_de_passe.to_string()));
        self.invalider_cache(&nom);
        enregistre?;
        self.noter(Niveau::Info, "Mot de passe enregistré", &nom, None)?;
        self.reponse_compte(&nom)
    }

    /// Commande `definir_enregistrement` : `actif = false` oublie le mot de passe ; `true` ne l'enregistre pas.
    pub fn definir_enregistrement(&self, compte: &str, actif: bool) -> Resultat<ReponseCompte> {
        let _garde = self.garde();
        let config = self.charger_config()?;
        let nom = self.exiger_compte(&config, compte)?;
        if !actif {
            let supprime = self.coffre.supprimer(&nom);
            self.invalider_cache(&nom);
            supprime?;
        }
        let mut etat = etat::lire(&self.dossiers)?;
        etat.compte_mut(&nom).enregistrer_mot_de_passe = actif;
        etat::ecrire(&self.dossiers, &etat)?;
        let message = if actif { "Enregistrement du mot de passe activé" } else { "Enregistrement du mot de passe désactivé : mot de passe oublié" };
        self.noter(Niveau::Info, message, &nom, None)?;
        self.reponse_compte(&nom)
    }

    /// Commande `definir_compte_par_defaut` : choisit le compte pré-sélectionné pour « Tout lancer ».
    pub fn definir_compte_par_defaut(&self, compte: &str) -> Resultat<ReponseComptes> {
        let _garde = self.garde();
        let config = self.charger_config()?;
        let nom = self.exiger_compte(&config, compte)?;
        let mut etat = etat::lire(&self.dossiers)?;
        etat.compte_par_defaut = Some(cle_nom_compte(&nom));
        etat::ecrire(&self.dossiers, &etat)?;
        self.noter(Niveau::Info, &format!("Compte par défaut : {nom}"), &nom, None)?;
        self.comptes()
    }

    /// Commande `se_deconnecter` : oublie le mot de passe, garde l'interrupteur.
    pub fn se_deconnecter(&self, compte: &str) -> Resultat<ReponseCompte> {
        let _garde = self.garde();
        let config = self.charger_config()?;
        let nom = self.exiger_compte(&config, compte)?;
        let supprime = self.coffre.supprimer(&nom);
        self.invalider_cache(&nom);
        supprime?;
        self.noter(Niveau::Info, "Compte déconnecté : mot de passe oublié", &nom, None)?;
        self.reponse_compte(&nom)
    }
}

pub(crate) fn mot_de_passe_requis(nom: &str) -> ErreurService {
    ErreurService::nouvelle(CODE_MOT_DE_PASSE_REQUIS, format!("Le mot de passe du compte « {nom} » est requis."))
}
