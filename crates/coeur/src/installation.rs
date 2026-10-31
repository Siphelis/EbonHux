//! Installation de Wine et de Lutris par le gestionnaire de paquets de la distribution.
//!
//! `planifier` est pure : elle ne lit que le `Systeme` déjà sondé. `executer` passe par le
//! port `Executeur`. Idempotence : le plan ne mémorise rien, il se recalcule depuis un
//! `Systeme` re-sondé, donc rejouer après un échec partiel ne refait que ce qui manque encore.
//! Aucun chemin système n'est lu ici : la `racine` est celle de la sonde du module `systeme`.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::erreur::{Erreur, Result};
use crate::executeur::{Commande, Executeur, grouper_eleve};
use crate::journal::{Journal, Niveau};
use crate::systeme::{FamillePaquets, FamillePilote, ModeWine, Systeme};

pub const ETAPE_JOURNAL: &str = "installation";
pub const MESSAGE_RIEN_A_INSTALLER: &str = "rien à installer";

pub const NOM_WINE: &str = "wine";
pub const NOM_LUTRIS: &str = "lutris";
pub const NOM_WINE32_APT: &str = "wine32:i386";
pub const NOM_VULKAN32_APT: &str = "libvulkan1:i386";
pub const NOM_MESA_VULKAN32_APT: &str = "mesa-vulkan-drivers:i386";
pub const NOM_NVIDIA32_DEBIAN: &str = "nvidia-driver-libs:i386";
pub const PREFIXE_NVIDIA32_UBUNTU: &str = "libnvidia-gl-";
pub const SUFFIXE_ARCHITECTURE_I386: &str = ":i386";
pub const ARCHITECTURE_I386: &str = "i386";
pub const ID_UBUNTU: &str = "ubuntu";

const PROGRAMME_EOPKG: &str = "eopkg";
const PROGRAMME_APT_GET: &str = "apt-get";
const PROGRAMME_DPKG: &str = "dpkg";
const PROGRAMME_DNF: &str = "dnf";
const PROGRAMME_PACMAN: &str = "pacman";
const VARIABLE_APT_NON_INTERACTIF: &str = "DEBIAN_FRONTEND";
const VALEUR_APT_NON_INTERACTIF: &str = "noninteractive";
const LONGUEUR_MAX_NOM_PAQUET: usize = 100;

/// Un nom de paquet n'entre dans une commande qu'après cette validation : minuscules,
/// chiffres, `+ . -`, et au plus un suffixe d'architecture `:xxx`. Ni espace ni caractère de shell.
pub fn valider_nom_paquet(nom: &str) -> Result<&str> {
    let invalide = |raison: &str| Erreur::analyse("un nom de paquet", format!("« {nom} » : {raison}"));
    if nom.is_empty() || nom.len() > LONGUEUR_MAX_NOM_PAQUET {
        return Err(invalide("longueur invalide"));
    }
    let (base, architecture) = match nom.split_once(':') {
        Some((base, architecture)) => (base, Some(architecture)),
        None => (nom, None),
    };
    let debut_ok = base.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let base_ok = base.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+.-".contains(c));
    let architecture_ok = architecture
        .is_none_or(|a| !a.is_empty() && a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    if debut_ok && base_ok && architecture_ok {
        Ok(nom)
    } else {
        Err(invalide("caractère ou forme non autorisé"))
    }
}

#[derive(Debug)]
pub struct EtapePlan {
    pub description: String,
    pub commandes: Vec<Commande>,
    pub elevation: bool,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub etapes: Vec<EtapePlan>,
    /// Points que le plan ne peut pas régler seul, à montrer à l'utilisateur.
    pub notes: Vec<String>,
}

impl Plan {
    pub fn est_vide(&self) -> bool {
        self.etapes.is_empty()
    }

    pub fn resume(&self) -> String {
        if self.est_vide() {
            return MESSAGE_RIEN_A_INSTALLER.to_string();
        }
        self.etapes
            .iter()
            .enumerate()
            .map(|(i, e)| format!("{}. {}", i + 1, e.description))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Aucune commande lancée : le plan est journalisé et rendu.
    #[default]
    Simulation,
    Reel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Statut {
    Simulee,
    Reussie,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatEtape {
    pub description: String,
    pub statut: Statut,
    pub duree_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rapport {
    pub mode: Mode,
    pub resultats: Vec<ResultatEtape>,
    /// Ligne de la commande élevée unique (`pkexec sh -c …`), s'il y en a une.
    pub commande_elevee: Option<String>,
    pub message: String,
}

fn branche_nvidia(version: &str) -> Option<&str> {
    let branche = version.split('.').next()?;
    (!branche.is_empty() && branche.chars().all(|c| c.is_ascii_digit())).then_some(branche)
}

fn est_ubuntu(systeme: &Systeme) -> bool {
    systeme
        .distribution
        .as_ref()
        .is_some_and(|d| d.id == ID_UBUNTU || d.id_like.iter().any(|l| l == ID_UBUNTU))
}

fn etape_elevee(description: String, commande: Commande) -> EtapePlan {
    EtapePlan {
        description,
        commandes: vec![commande.eleve()],
        elevation: true,
    }
}

fn commande_paquets(programme: &str, options: &[&str], paquets: &[String], apt: bool) -> Commande {
    let mut commande = Commande::new(programme);
    for option in options {
        commande = commande.arg(option);
    }
    for paquet in paquets {
        commande = commande.arg(paquet);
    }
    if apt {
        commande = commande.env(VARIABLE_APT_NON_INTERACTIF, VALEUR_APT_NON_INTERACTIF);
    }
    commande
}

fn valider_tous(noms: Vec<String>) -> Result<Vec<String>> {
    for nom in &noms {
        valider_nom_paquet(nom)?;
    }
    Ok(noms)
}

/// Paquets d'un Wine à l'ancien mode (Debian, Ubuntu), 32 bits compris.
fn paquets_apt(systeme: &Systeme, notes: &mut Vec<String>) -> Vec<String> {
    let besoin_32 = match &systeme.wine {
        None => true,
        Some(wine) => wine.mode == ModeWine::Ancien,
    };
    let mut paquets = Vec::new();
    if systeme.wine.is_none() {
        paquets.push(NOM_WINE.to_string());
        paquets.push(NOM_WINE32_APT.to_string());
    }
    if besoin_32 && !systeme.vulkan.pilote_32_present {
        paquets.push(NOM_VULKAN32_APT.to_string());
        match &systeme.pilote {
            FamillePilote::Nvidia { version } if est_ubuntu(systeme) => {
                match version.as_deref().and_then(branche_nvidia) {
                    Some(branche) => paquets.push(format!("{PREFIXE_NVIDIA32_UBUNTU}{branche}{SUFFIXE_ARCHITECTURE_I386}")),
                    None => notes.push(
                        "version du pilote NVIDIA inconnue : la bibliothèque NVIDIA 32 bits (libnvidia-gl-<branche>:i386) n'est pas planifiée, à installer à la main".to_string(),
                    ),
                }
            }
            FamillePilote::Nvidia { .. } => paquets.push(NOM_NVIDIA32_DEBIAN.to_string()),
            _ => paquets.push(NOM_MESA_VULKAN32_APT.to_string()),
        }
    }
    if systeme.lutris.is_none() {
        paquets.push(NOM_LUTRIS.to_string());
        notes.push(if est_ubuntu(systeme) {
            "lutris est dans le dépôt multiverse : il doit être activé".to_string()
        } else {
            "lutris est dans le composant contrib : il doit être activé".to_string()
        });
    }
    paquets
}

/// Plan d'installation (fonction pure). Famille inconnue : erreur, jamais un plan vide muet.
pub fn planifier(systeme: &Systeme) -> Result<Plan> {
    let famille = systeme
        .distribution
        .as_ref()
        .map(|d| &d.famille)
        .ok_or_else(|| Erreur::analyse("la famille de paquets", "distribution non détectée"))?;
    if *famille == FamillePaquets::Inconnue {
        return Err(Erreur::analyse(
            "la famille de paquets",
            "distribution non prise en charge (Solus, Debian/Ubuntu, Fedora et Arch le sont)",
        ));
    }
    let mut plan = Plan::default();
    if let Some(wine) = &systeme.wine
        && wine.mode == ModeWine::Inconnu
    {
        plan.notes.push("mode de Wine indéterminé : Wine est considéré comme présent".to_string());
    }
    let mut notes = Vec::new();
    let paquets = if *famille == FamillePaquets::Apt {
        paquets_apt(systeme, &mut notes)
    } else {
        let mut paquets = Vec::new();
        if systeme.wine.is_none() {
            paquets.push(NOM_WINE.to_string());
        }
        if systeme.lutris.is_none() {
            paquets.push(NOM_LUTRIS.to_string());
        }
        paquets
    };
    let paquets = valider_tous(paquets)?;
    if paquets.is_empty() {
        return Ok(plan);
    }
    plan.notes.extend(notes);
    let liste = paquets.join(" ");
    match famille {
        FamillePaquets::Eopkg => plan.etapes.push(etape_elevee(
            format!("Installer avec eopkg : {liste}"),
            commande_paquets(PROGRAMME_EOPKG, &["install", "-y"], &paquets, false),
        )),
        FamillePaquets::Dnf => plan.etapes.push(etape_elevee(
            format!("Installer avec dnf : {liste}"),
            commande_paquets(PROGRAMME_DNF, &["-y", "install"], &paquets, false),
        )),
        FamillePaquets::Pacman => plan.etapes.push(etape_elevee(
            format!("Installer avec pacman : {liste}"),
            commande_paquets(PROGRAMME_PACMAN, &["-S", "--noconfirm", "--needed"], &paquets, false),
        )),
        FamillePaquets::Apt => {
            if paquets.iter().any(|p| p.ends_with(SUFFIXE_ARCHITECTURE_I386)) {
                plan.etapes.push(etape_elevee(
                    format!("Activer l'architecture {ARCHITECTURE_I386} (paquets 32 bits)"),
                    Commande::new(PROGRAMME_DPKG).arg("--add-architecture").arg(ARCHITECTURE_I386),
                ));
            }
            plan.etapes.push(etape_elevee(
                "Mettre à jour la liste des paquets avec apt-get".to_string(),
                commande_paquets(PROGRAMME_APT_GET, &["-y", "update"], &[], true),
            ));
            plan.etapes.push(etape_elevee(
                format!("Installer avec apt-get : {liste}"),
                commande_paquets(PROGRAMME_APT_GET, &["-y", "install"], &paquets, true),
            ));
        }
        FamillePaquets::Inconnue => {
            return Err(Erreur::analyse("la famille de paquets", "inconnue"));
        }
    }
    Ok(plan)
}

fn contexte_duree(duree_ms: u128) -> BTreeMap<String, String> {
    BTreeMap::from([("duree_ms".to_string(), duree_ms.to_string())])
}

fn avec_etape(erreur: Erreur, description: &str) -> Erreur {
    match erreur {
        Erreur::Commande { programme, code, sortie } => Erreur::Commande {
            programme,
            code,
            sortie: format!("étape « {description} » : {sortie}"),
        },
        Erreur::Elevation { raison } => Erreur::Elevation {
            raison: format!("étape « {description} » : {raison}"),
        },
        autre => autre,
    }
}

/// Lance une unité, journalise début, résultat et durée ; un échec est journalisé puis rendu.
fn lancer_unite(
    executeur: &dyn Executeur,
    journal: &Journal,
    description: &str,
    commande: &Commande,
) -> Result<ResultatEtape> {
    journal.info(ETAPE_JOURNAL, &format!("début : {description}"))?;
    let debut = Instant::now();
    let issue = executeur.lancer(commande).and_then(|sortie| sortie.exiger_succes(commande));
    let duree_ms = debut.elapsed().as_millis();
    match issue {
        Ok(_) => {
            journal.evenement(
                Niveau::Info,
                ETAPE_JOURNAL,
                &format!("réussie : {description}"),
                &contexte_duree(duree_ms),
                None,
            )?;
            Ok(ResultatEtape { description: description.to_string(), statut: Statut::Reussie, duree_ms })
        }
        Err(erreur) => {
            let erreur = avec_etape(erreur, description);
            journal.evenement(
                Niveau::Erreur,
                ETAPE_JOURNAL,
                &format!("échec : {description}"),
                &contexte_duree(duree_ms),
                Some(erreur.to_string()),
            )?;
            Err(erreur)
        }
    }
}

/// Exécute le plan. Simulation : rien n'est envoyé à l'exécuteur. Réel : toutes les étapes
/// élevées forment UNE commande `pkexec sh -c` placée à la première étape élevée ; arrêt au
/// premier échec, avec une `Erreur` qui nomme l'étape.
pub fn executer(plan: Plan, mode: Mode, executeur: &dyn Executeur, journal: &Journal) -> Result<Rapport> {
    for note in &plan.notes {
        journal.avertir(ETAPE_JOURNAL, note)?;
    }
    if plan.est_vide() {
        journal.info(ETAPE_JOURNAL, MESSAGE_RIEN_A_INSTALLER)?;
        return Ok(Rapport {
            mode,
            resultats: Vec::new(),
            commande_elevee: None,
            message: MESSAGE_RIEN_A_INSTALLER.to_string(),
        });
    }
    let libelle_mode = if mode == Mode::Reel { "exécution" } else { "simulation" };
    journal.info(ETAPE_JOURNAL, &format!("plan ({libelle_mode}) :\n{}", plan.resume()))?;

    let mut descriptions_elevees = Vec::new();
    let mut commandes_elevees = Vec::new();
    let mut ordre: Vec<Option<(String, Commande)>> = Vec::new();
    for etape in plan.etapes {
        if etape.elevation {
            if descriptions_elevees.is_empty() {
                ordre.push(None);
            }
            descriptions_elevees.push(etape.description);
            commandes_elevees.extend(etape.commandes);
        } else {
            ordre.extend(etape.commandes.into_iter().map(|c| Some((etape.description.clone(), c))));
        }
    }
    let mut groupe = if commandes_elevees.is_empty() {
        None
    } else {
        let description = format!(
            "Installation avec droits ({} étape(s) : {})",
            descriptions_elevees.len(),
            descriptions_elevees.join(" ; ")
        );
        Some((description, grouper_eleve(commandes_elevees)?))
    };
    let commande_elevee = groupe.as_ref().map(|(_, commande)| commande.affichage());
    if let Some(ligne) = &commande_elevee {
        journal.info(ETAPE_JOURNAL, &format!("commande élevée unique : {ligne}"))?;
    }

    let mut resultats = Vec::new();
    for unite in ordre {
        let Some((description, commande)) = unite.or_else(|| groupe.take()) else {
            return Err(Erreur::analyse("le plan d'installation", "groupe élevé introuvable"));
        };
        if mode == Mode::Simulation {
            journal.info(ETAPE_JOURNAL, &format!("simulée : {description}"))?;
            resultats.push(ResultatEtape { description, statut: Statut::Simulee, duree_ms: 0 });
        } else {
            resultats.push(lancer_unite(executeur, journal, &description, &commande)?);
        }
    }
    let message = match mode {
        Mode::Simulation => format!("simulation : {} unité(s) planifiée(s), rien n'a été lancé", resultats.len()),
        Mode::Reel => format!("installation terminée : {} unité(s) réussie(s)", resultats.len()),
    };
    journal.info(ETAPE_JOURNAL, &message)?;
    Ok(Rapport { mode, resultats, commande_elevee, message })
}

/// Raccourci : planifie depuis le `Systeme` (re-sondé par l'appelant) puis exécute.
pub fn installer(systeme: &Systeme, mode: Mode, executeur: &dyn Executeur, journal: &Journal) -> Result<Rapport> {
    match planifier(systeme) {
        Ok(plan) => executer(plan, mode, executeur, journal),
        Err(erreur) => {
            journal.journaliser_erreur(ETAPE_JOURNAL, &erreur)?;
            Err(erreur)
        }
    }
}
