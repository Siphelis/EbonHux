//! Commandes `addons`, `definir_suivi`, `catalogue`, `installer_addon` (docs/interface.md,
//! section 3). Le cœur lit les `.toc` et installe ; le service regroupe (bibliothèques, modules
//! `Parent_xxx`), garde le suivi dans `etat-interface.json` et ne touche jamais au jeu hors installation.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use ebonhux_coeur::addons::{
    AddonInstalle, EntreeCatalogue, OptionsInstallation, analyser_catalogue, depuis_api_elendil, desinstaller, dossier_addons, installer_entree, lister,
    version_plus_recente,
};
use ebonhux_coeur::config::Dossiers;
use ebonhux_coeur::erreur::Erreur;
use ebonhux_coeur::journal::Niveau;
use serde::Serialize;

use crate::comptes::Service;
use crate::erreur::{CODE_ADDON_INCONNU, CODE_ENTREE_INCONNUE, CODE_JEU_NON_CONFIGURE, CODE_PORT_ABSENT, ErreurService, Resultat};
use crate::etat::{self, EtatInterface, InfoAddonInstalle};
use crate::lancer::horodatage_iso;

const ETAPE_ADDONS: &str = "addons";
const SEPARATEUR_MODULE: char = '_';
const PREFIXES_BIBLIOTHEQUE: [&str; 3] = ["Lib", "Ace", "CallbackHandler"];
const MOT_BIBLIOTHEQUE_DANS_TITRE: &str = "Library";
const PREFIXE_ADRESSE: &str = "https://";
const URL_API_CATALOGUE: &str = "https://web.way-of-elendil.fr/api/web/addons";
const LIMITE_API_CATALOGUE: u64 = 20_000_000;
const NOM_CACHE_CATALOGUE: &str = "catalogue.json";
const NOM_TELECHARGEMENT_CATALOGUE: &str = "catalogue-api.tmp";
const MODE_FICHIER: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleAddon {
    pub dossier: String,
    pub titre: String,
    pub suivi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddonListe {
    pub dossier: String,
    pub titre: String,
    pub notes: Option<String>,
    pub auteur: Option<String>,
    pub version: Option<String>,
    pub suivi: bool,
    pub bibliotheque: bool,
    pub modules: Vec<ModuleAddon>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProblemeListe {
    pub dossier: String,
    pub raison: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseAddons {
    pub total: usize,
    pub suivis: usize,
    pub addons: Vec<AddonListe>,
    pub problemes: Vec<ProblemeListe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseSuivi {
    pub suivis: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntreeAffichee {
    pub nom: String,
    pub description: String,
    pub url: String,
    pub source: String,
    pub dossier: Option<String>,
    pub avatar: Option<String>,
    pub categories: Vec<String>,
    /// Version annoncée par le catalogue.
    pub version: Option<String>,
    /// Dossiers réellement présents dans le jeu pour cette entrée (un zip peut en installer plusieurs) ; vide si non installée.
    pub dossiers_installes: Vec<String>,
    pub installe: bool,
    /// `None` : non installé, ou version illisible d'un côté ou de l'autre (à distinguer de `Some(false)` = à jour).
    pub mise_a_jour_disponible: Option<bool>,
    pub description_longue: Option<String>,
    pub images: Vec<String>,
    /// Version lue dans l'addon installé correspondant ; `None` si non installé.
    pub version_installee: Option<String>,
    /// Auteur (`## Author`) de l'addon installé correspondant ; `None` si non installé.
    pub auteur: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseCatalogue {
    pub entrees: Vec<EntreeAffichee>,
    pub invalides: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseInstallation {
    pub installes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReponseDesinstallation {
    pub desinstalle: String,
}

/// Cache du catalogue réel : `None` s'il n'existe pas encore.
fn lire_cache_catalogue(dossiers: &Dossiers) -> Resultat<Option<String>> {
    let chemin = dossiers.cache.join(NOM_CACHE_CATALOGUE);
    match fs::read_to_string(&chemin) {
        Ok(texte) => Ok(Some(texte)),
        Err(erreur) if erreur.kind() == ErrorKind::NotFound => Ok(None),
        Err(erreur) => Err(Erreur::io("lire le cache du catalogue", &chemin, erreur).into()),
    }
}

/// Écriture atomique : fichier temporaire voisin (droits 0600), synchronisé, puis renommage.
fn ecrire_cache_catalogue(dossiers: &Dossiers, texte: &str) -> Resultat<()> {
    let chemin = dossiers.cache.join(NOM_CACHE_CATALOGUE);
    fs::create_dir_all(&dossiers.cache).map_err(|e| Erreur::io("créer le dossier de cache", &dossiers.cache, e))?;
    let temporaire = dossiers.cache.join(format!("{NOM_CACHE_CATALOGUE}.tmp"));
    let mut fichier = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(MODE_FICHIER)
        .open(&temporaire)
        .map_err(|e| Erreur::io("créer le cache temporaire du catalogue", &temporaire, e))?;
    fichier
        .set_permissions(fs::Permissions::from_mode(MODE_FICHIER))
        .map_err(|e| Erreur::io("restreindre les droits du cache du catalogue", &temporaire, e))?;
    fichier
        .write_all(texte.as_bytes())
        .map_err(|e| Erreur::io("écrire le cache temporaire du catalogue", &temporaire, e))?;
    fichier
        .sync_all()
        .map_err(|e| Erreur::io("synchroniser le cache temporaire du catalogue", &temporaire, e))?;
    fs::rename(&temporaire, &chemin).map_err(|e| Erreur::io("remplacer le cache du catalogue", &chemin, e))?;
    Ok(())
}

/// Bibliothèque : dossier commençant par `Lib`, `Ace` ou `CallbackHandler`, ou titre contenant « Library ».
pub fn est_bibliotheque(dossier: &str, titre: &str) -> bool {
    PREFIXES_BIBLIOTHEQUE.iter().any(|prefixe| dossier.starts_with(prefixe)) || titre.contains(MOT_BIBLIOTHEQUE_DANS_TITRE)
}

/// Normalise un titre d'addon pour le comparer sans tenir compte des espaces de bord ni de la casse.
fn normaliser_titre(titre: &str) -> String {
    titre.trim().to_lowercase()
}

/// Hôte d'une adresse https : sans identifiants, port, chemin, requête ni ancre. Vide si l'adresse n'est pas https.
pub fn hote_de(adresse: &str) -> String {
    let Some(reste) = adresse.strip_prefix(PREFIXE_ADRESSE) else {
        return String::new();
    };
    let autorite = reste.split(['/', '?', '#']).next().unwrap_or_default();
    let sans_identifiants = autorite.rsplit('@').next().unwrap_or_default();
    sans_identifiants.split(':').next().unwrap_or_default().to_string()
}

/// Parent d'un dossier `Parent_xxx` : le plus long préfixe (avant un `_`) qui est un autre dossier d'addon.
fn parent_de<'a>(dossier: &str, connus: &'a BTreeMap<String, String>) -> Option<&'a String> {
    dossier
        .match_indices(SEPARATEUR_MODULE)
        .filter(|(rang, _)| *rang > 0 && *rang + 1 < dossier.len())
        .filter_map(|(rang, _)| connus.get(&dossier[..rang].to_lowercase()))
        .next_back()
}

/// Parent direct d'un dossier, avec `true` si le rattachement vient du TOC. Préfixe `Parent_` d'abord (inchangé) ;
/// sinon dépendance TOC (`Dependencies`/`RequiredDeps`), seulement si elle désigne exactement un dossier installé
/// distinct et que celui-ci n'est pas une bibliothèque. Avec deux dossiers distincts ou plus : ligne simple.
fn parent_direct_de(addon: &AddonInstalle, installes: &[AddonInstalle], connus: &BTreeMap<String, String>) -> Option<(String, bool)> {
    if let Some(parent) = parent_de(&addon.dossier, connus).filter(|p| !p.eq_ignore_ascii_case(&addon.dossier)) {
        return Some((parent.clone(), false));
    }
    let mut parents: Vec<&String> = Vec::new();
    for dependance in &addon.fiche.dependances {
        if let Some(reel) = connus.get(&dependance.trim().to_lowercase())
            && !reel.eq_ignore_ascii_case(&addon.dossier)
            && !parents.contains(&reel)
        {
            parents.push(reel);
        }
    }
    let [unique] = parents.as_slice() else {
        return None;
    };
    let est_lib = installes.iter().find(|a| &a.dossier == *unique).is_some_and(|a| est_bibliotheque(&a.dossier, &a.fiche.titre));
    (!est_lib).then(|| ((*unique).clone(), true))
}

/// Regroupe les addons en parents et modules (calcul pur), profondeur 1. Un dossier est le module d'un parent
/// installé : préfixe `Parent_`, sinon première dépendance installée du TOC. Un module dont le parent est lui-même
/// un module est rattaché à la racine de la chaîne ; deux dossiers qui se rattachent l'un à l'autre (cycle) restent des lignes simples.
pub fn regrouper(installes: &[AddonInstalle], suivis: &[String]) -> Vec<AddonListe> {
    let connus: BTreeMap<String, String> = installes.iter().map(|a| (a.dossier.to_lowercase(), a.dossier.clone())).collect();
    let candidats: BTreeMap<String, (String, bool)> =
        installes.iter().filter_map(|a| parent_direct_de(a, installes, &connus).map(|p| (a.dossier.clone(), p))).collect();
    // Pas de chaîne par TOC : le parent d'un rattachement TOC doit être lui-même une racine.
    let mut parent_direct: BTreeMap<String, String> = candidats
        .iter()
        .filter(|(_, (parent, par_toc))| !*par_toc || !candidats.contains_key(parent))
        .map(|(d, (parent, _))| (d.clone(), parent.clone()))
        .collect();
    // Cycle : la chaîne de parents revient à son point de départ ; aucun de ses membres n'est module.
    let en_cycle: Vec<String> = parent_direct
        .keys()
        .filter(|depart| {
            let mut courant = *depart;
            for _ in 0..=installes.len() {
                match parent_direct.get(courant) {
                    Some(parent) if parent == *depart => return true,
                    Some(parent) => courant = parent,
                    None => return false,
                }
            }
            false
        })
        .cloned()
        .collect();
    for dossier in &en_cycle {
        parent_direct.remove(dossier);
    }
    let racine_de = |dossier: &str| -> String {
        let mut courant = dossier.to_string();
        // Sans cycle la chaîne est finie ; la borne reste une protection.
        for _ in 0..=installes.len() {
            match parent_direct.get(&courant) {
                Some(parent) => courant = parent.clone(),
                None => break,
            }
        }
        courant
    };
    let suivi = |dossier: &str| suivis.iter().any(|s| s == dossier);
    let mut sortie: Vec<AddonListe> = installes
        .iter()
        .filter(|a| !parent_direct.contains_key(&a.dossier))
        .map(|a| AddonListe {
            dossier: a.dossier.clone(),
            titre: a.fiche.titre.clone(),
            notes: a.fiche.notes.clone(),
            auteur: a.fiche.auteur.clone(),
            version: a.fiche.version.clone(),
            suivi: suivi(&a.dossier),
            bibliotheque: est_bibliotheque(&a.dossier, &a.fiche.titre),
            modules: Vec::new(),
        })
        .collect();
    for module in installes.iter().filter(|a| parent_direct.contains_key(&a.dossier)) {
        let racine = racine_de(&module.dossier);
        if let Some(parent) = sortie.iter_mut().find(|p| p.dossier == racine) {
            parent.modules.push(ModuleAddon { dossier: module.dossier.clone(), titre: module.fiche.titre.clone(), suivi: suivi(&module.dossier) });
        }
    }
    sortie
}

/// Nombre d'addons (lignes parents) suivis ; les modules ne comptent pas à part.
fn compter_suivis(groupes: &[AddonListe]) -> usize {
    groupes.iter().filter(|g| g.suivi).count()
}

fn ajouter_sans_doublon(liste: &mut Vec<String>, dossier: &str) {
    if !liste.iter().any(|existant| existant == dossier) {
        liste.push(dossier.to_string());
    }
}

/// Addon installé correspondant à une entrée de catalogue (dossier connu, dossier enregistré lors
/// d'une installation passée, ou titre), pour lire sa version réelle. Même ordre de correspondance
/// que le calcul de `installe`.
fn addon_installe_correspondant<'a>(
    entree: &EntreeCatalogue,
    installes: &'a [AddonInstalle],
    addons_installes: &BTreeMap<String, InfoAddonInstalle>,
) -> Option<&'a AddonInstalle> {
    if let Some(dossier) = &entree.dossier
        && let Some(addon) = installes.iter().find(|a| a.dossier.eq_ignore_ascii_case(dossier))
    {
        return Some(addon);
    }
    let dossier_enregistre = entree
        .dossier
        .as_ref()
        .and_then(|d| addons_installes.get(&d.to_lowercase()))
        .or_else(|| addons_installes.values().find(|info| info.addon_url.as_deref() == Some(entree.url.as_str())))
        .map(|info| info.dossier.as_str());
    if let Some(dossier) = dossier_enregistre
        && let Some(addon) = installes.iter().find(|a| a.dossier.eq_ignore_ascii_case(dossier))
    {
        return Some(addon);
    }
    installes.iter().find(|a| normaliser_titre(&a.fiche.titre) == normaliser_titre(&entree.nom))
}

/// Dossiers présents dans le jeu qui correspondent à une entrée : tous ceux enregistrés lors d'une
/// installation depuis son adresse ; à défaut, celui de la correspondance habituelle (dossier connu, titre).
/// Seuls les dossiers réellement dans `installes` sont gardés (casse ignorée) ; ordre d'enregistrement stable.
fn dossiers_installes_de(
    entree: &EntreeCatalogue,
    installes: &[AddonInstalle],
    addons_installes: &BTreeMap<String, InfoAddonInstalle>,
) -> Vec<String> {
    let mut candidats: Vec<&str> =
        addons_installes.values().filter(|info| info.addon_url.as_deref() == Some(entree.url.as_str())).map(|info| info.dossier.as_str()).collect();
    if candidats.is_empty()
        && let Some(addon) = addon_installe_correspondant(entree, installes, addons_installes)
    {
        candidats.push(addon.dossier.as_str());
    }
    let mut sortie: Vec<String> = Vec::new();
    for candidat in candidats {
        if let Some(reel) = installes.iter().find(|a| a.dossier.eq_ignore_ascii_case(candidat))
            && !sortie.iter().any(|d| d.eq_ignore_ascii_case(&reel.dossier))
        {
            sortie.push(reel.dossier.clone());
        }
    }
    sortie
}

impl Service {
    fn dossier_jeu(&self) -> Resultat<PathBuf> {
        self.charger_config()?.dossier_jeu.ok_or_else(|| {
            ErreurService::nouvelle(CODE_JEU_NON_CONFIGURE, "Le dossier du jeu n'est pas configuré : indiquez-le d'abord.")
        })
    }

    /// Commande `addons`.
    pub fn addons(&self) -> Resultat<ReponseAddons> {
        let jeu = self.dossier_jeu()?;
        let liste = lister(&jeu, &self.journal)?;
        let etat = etat::lire(&self.dossiers)?;
        let addons = regrouper(&liste.addons, &etat.addons_suivis);
        Ok(ReponseAddons {
            total: addons.len(),
            suivis: compter_suivis(&addons),
            addons,
            problemes: liste.problemes.into_iter().map(|p| ProblemeListe { dossier: p.dossier, raison: p.raison }).collect(),
        })
    }

    /// Commande `definir_suivi` : le suivi d'un parent entraîne ses modules ; celui d'un module ne touche que lui.
    pub fn definir_suivi(&self, dossier: &str, suivi: bool) -> Resultat<ReponseSuivi> {
        let _garde = self.garde();
        let jeu = self.dossier_jeu()?;
        let liste = lister(&jeu, &self.journal)?;
        let groupes = regrouper(&liste.addons, &[]);
        let mut concernes: Vec<String> = Vec::new();
        if let Some(parent) = groupes.iter().find(|g| g.dossier == dossier) {
            concernes.push(parent.dossier.clone());
            concernes.extend(parent.modules.iter().map(|m| m.dossier.clone()));
        } else if liste.addons.iter().any(|a| a.dossier == dossier) {
            concernes.push(dossier.to_string());
        } else {
            return Err(ErreurService::nouvelle(CODE_ADDON_INCONNU, format!("L'addon « {dossier} » n'est pas installé.")));
        }
        let mut etat = etat::lire(&self.dossiers)?;
        if suivi {
            for dossier in &concernes {
                ajouter_sans_doublon(&mut etat.addons_suivis, dossier);
            }
        } else {
            etat.addons_suivis.retain(|existant| !concernes.contains(existant));
        }
        etat::ecrire(&self.dossiers, &etat)?;
        let message = if suivi { "Suivi des mises à jour activé" } else { "Suivi des mises à jour désactivé" };
        self.journal.evenement(
            Niveau::Info,
            ETAPE_ADDONS,
            message,
            &BTreeMap::from([("addon".to_string(), dossier.to_string())]),
            None,
        )?;
        Ok(ReponseSuivi { suivis: compter_suivis(&regrouper(&liste.addons, &etat.addons_suivis)) })
    }

    fn catalogue_analyse(&self) -> Resultat<ebonhux_coeur::addons::Catalogue> {
        let texte = lire_cache_catalogue(&self.dossiers)?;
        let catalogue = analyser_catalogue(texte.as_deref().unwrap_or(&self.catalogue));
        if let Err(erreur) = &catalogue {
            self.journal.journaliser_erreur(ETAPE_ADDONS, erreur)?;
        }
        Ok(catalogue?)
    }

    /// Commande `catalogue`.
    ///
    /// L'API way-of-elendil ne donne aucun nom de dossier pour ses entrées (`dossier` reste `None`
    /// tant que l'addon n'a pas été téléchargé) : la correspondance par dossier ne peut donc jouer
    /// que pour le catalogue embarqué. Pour reconnaître un addon déjà présent (installé avant
    /// l'usage du catalogue, à la main ou par un autre moyen), on compare aussi son titre réel
    /// (lu dans son `.toc`, ou le nom du dossier à défaut) au nom affiché dans le catalogue.
    pub fn catalogue(&self) -> Resultat<ReponseCatalogue> {
        let catalogue = self.catalogue_analyse()?;
        let config = self.charger_config()?;
        let etat: EtatInterface = etat::lire(&self.dossiers)?;
        let installes: Vec<AddonInstalle> = match &config.dossier_jeu {
            Some(jeu) => lister(jeu, &self.journal)?.addons,
            None => Vec::new(),
        };
        let entrees = catalogue
            .entrees
            .iter()
            .map(|entree| {
                let present = match (&config.dossier_jeu, &entree.dossier) {
                    (Some(jeu), Some(dossier)) => dossier_addons(jeu).join(dossier).is_dir(),
                    _ => false,
                };
                let enregistre = entree
                    .dossier
                    .as_ref()
                    .is_some_and(|dossier| etat.addons_installes.contains_key(&dossier.to_lowercase()))
                    || etat.addons_installes.values().any(|info| info.addon_url.as_deref() == Some(entree.url.as_str()));
                let correspond_par_titre = installes.iter().any(|a| normaliser_titre(&a.fiche.titre) == normaliser_titre(&entree.nom));
                let correspondant = addon_installe_correspondant(entree, &installes, &etat.addons_installes);
                let mise_a_jour_disponible =
                    correspondant.and_then(|addon| version_plus_recente(entree.version.as_deref(), addon.fiche.version.as_deref()));
                let version_installee = correspondant.and_then(|addon| addon.fiche.version.clone());
                let auteur = correspondant.and_then(|addon| addon.fiche.auteur.clone());
                EntreeAffichee {
                    nom: entree.nom.clone(),
                    description: entree.description.clone(),
                    url: entree.url.clone(),
                    source: hote_de(&entree.url),
                    dossier: entree.dossier.clone(),
                    avatar: entree.avatar.clone(),
                    categories: entree.categories.clone(),
                    version: entree.version.clone(),
                    dossiers_installes: dossiers_installes_de(entree, &installes, &etat.addons_installes),
                    installe: present || enregistre || correspond_par_titre,
                    mise_a_jour_disponible,
                    description_longue: entree.description_longue.clone(),
                    images: entree.images.clone(),
                    version_installee,
                    auteur,
                }
            })
            .collect();
        Ok(ReponseCatalogue { entrees, invalides: catalogue.invalides.len() })
    }

    /// Commande `rafraichir_catalogue` : télécharge le catalogue réel, le convertit et le garde en cache.
    pub fn rafraichir_catalogue(&self) -> Resultat<ReponseCatalogue> {
        let _garde = self.garde();
        let Some(telechargeur) = &self.telechargeur else {
            return Err(ErreurService::nouvelle(CODE_PORT_ABSENT, "Le téléchargement est indisponible dans cette installation."));
        };
        fs::create_dir_all(&self.dossiers.cache).map_err(|e| Erreur::io("créer le dossier de cache", &self.dossiers.cache, e))?;
        let fichier = self.dossiers.cache.join(NOM_TELECHARGEMENT_CATALOGUE);
        telechargeur.telecharger(URL_API_CATALOGUE, &fichier, LIMITE_API_CATALOGUE)?;
        let texte = fs::read_to_string(&fichier).map_err(|e| Erreur::io("lire le catalogue téléchargé", &fichier, e))?;
        let catalogue = match depuis_api_elendil(&texte) {
            Ok(catalogue) => catalogue,
            Err(erreur) => {
                self.journal.journaliser_erreur(ETAPE_ADDONS, &erreur)?;
                return Err(erreur.into());
            }
        };
        for invalide in &catalogue.invalides {
            self.journal.avertir(ETAPE_ADDONS, &format!("catalogue : entrée n° {} refusée : {}", invalide.position, invalide.raison))?;
        }
        let json = serde_json::to_string(&catalogue.entrees).map_err(|e| {
            ErreurService::nouvelle(CODE_PORT_ABSENT, format!("Sérialisation du catalogue impossible : {e}"))
        })?;
        ecrire_cache_catalogue(&self.dossiers, &json)?;
        self.catalogue()
    }

    fn entree_du_catalogue(&self, nom: &str) -> Resultat<EntreeCatalogue> {
        let catalogue = self.catalogue_analyse()?;
        catalogue
            .entrees
            .into_iter()
            .find(|entree| entree.nom.eq_ignore_ascii_case(nom.trim()))
            .ok_or_else(|| ErreurService::nouvelle(CODE_ENTREE_INCONNUE, format!("« {nom} » n'existe pas dans le catalogue des addons.")))
    }

    /// Commande `installer_addon` : par nom d'entrée du catalogue.
    pub fn installer_addon(&self, nom: &str) -> Resultat<ReponseInstallation> {
        self.installer_addon_avec(nom, false)
    }

    /// Comme `installer_addon`; `remplacer` autorise la mise à jour d'un addon déjà présent (l'ancien dossier est remplacé, sans sauvegarde).
    pub fn installer_addon_avec(&self, nom: &str, remplacer: bool) -> Resultat<ReponseInstallation> {
        let _garde = self.garde();
        let entree = self.entree_du_catalogue(nom)?;
        let jeu = self.dossier_jeu()?;
        let Some(telechargeur) = &self.telechargeur else {
            return Err(ErreurService::nouvelle(CODE_PORT_ABSENT, "Le téléchargement est indisponible dans cette installation."));
        };
        let resultat = installer_entree(&jeu, &entree, &OptionsInstallation { remplacer, ..OptionsInstallation::default() }, telechargeur.as_ref(), &self.journal)?;
        let mut etat = etat::lire(&self.dossiers)?;
        let installe_le = horodatage_iso(self.horloge.maintenant_s())?;
        for dossier in &resultat.installes {
            etat.addons_installes.insert(
                dossier.to_lowercase(),
                InfoAddonInstalle {
                    dossier: dossier.clone(),
                    addon_url: Some(entree.url.clone()),
                    installe_le: installe_le.clone(),
                },
            );
        }
        etat::ecrire(&self.dossiers, &etat)?;
        Ok(ReponseInstallation { installes: resultat.installes })
    }

    /// Commande `desinstaller_addon` : le dossier est réellement supprimé (un lien symbolique est seulement retiré).
    pub fn desinstaller_addon(&self, dossier: &str) -> Resultat<ReponseDesinstallation> {
        let _garde = self.garde();
        let jeu = self.dossier_jeu()?;
        let liste = lister(&jeu, &self.journal)?;
        let mut a_supprimer: Vec<String> =
            regrouper(&liste.addons, &[]).into_iter().find(|g| g.dossier.eq_ignore_ascii_case(dossier)).map(|g| g.modules.into_iter().map(|m| m.dossier).collect()).unwrap_or_default();
        a_supprimer.push(dossier.to_string());
        let mut etat = etat::lire(&self.dossiers)?;
        let mut echec: Option<ErreurService> = None;
        for cible in &a_supprimer {
            if let Err(erreur) = desinstaller(&jeu, cible, &self.journal) {
                let mut erreur = ErreurService::from(erreur);
                erreur.message = format!("Désinstallation interrompue sur le dossier « {cible} » : {}", erreur.message);
                echec = Some(erreur);
                break;
            }
            let cle = cible.to_lowercase();
            etat.addons_installes.remove(&cle);
            etat.addons_suivis.retain(|suivi| suivi.to_lowercase() != cle);
        }
        // Les dossiers déjà supprimés le restent : l'état est écrit même en cas d'échec.
        etat::ecrire(&self.dossiers, &etat)?;
        if let Some(erreur) = echec {
            return Err(erreur);
        }
        self.journal.evenement(
            Niveau::Info,
            ETAPE_ADDONS,
            "Addon désinstallé",
            &BTreeMap::from([("addon".to_string(), dossier.to_string())]),
            None,
        )?;
        Ok(ReponseDesinstallation { desinstalle: dossier.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ebonhux_coeur::addons::FicheToc;

    const URL: &str = "https://l.test/Pack.zip";

    fn entree(nom: &str) -> EntreeCatalogue {
        EntreeCatalogue { nom: nom.to_string(), description: "d".to_string(), url: URL.to_string(), dossier: None, avatar: None, categories: Vec::new(), version: None, description_longue: None, images: Vec::new() }
    }

    fn present(dossier: &str, titre: &str) -> AddonInstalle {
        AddonInstalle { dossier: dossier.to_string(), chemin: PathBuf::from(dossier), fiche: FicheToc { titre: titre.to_string(), ..FicheToc::default() } }
    }

    fn enregistre(cle: &str, dossier: &str, url: Option<&str>) -> (String, InfoAddonInstalle) {
        (cle.to_string(), InfoAddonInstalle { dossier: dossier.to_string(), addon_url: url.map(str::to_string), installe_le: "2026-10-30T10:00:00Z".to_string() })
    }

    #[test]
    fn enregistre_multi_dossiers_tous_rendus_dans_l_ordre() {
        let installes = vec![present("Pack", "Pack"), present("Pack_Options", "Pack options"), present("Autre", "Autre")];
        let registre: BTreeMap<_, _> =
            [enregistre("pack", "Pack", Some(URL)), enregistre("pack_options", "pack_options", Some(URL)), enregistre("autre", "Autre", Some("https://l.test/Autre.zip"))].into();
        assert_eq!(dossiers_installes_de(&entree("Pack"), &installes, &registre), vec!["Pack", "Pack_Options"], "casse du disque, autre addon exclu");
    }

    #[test]
    fn reconnu_par_titre_seul() {
        let installes = vec![present("fix-item-icon", "Fix Item Icon")];
        assert_eq!(dossiers_installes_de(&entree(" fix item icon "), &installes, &BTreeMap::new()), vec!["fix-item-icon"]);
    }

    #[test]
    fn dossier_enregistre_disparu_du_disque_est_ignore() {
        let installes = vec![present("Pack", "Pack")];
        let registre: BTreeMap<_, _> = [enregistre("pack", "Pack", Some(URL)), enregistre("pack_gone", "Pack_Gone", Some(URL))].into();
        assert_eq!(dossiers_installes_de(&entree("Pack"), &installes, &registre), vec!["Pack"]);
        let vide: Vec<AddonInstalle> = Vec::new();
        assert!(dossiers_installes_de(&entree("Zzz"), &vide, &registre).is_empty(), "tout a disparu : aucun dossier");
    }

    #[test]
    fn non_installe_rend_vide() {
        let installes = vec![present("Autre", "Autre")];
        let registre: BTreeMap<_, _> = [enregistre("autre", "Autre", Some("https://l.test/Autre.zip"))].into();
        assert!(dossiers_installes_de(&entree("Pack"), &installes, &registre).is_empty());
    }

    fn avec_deps(dossier: &str, deps: &[&str]) -> AddonInstalle {
        let mut a = present(dossier, dossier);
        a.fiche.dependances = deps.iter().map(|d| d.to_string()).collect();
        a
    }

    fn modules_de(liste: &[AddonListe], dossier: &str) -> Vec<String> {
        liste.iter().find(|g| g.dossier == dossier).map(|g| g.modules.iter().map(|m| m.dossier.clone()).collect()).unwrap_or_default()
    }

    #[test]
    fn weakauras_trois_modules_par_le_toc() {
        let installes = vec![
            present("WeakAuras", "WeakAuras"),
            avec_deps("WeakAurasArchive", &["weakauras"]),
            avec_deps("WeakAurasModelPaths", &["WeakAuras"]),
            avec_deps("WeakAurasOptions", &["WeakAuras"]),
            present("Recount", "Recount"),
        ];
        let liste = regrouper(&installes, &[]);
        assert_eq!(liste.len(), 2);
        assert_eq!(modules_de(&liste, "WeakAuras"), vec!["WeakAurasArchive", "WeakAurasModelPaths", "WeakAurasOptions"]);
    }

    #[test]
    fn cycle_aucun_des_deux_n_est_module() {
        let installes = vec![avec_deps("A", &["B"]), avec_deps("B", &["A"]), avec_deps("C", &["A"])];
        let liste = regrouper(&installes, &[]);
        assert_eq!(liste.iter().map(|g| g.dossier.as_str()).collect::<Vec<_>>(), vec!["A", "B", "C"]);
        assert!(modules_de(&liste, "A").is_empty(), "C dépend d'un membre du cycle (non racine) : ligne simple, pas de chaîne par TOC");
        assert_eq!(liste.len(), 3);
    }

    #[test]
    fn parent_non_installe_reste_ligne_simple() {
        let installes = vec![avec_deps("Option", &["Absent"]), present("Seul_Module", "x")];
        assert_eq!(regrouper(&installes, &[]).len(), 2);
    }

    #[test]
    fn chaine_par_prefixe_rattachee_a_la_racine_profondeur_un() {
        // Adapté : Feuille→Milieu par TOC n'est plus rattachée (pas de chaîne par TOC) ; le préfixe « _ » garde son comportement.
        let installes = vec![present("Racine", "R"), avec_deps("Milieu", &["Racine"]), avec_deps("Feuille", &["Milieu"]), present("Racine_Sous", "s"), present("Racine_Sous_Fin", "f")];
        let liste = regrouper(&installes, &[]);
        assert_eq!(liste.len(), 2);
        assert_eq!(modules_de(&liste, "Racine"), vec!["Milieu", "Racine_Sous", "Racine_Sous_Fin"]);
    }

    #[test]
    fn chaine_par_toc_seul_le_premier_maillon_est_module() {
        let installes = vec![present("A", "A"), avec_deps("B", &["A"]), avec_deps("C", &["B"])];
        let liste = regrouper(&installes, &[]);
        assert_eq!(modules_de(&liste, "A"), vec!["B"]);
        assert!(liste.iter().any(|g| g.dossier == "C"), "C reste ligne simple");
        assert_eq!(liste.len(), 2);
    }

    #[test]
    fn dependance_a_la_seule_bibliotheque_reste_ligne_simple() {
        let installes = vec![present("LibX", "LibX"), avec_deps("B", &["LibX"])];
        let liste = regrouper(&installes, &[]);
        assert_eq!(liste.len(), 2);
        assert!(liste.iter().all(|g| g.modules.is_empty()), "supprimer LibX n'emporterait pas B");
    }

    #[test]
    fn deux_addons_distincts_en_dependance_ligne_simple() {
        let installes = vec![present("A", "A"), present("C", "C"), avec_deps("D", &["A", "C"])];
        let liste = regrouper(&installes, &[]);
        assert_eq!(liste.len(), 3);
        assert!(liste.iter().all(|g| g.modules.is_empty()));
    }

    #[test]
    fn prefixe_prioritaire_puis_unique_dependance_installee() {
        // Adapté : Y (deux dépendances installées P et Q) n'est plus rattaché ; Z (une seule, Q) l'est.
        let installes = vec![present("P", "P"), present("Q", "Q"), avec_deps("P_x", &["Q"]), avec_deps("Y", &["Inconnu", "Q", "P"]), avec_deps("Z", &["Inconnu", "Q"])];
        let liste = regrouper(&installes, &[]);
        assert_eq!(modules_de(&liste, "P"), vec!["P_x"]);
        assert_eq!(modules_de(&liste, "Q"), vec!["Z"]);
        assert!(liste.iter().any(|g| g.dossier == "Y"));
    }

    #[test]
    fn bibliotheque_a_plusieurs_parents_reste_ligne_simple() {
        let installes = vec![present("P", "P"), present("Q", "Q"), avec_deps("LibPartagee", &["P", "Q"]), avec_deps("LibUnique", &["P"]), avec_deps("Module", &["P", "Q"])];
        let liste = regrouper(&installes, &[]);
        assert!(liste.iter().any(|g| g.dossier == "LibPartagee"));
        assert!(liste.iter().any(|g| g.dossier == "Module"), "adapté : deux parents installés, ligne simple");
        assert_eq!(modules_de(&liste, "P"), vec!["LibUnique"], "une bibliothèque à parent unique reste module comme avant");
    }

    #[test]
    fn dependances_optionnelles_ne_rattachent_pas() {
        let mut a = present("Opt", "Opt");
        a.fiche.dependances_optionnelles = vec!["P".to_string()];
        assert_eq!(regrouper(&[present("P", "P"), a], &[]).len(), 2);
    }

    #[test]
    fn suivis_comptes_en_addons() {
        let installes = vec![present("P", "P"), avec_deps("M", &["P"]), present("Z", "Z")];
        let liste = regrouper(&installes, &["P".to_string(), "M".to_string()]);
        assert_eq!(compter_suivis(&liste), 1);
        assert!(liste[0].modules[0].suivi);
    }
}
