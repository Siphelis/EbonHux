//! Bibliothèque cœur d'EbonHux. Ce fichier déclare tous les modules, y compris ceux
//! que les tâches suivantes rempliront : elles n'ont ainsi jamais à le modifier.

pub mod addons;
pub mod awesomewotlk;
pub mod coffre;
pub mod comptes;
pub mod config;
pub mod dossier_lancement;
pub mod dxvk;
pub mod erreur;
pub mod executeur;
pub mod installation;
pub mod journal;
pub mod lancement;
pub mod prefixe;
pub mod reseau;
pub mod secret;
pub mod systeme;

pub use erreur::{Erreur, Result};
