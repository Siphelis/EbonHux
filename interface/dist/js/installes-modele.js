// Logique pure de l'onglet Installés (sans DOM).
import { CATEGORIE_TOUTES, entreesCatalogueParDossier, ficheAddon, filtrerAddons } from './modeles.js';

export const CATEGORIE_SANS = 'Sans catégorie';

/** Index `url -> entrée` du catalogue (pour les addons dont l'état garde l'adresse d'origine). */
export function entreesCatalogueParUrl(entrees) {
  const index = new Map();
  for (const entree of entrees ?? []) {
    if (typeof entree.url === 'string' && entree.url !== '' && !index.has(entree.url)) index.set(entree.url, entree);
  }
  return index;
}

/** Entrée du catalogue d'un addon installé : par dossier (ou dossiers_installes), sinon par addon_url. */
export function entreeDeLAddon(addon, indexDossier, indexUrl) {
  const parDossier = indexDossier?.get(String(addon.dossier ?? '').toLowerCase());
  if (parDossier) return parDossier;
  return (addon.addon_url && indexUrl?.get(addon.addon_url)) || null;
}

/** Catégories d'un installé : celles de son entrée du catalogue, « Sans catégorie » sinon. */
export function categoriesDeLAddon(addon, indexDossier, indexUrl) {
  const entree = entreeDeLAddon(addon, indexDossier, indexUrl);
  const liste = [...new Set((entree?.categories ?? []).filter((nom) => typeof nom === 'string' && nom !== ''))];
  return liste.length > 0 ? liste : [CATEGORIE_SANS];
}

/** Fiches (addon + entrée + catégories) triées par nom A-Z. */
export function fichesInstalles(addons, entrees, { bibliotheques = false } = {}) {
  const indexDossier = entreesCatalogueParDossier(entrees);
  const indexUrl = entreesCatalogueParUrl(entrees);
  const fiches = filtrerAddons(addons ?? [], { requete: '', bibliotheques }).map((addon) => {
    const entree = entreeDeLAddon(addon, indexDossier, indexUrl);
    const fiche = ficheAddon(addon, new Map(entree ? [[addon.dossier.toLowerCase(), entree]] : []));
    return { ...fiche, categories: categoriesDeLAddon(addon, indexDossier, indexUrl) };
  });
  return fiches.sort((a, b) => a.titre.localeCompare(b.titre, 'fr', { sensitivity: 'base' }));
}

/** [{nom, nombre}] : « Toutes », puis les catégories (plus fournie d'abord), « Sans catégorie » en dernier si présente. */
export function categoriesInstalles(fiches) {
  const comptes = new Map();
  for (const fiche of fiches) for (const nom of fiche.categories) comptes.set(nom, (comptes.get(nom) ?? 0) + 1);
  const sans = comptes.get(CATEGORIE_SANS) ?? 0;
  comptes.delete(CATEGORIE_SANS);
  const liste = [...comptes].map(([nom, nombre]) => ({ nom, nombre }));
  liste.sort((a, b) => b.nombre - a.nombre || a.nom.localeCompare(b.nom, 'fr'));
  return [{ nom: CATEGORIE_TOUTES, nombre: fiches.length }, ...liste, ...(sans > 0 ? [{ nom: CATEGORIE_SANS, nombre: sans }] : [])];
}

export function compterMisesAJourInstalles(fiches) {
  return fiches.filter((fiche) => fiche.miseAJour).length;
}

/** Filtre : catégorie, recherche (mêmes champs que l'ancien écran), mises à jour seulement. L'ordre est conservé. */
export function filtrerInstalles(fiches, { categorie = CATEGORIE_TOUTES, requete = '', majSeulement = false } = {}) {
  return fiches
    .filter((fiche) => categorie === CATEGORIE_TOUTES || fiche.categories.includes(categorie))
    .filter((fiche) => !majSeulement || fiche.miseAJour)
    .filter((fiche) => filtrerAddons([fiche.addon], { requete, bibliotheques: true }).length === 1);
}

export function ligneVersionInstalle(fiche) {
  const { addon, entree } = fiche;
  const actuelle = entree?.version_installee || addon.version;
  if (fiche.miseAJour && entree?.version && actuelle) return `Version ${actuelle} → ${entree.version}`;
  return actuelle ? `Version ${actuelle}` : '';
}

export function libelleNomInstalle(fiche) {
  return fiche.addon.version ? `${fiche.titre} ${fiche.addon.version}` : fiche.titre;
}

export function libelleEtatInstalle(fiche, enCours = false) {
  if (enCours) return 'En cours';
  return fiche.miseAJour ? 'Mise à jour' : 'À jour';
}

/** Dossiers/nom d'entrée à mettre à jour pour « Tout mettre à jour » : les noms d'entrée, sans doublon. */
export function nomsAMettreAJour(fiches) {
  return [...new Set(fiches.filter((fiche) => fiche.miseAJour && fiche.entree).map((fiche) => fiche.entree.nom))];
}

// ---- Volets de modules : un addon = une ligne, ses modules en lignes repliables ----

export const HAUTEUR_LIGNE_ADDON = 64;
export const HAUTEUR_LIGNE_MODULE = 28;

/** Comptes du sous-titre, toujours en addons (jamais en dossiers). */
export function comptesInstalles(addons) {
  const liste = addons ?? [];
  return { total: liste.length, suivis: liste.filter((addon) => addon.suivi).length };
}

export const modulesDe = (fiche) => (Array.isArray(fiche.addon.modules) ? fiche.addon.modules : []);

/** Dossiers des addons à ouvrir d'office : recherche non vide, l'addon lui-même ne correspond pas mais un de ses modules oui. */
export function dossiersOuvertsParRecherche(fiches, requete) {
  const ouverts = new Set();
  if (String(requete ?? '').trim() === '') return ouverts;
  for (const fiche of fiches) {
    if (modulesDe(fiche).length === 0) continue;
    const sansModules = { ...fiche.addon, modules: [] };
    const addonCorrespond = filtrerAddons([sansModules], { requete, bibliotheques: true }).length === 1;
    if (!addonCorrespond && filtrerAddons([fiche.addon], { requete, bibliotheques: true }).length === 1) ouverts.add(fiche.addon.dossier);
  }
  return ouverts;
}

/** Lignes à plat : une ligne d'addon, puis ses lignes de module si son volet est ouvert. `haut` = décalage cumulé. */
export function aplatirLignes(fiches, ouverts = new Set()) {
  const lignes = [];
  let haut = 0;
  fiches.forEach((fiche, rangAddon) => {
    lignes.push({ type: 'addon', fiche, rangAddon, haut, hauteur: HAUTEUR_LIGNE_ADDON });
    haut += HAUTEUR_LIGNE_ADDON;
    if (!ouverts.has(fiche.addon.dossier)) return;
    for (const module of modulesDe(fiche)) {
      lignes.push({ type: 'module', fiche, module, rangAddon, haut, hauteur: HAUTEUR_LIGNE_MODULE });
      haut += HAUTEUR_LIGNE_MODULE;
    }
  });
  return { lignes, hauteurTotale: haut };
}

/** Index de la ligne contenant le décalage `y` (recherche dichotomique), borné. */
export function indexLigneA(lignes, y) {
  if (lignes.length === 0) return 0;
  let bas = 0;
  let haut = lignes.length - 1;
  while (bas < haut) {
    const milieu = (bas + haut + 1) >> 1;
    if (lignes[milieu].haut <= y) bas = milieu;
    else haut = milieu - 1;
  }
  return bas;
}

/** Fenêtrage à hauteurs variables : [debut, fin) en lignes, avec `marge` lignes de chaque côté. */
export function plageLignes(lignes, { defilement = 0, hauteurFenetre = 0, marge = 0 } = {}) {
  if (lignes.length === 0) return { debut: 0, fin: 0 };
  const premier = indexLigneA(lignes, Math.max(0, defilement));
  const dernier = indexLigneA(lignes, Math.max(0, defilement) + Math.max(0, hauteurFenetre));
  return { debut: Math.max(0, premier - marge), fin: Math.min(lignes.length, dernier + 1 + marge) };
}

/** Rangs 1-based des addons vus (une ligne de module compte pour son addon) ; {0,0} si rien. */
export function addonsVisibles(lignes, { defilement = 0, hauteurFenetre = 0 } = {}) {
  if (lignes.length === 0) return { premiere: 0, derniere: 0 };
  const premier = indexLigneA(lignes, Math.max(0, defilement));
  const dernier = indexLigneA(lignes, Math.max(0, defilement) + Math.max(0, hauteurFenetre) - 1);
  return { premiere: lignes[premier].rangAddon + 1, derniere: lignes[Math.max(premier, dernier)].rangAddon + 1 };
}

/** Nouveau défilement pour amener la ligne `index` dans la fenêtre (le même `défilement` si déjà visible). */
export function defilementPourVoir(lignes, index, { defilement, hauteurFenetre, decalageHaut = 0, ecart = 8 }) {
  const ligne = lignes[index];
  if (!ligne) return defilement;
  const haut = decalageHaut + ligne.haut;
  const bas = haut + ligne.hauteur - ecart;
  if (haut < defilement) return Math.max(0, haut - decalageHaut);
  if (bas > defilement + hauteurFenetre) return bas - hauteurFenetre;
  return defilement;
}

export function libelleSousTitreLigne(description, nombreModules) {
  if (nombreModules <= 0) return description;
  const modules = `${nombreModules} ${nombreModules === 1 ? 'module' : 'modules'}`;
  return description ? `${description} · ${modules}` : modules;
}
