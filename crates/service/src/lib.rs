//! Couche « service » d'EbonHux : comptes, interrupteur d'enregistrement du mot de passe, suivi des
//! comptes lancés, lancer / tout lancer. Rust pur, sans interface : la coque Tauri n'ajoutera que
//! la traduction des commandes du contrat (`docs/interface.md`, section 3).
//!
//! État « Lancement » puis « En jeu » : c'est une estimation par le temps (`DUREE_LANCEMENT_S`
//! secondes après le lancement du processus). Le launcher ne peut pas savoir si le personnage est
//! entré dans le monde ; seule la vie du processus est observée.

pub mod addons;
pub mod aiguillage;
pub mod comptes;
pub mod diagnostic;
pub mod erreur;
pub mod etat;
mod lancer;
pub mod langue;
pub mod preparation;
pub mod rapport;
pub mod reel;
pub mod sessions;
pub mod site;

pub use comptes::{Compte, EtatAffiche, ReponseCompte, ReponseComptes, Service};
pub use erreur::{ErreurService, Resultat};
pub use site::{Ouvreur, OuvreurReel, OuvreurSimule};
pub use lancer::{IdentifiantsLancement, ReponseToutLancer, ResultatLancementCompte};
