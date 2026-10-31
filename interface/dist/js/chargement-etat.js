// Logique pure de l'écran de chargement (sans DOM, importable par node).

export const ETAPE_CONNEXION = 'Connexion au serveur';
export const ETAPE_TELECHARGEMENT = 'Téléchargement du catalogue';
export const ETAPE_LECTURE = 'Lecture des addons';

// Pourcentage entier 0-100, ou null si les nombres ne permettent pas un vrai pourcentage.
export function pourcentReel(octets, total) {
  if (typeof octets !== 'number' || typeof total !== 'number') return null;
  if (!Number.isFinite(octets) || !Number.isFinite(total)) return null;
  if (total <= 0 || octets < 0 || octets > total) return null;
  return Math.round((octets / total) * 100);
}

// Traduit une lecture de progression en étape + pourcentage (jamais inventé).
export function etatDeProgression(lecture) {
  // Le serveur n'annonce aucune taille : sans lecture exploitable, on est en plein téléchargement.
  if (!lecture || typeof lecture !== 'object') return { etape: ETAPE_TELECHARGEMENT, pourcent: null };
  if (!lecture.actif) return { etape: ETAPE_CONNEXION, pourcent: null };
  const pourcent = pourcentReel(lecture.octets, lecture.total);
  if (lecture.termine) return { etape: ETAPE_LECTURE, pourcent: pourcent === 100 ? 100 : null };
  return { etape: ETAPE_TELECHARGEMENT, pourcent };
}

// Boucle de suivi : un seul appel en vol, erreurs ignorées, réponse tardive ignorée après l'arrêt.
// Retourne la fonction d'arrêt (idempotente).
export function suivreProgression(lireProgression, appliquer, { intervalle = 150 } = {}) {
  let arrete = false;
  let minuterie = null;
  appliquer(etatDeProgression(null));

  const tourner = async () => {
    minuterie = null;
    if (arrete) return;
    try {
      const lecture = await lireProgression();
      if (!arrete) appliquer(etatDeProgression(lecture));
    } catch {
      // Erreur de lecture : on garde le dernier état affiché.
    }
    if (!arrete) minuterie = setTimeout(tourner, intervalle);
  };
  tourner();

  return () => {
    arrete = true;
    if (minuterie !== null) clearTimeout(minuterie);
    minuterie = null;
  };
}

// ---- Estimation du pourcentage ----
// ATTENTION : le nombre produit ici est une ESTIMATION, pas une mesure. Le serveur n'annonce aucune
// taille ; la courbe montre seulement qu'il y a une évolution. Elle ralentit et plafonne à 95 tant
// que les données ne sont pas arrivées (le 100 vient du réel, jamais de la courbe).
// L'absence de signe « ≈ » à l'affichage est un choix du propriétaire (« 42% ça suffit ») :
// l'honnêteté passe par ce commentaire, pas par l'écran.

export const ATTENDU_DEFAUT_MS = 3000;
export const CLE_DUREE = 'ebonhux.catalogue.duree-ms';
const DUREE_MIN_MS = 200;
const DUREE_MAX_MS = 60000;
const PLAFOND_ESTIME = 95;

const dureeValide = (ms) => typeof ms === 'number' && Number.isFinite(ms) && ms >= DUREE_MIN_MS && ms <= DUREE_MAX_MS;

// Courbe min(95, floor(95 * (1 - exp(-t/tau)))), tau = attendu/3 (~90 % à t = attendu).
// t = 0 au premier appel de pourcent() (ou à la création si l'horloge répond). Jamais de recul.
export function creerEstimation({ attenduMs, maintenant = () => Date.now() } = {}) {
  const attendu = dureeValide(attenduMs) ? attenduMs : ATTENDU_DEFAUT_MS;
  const tau = attendu / 3;
  const debut = maintenant();
  let dernier = 0;
  return {
    pourcent() {
      const t = Math.max(0, maintenant() - debut);
      const p = Math.min(PLAFOND_ESTIME, Math.floor(PLAFOND_ESTIME * (1 - Math.exp(-t / tau))));
      if (p > dernier) dernier = p;
      return dernier;
    },
  };
}

// Durée attendue gardée par le stockage (localStorage injecté) ; défaut si absente/invalide/inaccessible.
export function lireDuree(stockage) {
  try {
    const brut = stockage.getItem(CLE_DUREE);
    if (typeof brut !== 'string' || brut.trim() === '') return ATTENDU_DEFAUT_MS;
    const ms = Number(brut);
    return dureeValide(ms) ? ms : ATTENDU_DEFAUT_MS;
  } catch {
    return ATTENDU_DEFAUT_MS;
  }
}

// Garde la durée mesurée ; refuse l'absurde. Retourne true si écrit.
export function ecrireDuree(stockage, ms) {
  if (!dureeValide(ms)) return false;
  try {
    stockage.setItem(CLE_DUREE, String(Math.round(ms)));
    return true;
  } catch {
    return false;
  }
}

// Texte affiché : « 42 % » (U+00A0 avant %), identique pour l'estimation et le réel.
// Pas de « ≈ » : choix du propriétaire, voir plus haut ; le nombre estimé reste une estimation.
export function libelleNombre(pourcent) {
  return `${pourcent} %`;
}

// Le pourcentage RÉEL prime sur l'estimation dès qu'il existe (nombre fini).
export function pourcentAffiche(reel, estime) {
  if (typeof reel === 'number' && Number.isFinite(reel)) return reel;
  if (typeof estime === 'number' && Number.isFinite(estime)) return estime;
  return null;
}

// Le loader du Catalogue doit rester visible TANT QU'IL N'Y A RIEN À MONTRER.
//  - du contenu (au moins une entrée, cache compris) : jamais de loader ;
//  - une erreur sans donnée : pas de loader (l'erreur est affichée) ;
//  - lecture du cache en cours (courant === null, sans erreur) : loader ;
//  - téléchargement avec catalogue vide : loader ;
//  - catalogue lu avec 0 entrée et pas de téléchargement : pas de loader (message « catalogue vide »).
export function doitAfficherChargement({ courant, telechargement = false, erreur = null } = {}) {
  if (courant && Array.isArray(courant.entrees) && courant.entrees.length > 0) return false;
  if (erreur) return false;
  if (!courant) return true;
  return Boolean(telechargement);
}
