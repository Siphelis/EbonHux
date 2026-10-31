//! Logique de la coque, sans fenêtre : création paresseuse du service, erreur conservée et rendue
//! à chaque appel. Rien ici ne journalise un argument de commande (il peut contenir un mot de passe).

use std::sync::{Arc, OnceLock};

use ebonhux_service::{ErreurService, Service};
use serde::Serialize;
use serde_json::Value;

const CODE_TACHE: &str = "tache_interrompue";

/// Étiquette de la fenêtre principale (`tauri.conf.json` et `capabilities/defaut.json` la nomment aussi).
pub const FENETRE_PRINCIPALE: &str = "principale";

/// Ce que la coque sait demander au service (le vrai `Service`, ou un simulacre en essai).
pub trait Executant: Send + Sync {
    fn executer(&self, commande: &str, args: Value) -> Result<Value, ErreurService>;
}

impl Executant for Service {
    fn executer(&self, commande: &str, args: Value) -> Result<Value, ErreurService> {
        Service::executer(self, commande, args)
    }
}

/// Erreur rendue à JavaScript : `{ code, message }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErreurJs {
    pub code: String,
    pub message: String,
}

impl From<ErreurService> for ErreurJs {
    fn from(erreur: ErreurService) -> Self {
        Self { code: erreur.code, message: erreur.message }
    }
}

/// Erreur d'une tâche interrompue (panique du service) : le détail technique reste hors du message.
pub fn erreur_tache(detail: &str) -> ErreurJs {
    eprintln!("EbonHux : tâche du service interrompue : {detail}");
    ErreurJs { code: CODE_TACHE.to_string(), message: "Le service a été interrompu avant de répondre.".to_string() }
}

type Fabrique = Box<dyn Fn() -> Result<Arc<dyn Executant>, ErreurService> + Send + Sync>;
type Creation = Result<Arc<dyn Executant>, ErreurJs>;

struct Interieur {
    fabrique: Fabrique,
    service: OnceLock<Creation>,
}

/// État partagé de l'application : le service est créé au premier appel, une seule fois.
/// Un échec de création est conservé et rendu à chaque appel.
#[derive(Clone)]
pub struct Etat {
    interieur: Arc<Interieur>,
}

impl Etat {
    pub fn avec_fabrique(fabrique: impl Fn() -> Result<Arc<dyn Executant>, ErreurService> + Send + Sync + 'static) -> Self {
        Self { interieur: Arc::new(Interieur { fabrique: Box::new(fabrique), service: OnceLock::new() }) }
    }

    pub fn reel() -> Self {
        Self::avec_fabrique(|| Ok(Arc::new(Service::reel()?) as Arc<dyn Executant>))
    }

    /// Exécute une commande du contrat. Seuls le nom de la commande et le code d'erreur sont journalisés.
    pub fn executer_commande(&self, commande: &str, args: Value) -> Result<Value, ErreurJs> {
        let creation = self.interieur.service.get_or_init(|| {
            (self.interieur.fabrique)().map_err(|erreur| {
                eprintln!("EbonHux : le service n'a pas pu démarrer ({}).", erreur.code);
                ErreurJs::from(erreur)
            })
        });
        let service = match creation {
            Ok(service) => service,
            Err(erreur) => return Err(erreur.clone()),
        };
        service.executer(commande, args).map_err(|erreur| {
            eprintln!("EbonHux : la commande « {commande} » a échoué ({}).", erreur.code);
            ErreurJs::from(erreur)
        })
    }
}

#[cfg(test)]
mod essais {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ebonhux_coeur::config::Dossiers;
    use serde_json::json;

    use super::*;

    const SECRET: &str = "Mdp-factice-9e7c";

    struct Echo;
    impl Executant for Echo {
        fn executer(&self, commande: &str, args: Value) -> Result<Value, ErreurService> {
            Ok(json!({ "commande": commande, "args": args }))
        }
    }

    struct Refuse;
    impl Executant for Refuse {
        fn executer(&self, _commande: &str, args: Value) -> Result<Value, ErreurService> {
            Err(ErreurService::nouvelle("refus_simule", format!("Refus simulé ({} champs).", args.as_object().map_or(0, |o| o.len()))))
        }
    }

    fn etat_echo() -> Etat {
        Etat::avec_fabrique(|| Ok(Arc::new(Echo) as Arc<dyn Executant>))
    }

    fn etat_service_absent() -> Etat {
        Etat::avec_fabrique(|| Err(ErreurService::nouvelle("configuration_invalide", "Dossier personnel introuvable.")))
    }

    fn etat_reel_temporaire(racine: &std::path::Path) -> Etat {
        let dossiers = Dossiers::avec_racine(racine);
        Etat::avec_fabrique(move || Ok(Arc::new(Service::assembler(dossiers.clone(), BTreeMap::new())) as Arc<dyn Executant>))
    }

    #[test]
    fn service_absent_rend_son_code_et_son_message() {
        let erreur = etat_service_absent().executer_commande("comptes", json!({})).unwrap_err();
        assert_eq!(erreur.code, "configuration_invalide");
        assert_eq!(erreur.message, "Dossier personnel introuvable.");
    }

    #[test]
    fn service_absent_rend_la_meme_erreur_a_chaque_appel_sans_recreer() {
        let appels = Arc::new(AtomicUsize::new(0));
        let compteur = appels.clone();
        let etat = Etat::avec_fabrique(move || {
            compteur.fetch_add(1, Ordering::SeqCst);
            Err(ErreurService::nouvelle("configuration_invalide", "Echec."))
        });
        let premiere = etat.executer_commande("comptes", json!({})).unwrap_err();
        let seconde = etat.executer_commande("langue", json!({})).unwrap_err();
        assert_eq!(premiere, seconde);
        assert_eq!(appels.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn service_cree_au_premier_appel_seulement() {
        let appels = Arc::new(AtomicUsize::new(0));
        let compteur = appels.clone();
        let etat = Etat::avec_fabrique(move || {
            compteur.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Echo) as Arc<dyn Executant>)
        });
        assert_eq!(appels.load(Ordering::SeqCst), 0);
        etat.executer_commande("comptes", json!({})).unwrap();
        etat.executer_commande("comptes", json!({})).unwrap();
        assert_eq!(appels.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn commande_et_arguments_sont_transmis_tels_quels() {
        let reponse = etat_echo().executer_commande("lancer", json!({ "compte": "Aldrik" })).unwrap();
        assert_eq!(reponse, json!({ "commande": "lancer", "args": { "compte": "Aldrik" } }));
    }

    #[test]
    fn commande_inconnue_du_vrai_service() {
        let dossier = tempfile::tempdir().unwrap();
        let erreur = etat_reel_temporaire(dossier.path()).executer_commande("detruire_tout", json!({})).unwrap_err();
        assert_eq!(erreur.code, "commande_inconnue");
    }

    #[test]
    fn chemin_nominal_du_vrai_service_assemble_sur_un_dossier_temporaire() {
        let dossier = tempfile::tempdir().unwrap();
        let reponse = etat_reel_temporaire(dossier.path()).executer_commande("diagnostic", json!({})).unwrap();
        assert!(reponse.get("systeme").is_some(), "réponse sans état : {reponse}");
    }

    #[test]
    fn erreur_du_vrai_service_gardee_avec_son_code() {
        let dossier = tempfile::tempdir().unwrap();
        let erreur = etat_reel_temporaire(dossier.path()).executer_commande("addons", json!({ "intrus": 1 })).unwrap_err();
        assert_eq!(erreur.code, "arguments_invalides");
    }

    #[test]
    fn erreur_du_service_gardee_sans_fuite_de_l_argument() {
        let erreur = Etat::avec_fabrique(|| Ok(Arc::new(Refuse) as Arc<dyn Executant>))
            .executer_commande("lancer", json!({ "compte": "Aldrik", "mot_de_passe": SECRET }))
            .unwrap_err();
        assert_eq!(erreur.code, "refus_simule");
        assert!(!erreur.message.contains(SECRET));
    }

    #[test]
    fn erreur_du_vrai_service_ne_contient_pas_le_mot_de_passe() {
        let dossier = tempfile::tempdir().unwrap();
        let erreur = etat_reel_temporaire(dossier.path())
            .executer_commande("lancer", json!({ "compte": "Inconnu", "mot_de_passe": SECRET, "intrus": true }))
            .unwrap_err();
        assert!(!erreur.message.contains(SECRET) && !erreur.code.contains(SECRET));
    }

    fn json_du_fichier(texte: &str) -> Value {
        serde_json::from_str(texte).unwrap()
    }

    const CONFIG: &str = include_str!("../tauri.conf.json");
    const CAPACITE: &str = include_str!("../capabilities/defaut.json");

    #[test]
    fn capacite_ne_donne_que_le_dialogue_d_ouverture() {
        let capacite = json_du_fichier(CAPACITE);
        assert_eq!(capacite["permissions"], json!(["core:default", "dialog:allow-open"]));
        assert_eq!(capacite["windows"], json!([FENETRE_PRINCIPALE]));
    }

    #[test]
    fn configuration_garde_le_global_tauri_et_une_csp_stricte() {
        let config = json_du_fichier(CONFIG);
        assert_eq!(config["app"]["withGlobalTauri"], json!(true));
        assert_eq!(config["app"]["windows"][0]["label"], json!(FENETRE_PRINCIPALE));
        let csp = config["app"]["security"]["csp"].as_str().unwrap();
        assert!(csp.starts_with("default-src 'none'"));
        assert!(!csp.contains("unsafe-"));
        assert!(!csp.contains('*'));
        // Le dialogue passe par l'IPC : seule cette origine est ouverte au réseau.
        assert!(csp.contains("connect-src ipc: http://ipc.localhost;"));
    }

    #[test]
    fn identite_du_binaire_et_du_bureau_est_alignee() {
        let config = json_du_fichier(CONFIG);
        assert_eq!(config["mainBinaryName"], json!("ebonhux-launcher"));
        assert_eq!(config["identifier"], json!("app.ebonhux.launcher"));
    }

    #[test]
    fn erreur_js_se_serialise_en_code_et_message() {
        let json = serde_json::to_value(ErreurJs::from(ErreurService::nouvelle("x_code", "Un message."))).unwrap();
        assert_eq!(json, json!({ "code": "x_code", "message": "Un message." }));
    }

    #[test]
    fn tache_interrompue_rend_une_erreur_sans_detail_technique() {
        let erreur = erreur_tache("panic interne secret-technique");
        assert_eq!(erreur.code, "tache_interrompue");
        assert!(!erreur.message.contains("secret-technique"));
    }
}
