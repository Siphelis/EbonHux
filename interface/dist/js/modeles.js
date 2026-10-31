export const VERSION_CLIENT = 'Client 3.3.5a · v1.0.0';
export const DUREE_LANCEMENT_S = 25;
export const INTERVALLE_ACTIF_MS = 1000;
export const INTERVALLE_REPOS_MS = 5000;

const ESPACE_INSECABLE = ' ';
const ETATS_LANCES = ['lancement', 'en_jeu'];
const ETATS_CONNECTES = ['connecte', 'lancement', 'en_jeu'];
const ETATS_SANS_MOT_DE_PASSE = ['deconnecte', 'mot_de_passe_demande'];

const LIBELLES_ETAT = {
  deconnecte: 'Déconnecté',
  connecte: 'Connecté',
  mot_de_passe_demande: 'Mot de passe demandé',
  lancement: 'Lancement…',
  en_jeu: 'En jeu',
};

function pluriel(nombre, singulier, plurielForme) {
  return nombre > 1 ? plurielForme : singulier;
}

export function estLance(compte) {
  return ETATS_LANCES.includes(compte.etat);
}

export function compterLances(comptes) {
  return comptes.filter(estLance).length;
}

export function compterConnectes(comptes) {
  return comptes.filter((compte) => ETATS_CONNECTES.includes(compte.etat)).length;
}

export function initialeDe(nom) {
  return nom.trim().charAt(0).toUpperCase();
}

export function libelleEtat(etat) {
  return LIBELLES_ETAT[etat] ?? etat;
}

export function libelleEnJeu(secondes) {
  const minutes = Math.floor(secondes / 60);
  if (minutes < 1) return "En jeu depuis moins d'une minute";
  if (minutes < 60) return `En jeu depuis ${minutes} min`;
  const reste = String(minutes % 60).padStart(2, '0');
  return `En jeu depuis ${Math.floor(minutes / 60)} h ${reste} min`;
}

export function libelleLancement(progression) {
  return `Lancement…${ESPACE_INSECABLE}${progression ?? 0}${ESPACE_INSECABLE}%`;
}

export function libelleEtatCourt(compte) {
  if (compte.etat === 'en_jeu') return `En jeu · ${Math.floor((compte.en_jeu_depuis_s ?? 0) / 60)} min`;
  return libelleEtat(compte.etat);
}

export function libelleSousLigne(compte) {
  if (compte.etat === 'en_jeu') return libelleEnJeu(compte.en_jeu_depuis_s ?? 0);
  if (compte.etat === 'lancement') return libelleLancement(compte.progression);
  if (compte.etat === 'connecte') return 'Mot de passe enregistré';
  if (compte.etat === 'deconnecte') return 'À saisir au prochain lancement';
  return 'Demandé à chaque lancement';
}

export function libelleToutLancer(nombre) {
  return nombre === 1 ? 'Jouer' : `Tout lancer (${nombre})`;
}

export function libelleMotsDePasse(nombre) {
  if (nombre < 1) return '';
  const verbe = pluriel(nombre, 'sera demandé', 'seront demandés');
  return `${nombre} ${pluriel(nombre, 'mot de passe', 'mots de passe')} ${verbe}`;
}

export function libelleSelection(selectionnes, total) {
  return `${selectionnes} sur ${total} ${pluriel(total, 'sélectionné', 'sélectionnés')}`;
}

export function libelleResume(comptes) {
  if (comptes.length === 0) return 'Aucun compte';
  const connectes = compterConnectes(comptes);
  return `${comptes.length} ${pluriel(comptes.length, 'compte', 'comptes')} · ${connectes} ${pluriel(connectes, 'connecté', 'connectés')}`;
}

export function libelleAddons(suivis) {
  if (!suivis) return 'Aucun suivi';
  return `${suivis} ${pluriel(suivis, 'suivi', 'suivis')}`;
}

export function libelleDerniereConnexion(iso, maintenant) {
  if (!iso) return 'Jamais';
  const date = new Date(iso);
  const heure = `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
  const jour = (valeur) => new Date(valeur.getFullYear(), valeur.getMonth(), valeur.getDate());
  const ecart = Math.round((jour(maintenant) - jour(date)) / 86400000);
  if (ecart === 0) return `aujourd'hui, ${heure}`;
  if (ecart === 1) return `hier, ${heure}`;
  const jj = String(date.getDate()).padStart(2, '0');
  const mm = String(date.getMonth() + 1).padStart(2, '0');
  return `${jj}/${mm}/${date.getFullYear()}, ${heure}`;
}

export function boutonsCompte(compte) {
  const lance = estLance(compte);
  const peutSeDeconnecter = compte.etat === 'connecte' || lance;
  return {
    lancer: { libelle: lance ? libelleEtat(compte.etat) : 'Lancer', actif: !lance, forme: peutSeDeconnecter ? 'icone' : 'texte' },
    deconnecter: { libelle: 'Se déconnecter', visible: peutSeDeconnecter, actif: peutSeDeconnecter },
    changerMotDePasse: { libelle: 'Changer le mot de passe', visible: Boolean(compte.enregistrer_mot_de_passe && compte.mot_de_passe_enregistre) },
  };
}

export function selectionnes(comptes, deselectionnes) {
  return comptes.filter((compte) => !deselectionnes.has(compte.nom));
}

export function aLancer(comptes, deselectionnes) {
  return selectionnes(comptes, deselectionnes).filter((compte) => !estLance(compte));
}

export function motsDePasseManquants(comptes) {
  return comptes.filter((compte) => ETATS_SANS_MOT_DE_PASSE.includes(compte.etat) && !compte.mot_de_passe_enregistre);
}

export function boutonJouer(comptes, deselectionnes, installation = null) {
  if (preparationRequise(installation)) return { libelle: LIBELLE_TERMINER_PREPARATION, actif: true, action: 'preparation' };
  if (comptes.length === 0) return { libelle: 'Ajouter un compte', actif: true, action: 'ajouter' };
  if (comptes.length === 1) {
    const compte = comptes[0];
    if (compte.etat === 'en_jeu') return { libelle: 'EN JEU', actif: false, action: 'aucune', point: true };
    if (compte.etat === 'lancement') return { libelle: 'LANCEMENT…', actif: false, action: 'aucune', point: true };
    return { libelle: 'JOUER', actif: true, action: 'lancer' };
  }
  const choisis = selectionnes(comptes, deselectionnes);
  const lanceables = aLancer(comptes, deselectionnes);
  if (choisis.length === 1) return { libelle: libelleToutLancer(1).toUpperCase(), actif: lanceables.length > 0, action: 'lancer', compte: choisis[0] };
  return { libelle: libelleToutLancer(choisis.length).toUpperCase(), actif: lanceables.length > 0, action: 'tout_lancer' };
}

export function pastille(comptes, enJeu) {
  if (comptes.length === 0) return { type: 'aucun', texte: 'Aucun compte' };
  if (comptes.length === 1) {
    const compte = comptes[0];
    return { type: 'un', nom: compte.nom, initiale: initialeDe(compte.nom), verrouFerme: Boolean(compte.mot_de_passe_enregistre), enJeu: estLance(compte) };
  }
  const actifs = comptes.filter(estLance);
  const misEnAvant = [...actifs, ...comptes.filter((compte) => !estLance(compte))].slice(0, 2);
  return {
    type: 'plusieurs',
    initiales: misEnAvant.map((compte) => initialeDe(compte.nom)),
    texte: enJeu > 0 ? `${enJeu} en jeu` : 'Aucun en jeu',
  };
}

export function intervalleInterrogation(comptes) {
  return compterLances(comptes) > 0 ? INTERVALLE_ACTIF_MS : INTERVALLE_REPOS_MS;
}

export function noteCompte(compte) {
  if (compte.etat === 'mot_de_passe_demande' || compte.etat === 'deconnecte') {
    return { texte: "Active l'interrupteur pour rester connecté et ne plus ressaisir le mot de passe.", verrouFerme: true };
  }
  return { texte: "Se déconnecter oublie le mot de passe enregistré ; l'interrupteur décide s'il est enregistré.", verrouFerme: false };
}

export function libelleDeconnexion(compte) {
  const reste = compte.enregistrer_mot_de_passe
    ? " L'interrupteur « Enregistrer le mot de passe » reste activé : il sera redemandé au prochain lancement, puis enregistré de nouveau."
    : '';
  let precision = null;
  if (compte.etat === 'en_jeu') precision = `${compte.nom} est en jeu : la partie en cours n'est pas interrompue.`;
  if (compte.etat === 'lancement') precision = `${compte.nom} est en cours de lancement : le lancement n'est pas interrompu.`;
  return { titre: `Se déconnecter d'${compte.nom} ?`, texte: `Le mot de passe enregistré sera oublié.${reste}`, precision };
}

export function libelleSuppression(nom, modules = 0) {
  if (modules > 0) return { titre: `Supprimer ${nom} et ${modules === 1 ? 'son module' : `ses ${modules} modules`} ?`, confirmer: 'Supprimer' };
  return { titre: `Supprimer ${nom} ?`, confirmer: 'Supprimer' };
}

// Logique pure de la confirmation : annuler n'appelle rien, valider appelle une seule fois.
export function demandeSuppression(nom, appeler, modules = 0) {
  const { titre, confirmer } = libelleSuppression(nom, modules);
  return { titre, confirmer, annuler: () => {}, valider: () => appeler() };
}

export function libelleMotDePasseChange(compte) {
  return `Mot de passe changé pour « ${compte.nom} ».`;
}

export function libelleDialogueMotDePasse(compte, modeConnexion) {
  if (modeConnexion === 'changer') {
    return { titre: `Changer le mot de passe de ${compte.nom}`, sousTitre: 'Saisis le nouveau mot de passe.', action: 'Enregistrer' };
  }
  return {
    titre: `Mot de passe de ${compte.nom}`,
    sousTitre: modeConnexion ? 'Saisis-le pour connecter ce compte.' : 'Saisis-le pour lancer ce compte.',
    action: modeConnexion ? 'Se connecter' : 'Lancer',
  };
}

export function libelleDialogueMotsDePasse(manquants) {
  if (manquants.length === 1) {
    return { titre: `Mot de passe de ${manquants[0].nom}`, sousTitre: 'Saisis-le pour lancer ce compte.' };
  }
  return { titre: 'Mots de passe à saisir', sousTitre: `${manquants.length} comptes ont besoin de leur mot de passe pour être lancés.` };
}

const SEPARATEUR = ' · ';
const REGEX_CODES_JEU = /\|\||\|c[0-9a-f]{8}|\|r|\|n|\|T[^|]*\|t|\|H[^|]*\|h|\|h/gi;
const REGEX_ACCENTS = /[̀-ͯ]/g;
const HEURE_INCONNUE = '--:--:--';
const LIBELLE_COMMUN = 'Commun';

const LIBELLES_LANGUE = {
  protege: 'Le client est en français et protégé.',
  incomplet: 'Certains éléments de la langue française sont absents ou différents.',
  absent: 'Les fichiers de la langue française sont introuvables.',
};

const SOUS_TITRES_LANGUE = {
  protege: 'Le jeu reste en français : les éléments ci-dessous sont protégés contre les mises à jour.',
  incomplet: 'Certains éléments ci-dessous sont absents ou à vérifier.',
  absent: "Les éléments ci-dessous n'ont pas été trouvés dans le dossier du jeu.",
};

const LIBELLES_RESULTAT = { ok: 'OK', avertissement: 'Avertissement', erreur: 'Erreur', attente: 'En attente' };

const ETIQUETTES_SYSTEME = [
  ['wine', 'Wine'],
  ['vulkan', 'Vulkan'],
  ['pilote', 'Pilote graphique'],
  ['fichiers_ouverts', 'Fichiers ouverts'],
  ['session', 'Session'],
];

export const MESSAGE_CATALOGUE_VIDE = "Le catalogue est vide : aucun addon communautaire n'est encore référencé.";

function remplacerCodeJeu(code) {
  if (code === '||') return '|';
  if (code.toLowerCase() === '|n') return ' ';
  return '';
}

export function nettoyerTexteJeu(texte) {
  return String(texte ?? '').replace(REGEX_CODES_JEU, remplacerCodeJeu).replace(/\s+/g, ' ').trim();
}

export function normaliserRecherche(texte) {
  return String(texte ?? '').normalize('NFD').replace(REGEX_ACCENTS, '').toLowerCase();
}

export function correspondRecherche(champs, requete) {
  const mots = normaliserRecherche(requete).split(/\s+/).filter(Boolean);
  if (mots.length === 0) return true;
  const botte = normaliserRecherche(champs.join(' '));
  return mots.every((mot) => botte.includes(mot));
}

export function titreAddon(addon) {
  return nettoyerTexteJeu(addon.titre) || addon.dossier;
}

export function filtrerAddons(addons, { requete = '', bibliotheques = false } = {}) {
  return addons
    .filter((addon) => bibliotheques || !addon.bibliotheque)
    .filter((addon) => correspondRecherche(
      [titreAddon(addon), addon.dossier, addon.auteur, nettoyerTexteJeu(addon.notes), ...addon.modules.flatMap((module) => [module.dossier, nettoyerTexteJeu(module.titre)])],
      requete,
    ));
}

export function libelleAddonsInstalles(total, suivis) {
  return `${total} ${pluriel(total, 'addon installé', 'addons installés')}${SEPARATEUR}${suivis} ${pluriel(suivis, 'suivi', 'suivis')}`;
}

export function libellePiedAddons(suivis, total) {
  return `${suivis} ${pluriel(suivis, 'suivi', 'suivis')} sur ${total}`;
}

export function libelleModules(nombre) {
  return `${nombre} ${pluriel(nombre, 'module', 'modules')}`;
}

export function etatCaseParent(addon) {
  const suivis = addon.modules.filter((module) => module.suivi).length;
  if (addon.suivi && suivis === addon.modules.length) return 'coche';
  if (!addon.suivi && suivis === 0) return 'vide';
  return 'partiel';
}

export function libelleProblemes(problemes) {
  if (problemes.length === 0) return '';
  const detail = problemes.map((probleme) => `${probleme.dossier} (${probleme.raison})`).join(SEPARATEUR);
  const debut = problemes.length === 1 ? "1 dossier n'a pas pu être lu" : `${problemes.length} dossiers n'ont pas pu être lus`;
  return `${debut} : ${detail}`;
}

export function messageAddonsVide(nombreInstalles, nombreVisibles, requete) {
  if (nombreInstalles === 0) return 'Aucun addon installé dans le dossier du jeu.';
  if (nombreVisibles > 0) return '';
  if (requete.trim()) return `Aucun addon ne correspond à « ${requete.trim()} ».`;
  return 'Seules des bibliothèques sont installées : active « Afficher les bibliothèques » pour les voir.';
}

export function filtrerCatalogue(entrees, requete = '') {
  return entrees.filter((entree) => correspondRecherche([entree.nom, entree.description, entree.source, entree.dossier], requete));
}

export function messageCatalogueVide(nombreEntrees, nombreVisibles, requete) {
  if (nombreEntrees === 0) return MESSAGE_CATALOGUE_VIDE;
  if (nombreVisibles > 0) return '';
  return `Aucun addon du catalogue ne correspond à « ${requete.trim()} ».`;
}

export const CATEGORIE_TOUTES = 'Toutes';

/** Transition pure : clic sur le compteur de mises à jour. Activer remet la catégorie sur « Toutes » ; désactiver ne touche à rien. */
export function basculerMisesAJour({ categorie = CATEGORIE_TOUTES, majSeulement = false } = {}) {
  return majSeulement ? { categorie, majSeulement: false } : { categorie: CATEGORIE_TOUTES, majSeulement: true };
}

/** Transition pure : clic sur une pastille. Recliquer la catégorie active (hors « Toutes ») revient sur « Toutes ». */
export function choisirCategorieEtat({ categorie = CATEGORIE_TOUTES, majSeulement = false } = {}, nom) {
  return { categorie: nom === categorie && nom !== CATEGORIE_TOUTES ? CATEGORIE_TOUTES : nom, majSeulement };
}

function uniquesParUrl(entrees) {
  const vues = new Set();
  return entrees.filter((entree) => {
    if (vues.has(entree.url)) return false;
    vues.add(entree.url);
    return true;
  });
}

/** [{nom, nombre}] : « Toutes » (addons uniques par url) puis les vraies catégories, du plus fourni au moins fourni. */
export function categoriesDuCatalogue(entrees) {
  const uniques = uniquesParUrl(entrees);
  const comptes = new Map();
  for (const entree of uniques) {
    for (const nom of new Set(entree.categories ?? [])) comptes.set(nom, (comptes.get(nom) ?? 0) + 1);
  }
  const categories = [...comptes].map(([nom, nombre]) => ({ nom, nombre }));
  categories.sort((a, b) => b.nombre - a.nombre || a.nom.localeCompare(b.nom, 'fr'));
  return [{ nom: CATEGORIE_TOUTES, nombre: uniques.length }, ...categories];
}

export function entreesDeCategorie(entrees, categorie) {
  const uniques = uniquesParUrl(entrees);
  if (categorie === CATEGORIE_TOUTES) return uniques;
  return uniques.filter((entree) => (entree.categories ?? []).includes(categorie));
}

export function trierParNom(entrees) {
  return [...entrees].sort((a, b) => String(a.nom ?? '').localeCompare(String(b.nom ?? ''), 'fr', { sensitivity: 'base' }));
}

/** Description en texte seul (à poser par textContent, jamais innerHTML). */
export function texteDescription(entree) {
  const longue = typeof entree.description_longue === 'string' ? entree.description_longue : '';
  let texte = longue.trim() !== '' ? longue : String(entree.description ?? '');
  texte = texte.replace(/<br\s*\/?>/gi, '\n').replace(/<\/(p|div|li)\s*>/gi, '\n');
  let avant;
  do {
    avant = texte;
    texte = texte.replace(/<[^<>]*>/g, '');
  } while (texte !== avant);
  texte = texte
    .replace(/&nbsp;/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, '&');
  return texte.replace(/\n[ \t]*(?:\n[ \t]*)+/g, '\n\n').trim();
}

export function ligneVersionDetail(entree) {
  if (!entree.version) return '';
  if (etatCatalogue(entree) === 'maj_disponible' && entree.version_installee) {
    return `Version ${entree.version_installee} → ${entree.version}`;
  }
  return `Version ${entree.version}`;
}

export function libelleEtatLigne(entree, enCours = false) {
  if (enCours) return 'En cours';
  const etat = etatCatalogue(entree);
  if (etat === 'non_installe') return 'Non installé';
  if (etat === 'maj_disponible') return 'Mise à jour';
  return 'Installé';
}

export function libelleAffichage(fin, total) {
  if (!total) return '';
  if (fin >= total) return `Affichage 1 à ${total} sur ${total}`;
  return `Affichage 1 à ${fin} sur ${total} · défiler pour la suite`;
}

const entierSur = (v) => Number.isFinite(v);

/** Lignes à rendre (fenêtrage) : [debut, fin) avec `marge` lignes de chaque côté ; haut/bas = espaces en px. */
export function plageVisible({ defilement, hauteurFenetre, hauteurLigne, total, marge } = {}) {
  const vide = { debut: 0, fin: 0, haut: 0, bas: 0 };
  if (!entierSur(total) || total <= 0 || !entierSur(hauteurLigne) || hauteurLigne <= 0) return vide;
  const n = Math.floor(total);
  const mg = entierSur(marge) && marge > 0 ? Math.floor(marge) : 0;
  const def = entierSur(defilement) ? Math.max(0, defilement) : 0;
  const fen = entierSur(hauteurFenetre) ? Math.max(0, hauteurFenetre) : 0;
  const premier = Math.min(n - 1, Math.floor(def / hauteurLigne));
  const dernierExclu = Math.max(premier + 1, Math.ceil((def + fen) / hauteurLigne));
  const debut = Math.max(0, premier - mg);
  const fin = Math.min(n, Math.max(debut, dernierExclu + mg));
  return { debut, fin, haut: debut * hauteurLigne, bas: (n - fin) * hauteurLigne };
}

/** Rangs 1-based des lignes réellement vues (sans marge) ; {0,0} si rien. */
export function plageEntierementVisible({ defilement, hauteurFenetre, hauteurLigne, total } = {}) {
  if (!entierSur(total) || total <= 0 || !entierSur(hauteurLigne) || hauteurLigne <= 0) {
    return { premiere: 0, derniere: 0 };
  }
  const n = Math.floor(total);
  const def = entierSur(defilement) ? Math.max(0, defilement) : 0;
  const fen = entierSur(hauteurFenetre) ? Math.max(0, hauteurFenetre) : 0;
  const premier = Math.min(n - 1, Math.floor(def / hauteurLigne));
  const dernierExclu = Math.min(n, Math.max(premier + 1, Math.ceil((def + fen) / hauteurLigne)));
  return { premiere: premier + 1, derniere: dernierExclu };
}

export function filtrerMisesAJour(entrees, actif) {
  if (!actif) return entrees;
  return entrees.filter((e) => etatCatalogue(e) === 'maj_disponible');
}

export function compterMisesAJour(entrees) {
  const vues = new Set();
  let n = 0;
  for (const e of entrees) {
    if (etatCatalogue(e) !== 'maj_disponible') continue;
    if (e.url) {
      if (vues.has(e.url)) continue;
      vues.add(e.url);
    }
    n += 1;
  }
  return n;
}

export function libelleAffichagePlage(premiere, derniere, total) {
  if (!total) return '';
  return `Affichage ${premiere} à ${derniere} sur ${total}`;
}

export function libelleInstallation(nom, miseAJour = false) {
  return miseAJour ? `${nom} mis à jour.` : `${nom} installé.`;
}

/**
 * État de version d'une entrée du catalogue : 'non_installe', ou pour un addon installé
 * 'maj_disponible' / 'a_jour' (version comparée des deux côtés) / 'inconnu' (comparaison impossible).
 */
export function etatCatalogue(entree) {
  if (!entree.installe) return 'non_installe';
  if (entree.mise_a_jour_disponible === true) return 'maj_disponible';
  if (entree.mise_a_jour_disponible === false) return 'a_jour';
  return 'inconnu';
}

export function libelleEtatLangue(etat) {
  return LIBELLES_LANGUE[etat] ?? 'État de la langue inconnu.';
}

export function sousTitreLangue(etat) {
  return SOUS_TITRES_LANGUE[etat] ?? '';
}

export function titreLangue(langue) {
  return `${langue.nom} (${langue.code})`;
}

export function badgeLangue(langue) {
  return String(langue.code).slice(0, 2).toUpperCase();
}

export function piedLangue(etat) {
  return etat === 'protege' ? 'Aucune mise à jour du jeu ne les remplacera.' : '';
}

export function pastilleElementLangue(element) {
  if (element.id === 'locale') {
    return { libelle: element.protege ? 'Protégé' : 'À vérifier', ton: element.protege ? 'ok' : 'alerte', valeur: element.valeur ?? 'Introuvable' };
  }
  if (!element.present) return { libelle: 'Absent', ton: 'alerte', valeur: null };
  if (element.protege) return { libelle: 'Présent · Protégé', ton: 'ok', valeur: null };
  return { libelle: 'Présent', ton: 'neutre', valeur: null };
}

export function heureLocale(iso) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return HEURE_INCONNUE;
  return [date.getHours(), date.getMinutes(), date.getSeconds()].map((valeur) => String(valeur).padStart(2, '0')).join(':');
}

export function libelleResultat(resultat) {
  return LIBELLES_RESULTAT[resultat] ?? resultat;
}

export function tonResultat(resultat) {
  return LIBELLES_RESULTAT[resultat] ? resultat : 'erreur';
}

export function libelleCompteLigne(compte) {
  return compte ?? LIBELLE_COMMUN;
}

export function filtrerJournal(lignes, compte) {
  return compte === null ? lignes : lignes.filter((ligne) => ligne.compte === compte);
}

export function libelleJournal(etapes, erreurs, avertissements, comptes) {
  return [
    `${etapes} ${pluriel(etapes, 'étape', 'étapes')}`,
    `${erreurs} ${pluriel(erreurs, 'erreur', 'erreurs')}`,
    `${avertissements} ${pluriel(avertissements, 'avertissement', 'avertissements')}`,
    `${comptes} ${pluriel(comptes, 'compte', 'comptes')}`,
  ].join(SEPARATEUR);
}

export function libelleFiltreTous(nombreComptes) {
  return `Tous (${nombreComptes})`;
}

export function plusieursComptesDiagnostic(comptes) {
  return comptes.length > 1;
}

export function sousTitreDiagnostic(nombreComptes) {
  if (nombreComptes > 1) {
    return "Journal par compte. Les noms deviennent Compte 1, 2, 3 dans le rapport exporté ; aucun mot de passe n'y figure, enregistré ou saisi.";
  }
  return "Journal du lancement. Aucun mot de passe n'y figure, enregistré ou saisi.";
}

export function elementsSysteme(systeme) {
  return ETIQUETTES_SYSTEME.map(([cle, etiquette]) => ({ etiquette, valeur: String(systeme?.[cle] ?? '').trim() || 'Inconnu' }));
}

export function messageJournalVide(nombreLignes, nombreVisibles) {
  if (nombreLignes === 0) return "Aucune étape n'a encore été enregistrée.";
  return nombreVisibles === 0 ? "Aucune étape pour ce compte." : '';
}

export function libelleRapportExporte(chemin) {
  return `Rapport exporté : ${chemin}`;
}

export function cleLigneJournal(ligne) {
  return `${ligne.heure}|${ligne.compte ?? ''}|${ligne.etape}`;
}

export const DELAI_OUVERTURE_MS = 1500;
export const TITRE_PREPARATION = 'Préparons le jeu';
export const SOUS_TITRE_PREPARATION = 'Trois étapes, une seule fois.';
export const LIBELLE_TERMINER_PREPARATION = 'TERMINER LA PRÉPARATION';
export const TEXTE_LOGICIELS = 'EbonHux a besoin de Wine pour faire tourner le jeu.';
export const TEXTE_PREPARATION = "EbonHux met le jeu en état de démarrer depuis ce launcher. C'est à faire une seule fois.";
export const TEXTE_OCCUPE_PREPARATION = 'Préparation en cours… cela peut prendre une minute';
export const TEXTE_OCCUPE_INSTALLATION = 'Installation en cours… le système peut vous demander votre mot de passe.';
export const MESSAGE_DISTRIBUTION = "Votre distribution n'est pas prise en charge pour l'installation automatique. Installez Wine, puis revenez ici.";
export const TITRE_CONFIRMATION_INSTALLATION = 'Installer Wine ?';
export const TEXTE_CONFIRMATION_INSTALLATION = 'Pour installer Wine, votre système va vous demander votre mot de passe dans sa propre fenêtre. EbonHux ne le voit pas et ne le garde pas.';
export const TITRE_PRET = 'Tout est prêt';
export const TEXTE_PRET = 'Le jeu peut être lancé depuis EbonHux.';

const TITRES_ETAPES = { jeu: 'Dossier du jeu', logiciels: 'Logiciels nécessaires', preparation: 'Préparation du jeu' };
const ORDRE_ETAPES = ['jeu', 'logiciels', 'preparation'];
const ETATS_ETAPE = {
  fait: { libelle: 'Fait', ton: 'ok' },
  a_faire: { libelle: 'À faire', ton: '' },
  bloque: { libelle: 'Bloqué', ton: 'neutre' },
};
const LIBELLES_STATUT_ETAPE = { reussie: 'réussie', simulee: 'simulée' };

export function preparationRequise(installation) {
  return Boolean(installation) && installation.pret === false;
}

export function cartesPreparation(installation) {
  const etapes = installation?.etapes ?? [];
  return ORDRE_ETAPES.map((id, rang) => {
    const etape = etapes.find((candidate) => candidate.id === id);
    const etat = etape && etape.etat in ETATS_ETAPE ? etape.etat : 'a_faire';
    return {
      id,
      numero: rang + 1,
      titre: TITRES_ETAPES[id],
      etat,
      libelle: ETATS_ETAPE[etat].libelle,
      ton: ETATS_ETAPE[etat].ton,
      detail: etape?.detail ?? '',
    };
  });
}

export function etatEtape(installation, id) {
  return cartesPreparation(installation).find((carte) => carte.id === id).etat;
}

export function peutPreparer(installation) {
  return etatEtape(installation, 'jeu') === 'fait' && etatEtape(installation, 'logiciels') === 'fait';
}

export function libelleWine(wine) {
  if (!wine?.present) return "Wine n'est pas installé";
  return wine.version ? `Wine ${wine.version} est installé` : 'Wine est installé';
}

export function modeLogiciels(installation) {
  if (installation.wine?.present) return 'installe';
  return installation.installation_possible ? 'installable' : 'non_prise_en_charge';
}

export function validerCheminJeu(chemin) {
  const propre = String(chemin ?? '').trim();
  if (propre === '') return 'Saisissez le chemin du dossier du jeu.';
  if (propre.startsWith('~')) return 'Saisissez le chemin complet, sans « ~ ».';
  if (!propre.startsWith('/')) return 'Saisissez un chemin complet, qui commence par « / ».';
  return null;
}

export function libelleStatutEtape(statut) {
  return LIBELLES_STATUT_ETAPE[statut] ?? String(statut);
}

export function libelleLancerComptes(nombre) {
  return `Lancer (${nombre})`;
}

/** Index `dossier (minuscule) -> entrée du catalogue`, bâti sur `dossier` et `dossiers_installes` (absents tolérés). */
export function entreesCatalogueParDossier(entrees) {
  const index = new Map();
  for (const entree of entrees ?? []) {
    for (const dossier of [entree.dossier, ...(entree.dossiers_installes ?? [])]) {
      if (typeof dossier !== 'string' || dossier === '') continue;
      const cle = dossier.toLowerCase();
      if (!index.has(cle)) index.set(cle, entree);
    }
  }
  return index;
}

/** Données d'une carte : l'addon local, enrichi par l'entrée du catalogue si elle existe (sinon repli, « à jour »). */
export function ficheAddon(addon, indexCatalogue) {
  const entree = indexCatalogue?.get(addon.dossier.toLowerCase()) ?? null;
  const image = entree ? (entree.avatar ?? entree.images?.[0] ?? null) : null;
  const description = (entree?.description ? nettoyerTexteJeu(entree.description) : '') || nettoyerTexteJeu(addon.notes);
  return {
    addon,
    entree,
    titre: titreAddon(addon),
    description,
    image: typeof image === 'string' && image !== '' ? image : null,
    miseAJour: entree !== null && entree.mise_a_jour_disponible === true,
  };
}

/** Sépare les fiches en `{ miseAJour, aJour }` en gardant l'ordre reçu. */
export function trierAddonsInstalles(addons, indexCatalogue) {
  const fiches = addons.map((addon) => ficheAddon(addon, indexCatalogue));
  return { miseAJour: fiches.filter((fiche) => fiche.miseAJour), aJour: fiches.filter((fiche) => !fiche.miseAJour) };
}

export function libelleMisesAJourDisponibles(nombre) {
  return `Mises à jour disponibles (${nombre})`;
}
