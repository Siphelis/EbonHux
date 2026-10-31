//! Port réseau : télécharger une URL vers un fichier, avec limite de taille et somme SHA256.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::http::{StatusCode, Uri};

use crate::erreur::{Erreur, Result};

pub const DELAI_CONNEXION: Duration = Duration::from_secs(15);
pub const DELAI_GLOBAL: Duration = Duration::from_secs(900);
pub const REDIRECTIONS_MAX: usize = 5;
pub const SUFFIXE_PARTIEL: &str = "part";
const TAILLE_BLOC: usize = 64 * 1024;
const AGENT_UTILISATEUR: &str = concat!("ebonhux/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Telechargement {
    pub octets: u64,
    /// Hexadécimal minuscule.
    pub sha256: String,
}

pub trait Telechargeur {
    fn telecharger(&self, url: &str, destination: &Path, limite_octets: u64) -> Result<Telechargement>;
}

pub fn somme_sha256(octets: &[u8]) -> String {
    hexa(&Sha256::digest(octets))
}

fn hexa(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

fn erreur_reseau(url: &str, raison: impl std::fmt::Display) -> Erreur {
    Erreur::Reseau {
        url: url.to_string(),
        raison: raison.to_string(),
    }
}

fn chemin_partiel(destination: &Path) -> PathBuf {
    let mut nom: OsString = destination.as_os_str().to_owned();
    nom.push(".");
    nom.push(SUFFIXE_PARTIEL);
    PathBuf::from(nom)
}

/// Copie `lecteur` vers `<destination>.part` en comptant et en hachant, puis renomme.
/// Le `.part` créé ici est retiré à tout échec ; la destination n'existe alors pas.
fn ecrire_flux(
    url: &str,
    lecteur: &mut dyn Read,
    longueur_annoncee: Option<u64>,
    destination: &Path,
    limite_octets: u64,
) -> Result<Telechargement> {
    let partiel = chemin_partiel(destination);
    let mut fichier = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partiel)
        .map_err(|e| Erreur::io("créer le fichier temporaire", &partiel, e))?;
    let resultat = copier(url, lecteur, longueur_annoncee, &mut fichier, &partiel, limite_octets)
        .and_then(|bilan| {
            fichier
                .sync_all()
                .map_err(|e| Erreur::io("synchroniser le fichier temporaire", &partiel, e))?;
            drop(fichier);
            fs::rename(&partiel, destination)
                .map_err(|e| Erreur::io("renommer le fichier téléchargé", destination, e))?;
            Ok(bilan)
        });
    match resultat {
        Ok(bilan) => Ok(bilan),
        Err(erreur) => match fs::remove_file(&partiel) {
            Ok(()) => Err(erreur),
            Err(e) if e.kind() == ErrorKind::NotFound => Err(erreur),
            Err(e) => Err(erreur_reseau(
                url,
                format!(
                    "{erreur} ; en plus, le fichier temporaire « {} » n'a pas pu être retiré : {e}",
                    partiel.display()
                ),
            )),
        },
    }
}

fn copier(
    url: &str,
    lecteur: &mut dyn Read,
    longueur_annoncee: Option<u64>,
    fichier: &mut File,
    partiel: &Path,
    limite_octets: u64,
) -> Result<Telechargement> {
    let mut hachage = Sha256::new();
    let mut total: u64 = 0;
    let mut bloc = vec![0u8; TAILLE_BLOC];
    loop {
        let lus = match lecteur.read(&mut bloc) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(erreur_lecture(url, total, &e)),
        };
        total += lus as u64;
        if total > limite_octets {
            return Err(erreur_reseau(
                url,
                format!("taille maximale dépassée en cours de réception (limite : {limite_octets} octets)"),
            ));
        }
        hachage.update(&bloc[..lus]);
        fichier
            .write_all(&bloc[..lus])
            .map_err(|e| Erreur::io("écrire le fichier temporaire", partiel, e))?;
    }
    if let Some(annoncee) = longueur_annoncee
        && annoncee != total
    {
        return Err(erreur_reseau(
            url,
            format!("connexion coupée : {total} octets reçus sur {annoncee} annoncés"),
        ));
    }
    Ok(Telechargement {
        octets: total,
        sha256: hexa(&hachage.finalize()),
    })
}

fn erreur_lecture(url: &str, recus: u64, erreur: &std::io::Error) -> Erreur {
    let delai = erreur.kind() == ErrorKind::TimedOut
        || erreur
            .get_ref()
            .and_then(|interne| interne.downcast_ref::<ureq::Error>())
            .is_some_and(|e| matches!(e, ureq::Error::Timeout(_)));
    if delai {
        erreur_reseau(url, format!("délai dépassé après {recus} octets reçus"))
    } else {
        erreur_reseau(url, format!("connexion coupée après {recus} octets reçus : {erreur}"))
    }
}

fn erreur_ureq(url: &str, erreur: &ureq::Error) -> Erreur {
    match erreur {
        ureq::Error::Timeout(quel) => erreur_reseau(url, format!("délai dépassé ({quel})")),
        autre => erreur_reseau(url, format!("échec de la requête : {autre}")),
    }
}

fn est_boucle_locale(hote: &str) -> bool {
    let nu = hote.trim_start_matches('[').trim_end_matches(']');
    nu.parse::<IpAddr>().is_ok_and(|adresse| adresse.is_loopback())
}

/// Valide l'URL d'un saut : https, ou http vers une boucle locale si le mode d'essai l'autorise.
fn valider_url(url: &str, http_boucle_locale: bool) -> Result<Uri> {
    let uri: Uri = url
        .parse()
        .map_err(|e| erreur_reseau(url, format!("URL invalide : {e}")))?;
    let hote = uri
        .host()
        .ok_or_else(|| erreur_reseau(url, "URL invalide : aucun nom d'hôte"))?;
    match uri.scheme_str() {
        Some("https") => Ok(uri),
        Some("http") if http_boucle_locale && est_boucle_locale(hote) => Ok(uri),
        Some("http") => Err(erreur_reseau(
            url,
            "connexion non chiffrée refusée : seul https est accepté",
        )),
        Some(autre) => Err(erreur_reseau(url, format!("schéma « {autre} » refusé : seul https est accepté"))),
        None => Err(erreur_reseau(url, "URL invalide : schéma absent")),
    }
}

fn resoudre_redirection(base: &Uri, location: &str) -> String {
    if location.contains("://") {
        return location.to_string();
    }
    let schema = base.scheme_str().unwrap_or("https");
    let autorite = base.authority().map(|a| a.as_str()).unwrap_or_default();
    if let Some(suite) = location.strip_prefix("//") {
        return format!("{schema}://{suite}");
    }
    if location.starts_with('/') {
        return format!("{schema}://{autorite}{location}");
    }
    let chemin = base.path();
    let dossier = chemin.rsplit_once('/').map_or("", |(tete, _)| tete);
    format!("{schema}://{autorite}{dossier}/{location}")
}

pub struct TelechargeurReel {
    http_boucle_locale: bool,
    delai_connexion: Duration,
    delai_global: Duration,
}

impl Default for TelechargeurReel {
    fn default() -> Self {
        Self::new()
    }
}

impl TelechargeurReel {
    /// Mode normal : https uniquement, à chaque saut de redirection.
    pub fn new() -> Self {
        Self {
            http_boucle_locale: false,
            delai_connexion: DELAI_CONNEXION,
            delai_global: DELAI_GLOBAL,
        }
    }

    /// POUR LES ESSAIS : autorise `http://` uniquement vers les adresses de boucle locale
    /// (127.0.0.0/8 et ::1), au premier appel comme à chaque redirection. Toute autre
    /// adresse en `http://` reste refusée.
    pub fn avec_boucle_locale_http() -> Self {
        Self {
            http_boucle_locale: true,
            ..Self::new()
        }
    }

    pub fn avec_delais(mut self, connexion: Duration, global: Duration) -> Self {
        self.delai_connexion = connexion;
        self.delai_global = global;
        self
    }

    fn agent(&self) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .https_only(!self.http_boucle_locale)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(self.delai_connexion))
            .timeout_global(Some(self.delai_global))
            .user_agent(AGENT_UTILISATEUR)
            .build();
        ureq::Agent::new_with_config(config)
    }
}

impl Telechargeur for TelechargeurReel {
    fn telecharger(&self, url: &str, destination: &Path, limite_octets: u64) -> Result<Telechargement> {
        let mut courante = url.to_string();
        let mut uri = valider_url(&courante, self.http_boucle_locale)?;
        let agent = self.agent();
        let mut redirections = 0;
        loop {
            let reponse = agent
                .get(&courante)
                .call()
                .map_err(|e| erreur_ureq(&courante, &e))?;
            let statut = reponse.status();
            if est_redirection(statut) {
                let location = reponse
                    .headers()
                    .get("location")
                    .ok_or_else(|| erreur_reseau(&courante, format!("redirection (code HTTP {statut}) sans en-tête Location")))?
                    .to_str()
                    .map_err(|_| erreur_reseau(&courante, "en-tête Location illisible"))?
                    .to_string();
                if redirections >= REDIRECTIONS_MAX {
                    return Err(erreur_reseau(
                        url,
                        format!("trop de redirections (plus de {REDIRECTIONS_MAX})"),
                    ));
                }
                redirections += 1;
                let suivante = resoudre_redirection(&uri, &location);
                uri = valider_url(&suivante, self.http_boucle_locale).map_err(|e| match e {
                    Erreur::Reseau { raison, .. } => {
                        erreur_reseau(&courante, format!("redirection refusée vers « {suivante} » : {raison}"))
                    }
                    autre => autre,
                })?;
                courante = suivante;
                continue;
            }
            if !statut.is_success() {
                return Err(erreur_reseau(&courante, format!("code HTTP {}", statut.as_u16())));
            }
            let longueur = longueur_annoncee(&courante, &reponse)?;
            if let Some(annoncee) = longueur
                && annoncee > limite_octets
            {
                return Err(erreur_reseau(
                    &courante,
                    format!("taille annoncée ({annoncee} octets) supérieure à la limite ({limite_octets} octets)"),
                ));
            }
            let mut corps = reponse.into_body();
            let mut lecteur = corps.as_reader();
            return ecrire_flux(&courante, &mut lecteur, longueur, destination, limite_octets);
        }
    }
}

fn est_redirection(statut: StatusCode) -> bool {
    matches!(statut.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn longueur_annoncee(url: &str, reponse: &ureq::http::Response<ureq::Body>) -> Result<Option<u64>> {
    let Some(valeur) = reponse.headers().get("content-length") else {
        return Ok(None);
    };
    valeur
        .to_str()
        .ok()
        .and_then(|texte| texte.trim().parse::<u64>().ok())
        .map(Some)
        .ok_or_else(|| erreur_reseau(url, "en-tête Content-Length invalide"))
}

#[derive(Debug, Clone)]
enum Reponse {
    Octets(Vec<u8>),
    Echec(String),
}

/// Réponses programmées par URL ; une URL non programmée est une erreur explicite.
#[derive(Default)]
pub struct TelechargeurSimule {
    reponses: Mutex<HashMap<String, Reponse>>,
    appels: Mutex<Vec<String>>,
}

fn verrouiller<T>(verrou: &Mutex<T>) -> MutexGuard<'_, T> {
    verrou.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TelechargeurSimule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn programmer_octets(&self, url: &str, octets: Vec<u8>) {
        verrouiller(&self.reponses).insert(url.to_string(), Reponse::Octets(octets));
    }

    pub fn programmer_erreur(&self, url: &str, raison: &str) {
        verrouiller(&self.reponses).insert(url.to_string(), Reponse::Echec(raison.to_string()));
    }

    /// URL demandées, dans l'ordre, y compris celles qui n'étaient pas programmées.
    pub fn appels(&self) -> Vec<String> {
        verrouiller(&self.appels).clone()
    }
}

impl Telechargeur for TelechargeurSimule {
    fn telecharger(&self, url: &str, destination: &Path, limite_octets: u64) -> Result<Telechargement> {
        verrouiller(&self.appels).push(url.to_string());
        let reponse = verrouiller(&self.reponses).get(url).cloned();
        match reponse {
            None => Err(erreur_reseau(url, "URL non programmée dans le téléchargeur simulé")),
            Some(Reponse::Echec(raison)) => Err(erreur_reseau(url, raison)),
            Some(Reponse::Octets(octets)) => {
                let longueur = octets.len() as u64;
                if longueur > limite_octets {
                    return Err(erreur_reseau(
                        url,
                        format!("taille annoncée ({longueur} octets) supérieure à la limite ({limite_octets} octets)"),
                    ));
                }
                ecrire_flux(url, &mut octets.as_slice(), Some(longueur), destination, limite_octets)
            }
        }
    }
}
