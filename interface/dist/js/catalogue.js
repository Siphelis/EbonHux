import * as api from './api.js';
import { creerChampRecherche, creerOnglets } from './addons.js';
import { el, remplacer } from './dom.js';
import { rendreGlissable } from './glisser.js';
import { afficherChargement, retirerChargement } from './chargement.js';
import { doitAfficherChargement } from './chargement-etat.js';
import { icone } from './icones.js';
import { demanderSuppression } from './confirmation.js';
import {
  CATEGORIE_TOUTES, basculerMisesAJour, choisirCategorieEtat, categoriesDuCatalogue, entreesDeCategorie, etatCatalogue, compterMisesAJour, filtrerCatalogue, plageEntierementVisible, plageVisible, filtrerMisesAJour, initialeDe, libelleAffichagePlage, libelleEtatLigne, libelleInstallation,
  ligneVersionDetail, messageCatalogueVide, texteDescription, trierParNom,
} from './modeles.js';

// Liste virtualisée : pas vertical fixe d'une ligne (carte de 56 px + 8 px d'espace), identique à
// --catalogue-hauteur-ligne dans addons.css. Seules les lignes de la plage visible +/- MARGE existent dans le DOM.
const HAUTEUR_LIGNE = 64;
const MARGE = 6;
const DECALAGE_HAUT = 12; // padding-haut de .catalogue-liste__defile
const MESSAGE_AUCUNE_MAJ = 'Aucune mise à jour dans cette sélection.';
const TITRE_PAGE = 'Addons';
const LIBELLE_TOUT_METTRE_A_JOUR = 'Tout mettre à jour';

function sousTitreDe(uniques, invalides) {
  const base = uniques === null ? 'Catalogue way-of-elendil' : `Catalogue way-of-elendil · ${uniques} ${uniques === 1 ? 'addon' : 'addons'}`;
  if (!invalides) return base;
  return `${base} · ${invalides} ${invalides === 1 ? 'entrée ignorée' : 'entrées ignorées'}`;
}

function libelleMisesAJour(nombre) {
  return `${nombre} ${nombre === 1 ? 'mise à jour' : 'mises à jour'}`;
}

function creerImage(source, classeImage, repli) {
  const conteneur = el('div', { classe: classeImage });
  if (source) {
    const image = el('img', { attrs: { loading: 'lazy', decoding: 'async', alt: '', src: source } });
    image.addEventListener('error', () => conteneur.replaceChildren(repli()));
    conteneur.append(image);
  } else {
    conteneur.append(repli());
  }
  return conteneur;
}

export function creerVueCatalogue(contexte) {
  const occupes = new Set();
  let courant = null;
  let derniereErreur = null;
  let telechargement = false;
  let chargementDeLecture = false;
  let toutEnCours = false;
  let rafraichiAutomatiquement = false;
  let categorieActive = CATEGORIE_TOUTES;
  let majSeulement = false;
  let filtreesCourantes = [];
  let plageCourante = { debut: -1, fin: -1 };
  let rafEnAttente = 0;
  let selection = null;
  let imageChoisie = null;
  let selectionDetail = null;

  const sousTitre = el('p', { classe: 'sous-titre', texte: sousTitreDe(null, 0) });
  const recherche = creerChampRecherche({
    libelle: 'Rechercher un addon',
    quandChange: () => {
      // Décision : tout changement de filtre (recherche, catégorie, mises à jour) remet le défilement en haut.
      zoneListe.scrollTop = 0;
      rendre();
    },
  });
  recherche.racine.classList.add('carte');
  const onglets = creerOnglets({ actif: 'catalogue', naviguer: contexte.naviguer });

  const boutonActualiser = el('button', { classe: 'catalogue-actualiser', attrs: { type: 'button', 'aria-label': 'Actualiser le catalogue', title: 'Actualiser le catalogue' } }, [icone('actualiser')]);
  boutonActualiser.addEventListener('click', () => actualiser());
  const compteMaj = el('button', { classe: 'catalogue-maj', attrs: { type: 'button', 'aria-pressed': 'false' } });
  compteMaj.addEventListener('click', () => {
    ({ categorie: categorieActive, majSeulement } = basculerMisesAJour({ categorie: categorieActive, majSeulement }));
    zoneListe.scrollTop = 0;
    rendre();
  });
  const boutonToutMaj = el('button', { classe: 'catalogue-tout-maj', attrs: { type: 'button', 'aria-label': LIBELLE_TOUT_METTRE_A_JOUR } }, [icone('majHaut')]);
  boutonToutMaj.addEventListener('click', () => toutMettreAJour());
  const infobulle = el('span', { classe: 'catalogue-infobulle', texte: LIBELLE_TOUT_METTRE_A_JOUR, attrs: { 'aria-hidden': 'true' } });
  const groupeMaj = el('div', { classe: 'catalogue-groupe-maj' }, [boutonToutMaj, infobulle]);
  const barre = el('div', { classe: 'catalogue-barre' }, [
    recherche.racine,
    el('span', { classe: 'catalogue-tri', texte: 'Trier : nom A-Z' }),
    compteMaj,
    boutonActualiser,
    groupeMaj,
  ]);

  const pastilles = el('div', { classe: 'catalogue-pastilles', attrs: { role: 'group', 'aria-label': 'Catégories' } });
  const glisseurPastilles = rendreGlissable(pastilles);

  // Panneau de la liste.
  const listeUl = el('ul', { classe: 'catalogue-liste__lignes', attrs: { role: 'list' } });
  const zoneListe = el('div', { classe: 'catalogue-liste__defile' }, [listeUl]);
  const message = el('p', { classe: 'message-liste catalogue-message' });
  const pied = el('p', { classe: 'catalogue-liste__pied' });
  const panneauListe = el('section', { classe: 'carte catalogue-liste', attrs: { 'aria-label': 'Liste des addons' } }, [
    el('div', { classe: 'catalogue-liste__corps' }, [zoneListe, message]),
    pied,
  ]);

  // Panneau du détail.
  const detailDefile = el('div', { classe: 'catalogue-detail__defile' });
  const detailPied = el('div', { classe: 'catalogue-detail__pied' });
  const panneauDetail = el('section', { classe: 'carte catalogue-detail', attrs: { 'aria-label': 'Détail de l’addon' } }, [detailDefile, detailPied]);

  // Zone d'attente : enfant direct de .catalogue-zone, jamais touché par rendre() (qui n'écrit que des textContent
  // dans les autres nœuds) ; vide hors attente (:empty la masque), elle recouvre la liste et le détail.
  const zoneAttente = el('div', { classe: 'catalogue-attente' });
  const racine = el('section', { classe: 'ecran catalogue' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: TITRE_PAGE }), sousTitre]),
    onglets,
    barre,
    pastilles,
    el('div', { classe: 'catalogue-zone' }, [panneauListe, panneauDetail, zoneAttente]),
  ]);

  zoneListe.addEventListener('scroll', planifierPeinture, { passive: true });
  if (typeof ResizeObserver === 'function') new ResizeObserver(planifierPeinture).observe(zoneListe);

  function planifierPeinture() {
    if (rafEnAttente) return;
    rafEnAttente = requestAnimationFrame(() => {
      rafEnAttente = 0;
      peindre(false);
    });
  }

  function defilementUtile() {
    return Math.max(0, zoneListe.scrollTop - DECALAGE_HAUT);
  }

  /** Amène le rang (0-based) dans la vue, en ne défilant que s'il dépasse. */
  function amenerDansVue(rang) {
    const haut = DECALAGE_HAUT + rang * HAUTEUR_LIGNE;
    const bas = haut + HAUTEUR_LIGNE - 8;
    if (haut < zoneListe.scrollTop) zoneListe.scrollTop = haut - DECALAGE_HAUT;
    else if (bas > zoneListe.scrollTop + zoneListe.clientHeight) zoneListe.scrollTop = bas - zoneListe.clientHeight;
  }

  /** Met à jour la liste : plage visible + marge. `complet` reconstruit toutes les lignes, sinon diff par url. */
  function peindre(complet) {
    const total = filtreesCourantes.length;
    const fenetre = zoneListe.clientHeight;
    const defilement = defilementUtile();
    const plage = plageVisible({ defilement, hauteurFenetre: fenetre, hauteurLigne: HAUTEUR_LIGNE, total, marge: MARGE });
    const vue = plageEntierementVisible({ defilement, hauteurFenetre: fenetre, hauteurLigne: HAUTEUR_LIGNE, total });
    pied.hidden = total === 0;
    pied.textContent = libelleAffichagePlage(vue.premiere, vue.derniere, total);
    if (!complet && plage.debut === plageCourante.debut && plage.fin === plageCourante.fin) return;
    plageCourante = { debut: plage.debut, fin: plage.fin };

    const actif = document.activeElement;
    const ligneFocus = actif && listeUl.contains(actif) ? actif.closest('li') : null;
    const refocus = complet && ligneFocus ? { url: ligneFocus._url, classe: actif.className } : null;
    const existantes = new Map();
    for (const li of [...listeUl.children]) {
      const garde = !complet && (li._rang >= plage.debut && li._rang < plage.fin ? true : li === ligneFocus);
      if (garde) existantes.set(li._url, li);
      else li.remove();
    }
    for (let rang = plage.debut; rang < plage.fin; rang += 1) {
      const entree = filtreesCourantes[rang];
      if (existantes.get(entree.url)?._rang === rang) continue;
      const li = creerLigne(entree);
      li._rang = rang;
      li._url = entree.url;
      li.style.top = `${rang * HAUTEUR_LIGNE}px`;
      const suivante = [...listeUl.children].find((c) => c._rang > rang) ?? null;
      listeUl.insertBefore(li, suivante);
    }
    if (refocus) {
      const li = [...listeUl.children].find((c) => c._url === refocus.url);
      const cible = li && [...li.children].find((c) => c.className === refocus.classe);
      if (cible && !cible.disabled) cible.focus({ preventScroll: true });
    }
  }

  let categorieChoisieALAmener = false;

  function choisirCategorie(nom) {
    ({ categorie: categorieActive, majSeulement } = choisirCategorieEtat({ categorie: categorieActive, majSeulement }, nom));
    categorieChoisieALAmener = true;
    zoneListe.scrollTop = 0;
    rendre();
  }

  function choisirEntree(entree) {
    if (selection === entree.url) return;
    const avant = selection;
    selection = entree.url;
    imageChoisie = null;
    for (const li of listeUl.children) {
      if (li._url !== avant && li._url !== selection) continue;
      const choisie = li._url === selection;
      li.classList.toggle('catalogue-ligne--choisie', choisie);
      li.firstElementChild.setAttribute('aria-pressed', String(choisie));
    }
    const rang = filtreesCourantes.findIndex((e) => e.url === selection);
    if (rang >= 0) amenerDansVue(rang);
    rendreDetail(entree);
  }

  async function recharger() {
    await Promise.all([contexte.charger('catalogue'), contexte.charger('addons')]);
  }

  async function installer(entree) {
    const miseAJour = etatCatalogue(entree) === 'maj_disponible';
    occupes.add(entree.nom);
    rendre();
    try {
      await api.installerAddon(entree.nom, miseAJour);
      entree.installe = true;
      contexte.bandeau.montrerSucces(libelleInstallation(entree.nom, miseAJour));
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupes.delete(entree.nom);
    rendre();
    await recharger();
  }

  async function desinstaller(entree) {
    occupes.add(entree.nom);
    rendre();
    let reussi = true;
    try {
      for (const dossier of entree.dossiers_installes) await api.desinstallerAddon(dossier);
    } catch (cause) {
      reussi = false;
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    if (reussi) contexte.bandeau.montrerSucces(`${entree.nom} désinstallé.`);
    occupes.delete(entree.nom);
    rendre();
    await recharger();
  }

  async function toutMettreAJour() {
    if (toutEnCours || courant === null) return;
    const cibles = entreesDeCategorie(courant.entrees, CATEGORIE_TOUTES).filter((entree) => etatCatalogue(entree) === 'maj_disponible');
    if (cibles.length === 0) return;
    toutEnCours = true;
    rendre();
    let reussites = 0;
    const echecs = [];
    for (const entree of cibles) {
      occupes.add(entree.nom);
      rendre();
      try {
        await api.installerAddon(entree.nom, true);
        reussites += 1;
      } catch {
        echecs.push(entree.nom);
      }
      occupes.delete(entree.nom);
    }
    toutEnCours = false;
    rendre();
    await recharger();
    const suite = echecs.length > 0 ? ` Échec : ${echecs.join(', ')}.` : '';
    const texte = `${reussites} ${reussites === 1 ? 'addon mis à jour' : 'addons mis à jour'}.${suite}`;
    if (echecs.length > 0) contexte.bandeau.montrer({ message: texte });
    else contexte.bandeau.montrerSucces(texte);
  }

  async function actualiser() {
    if (telechargement) return;
    telechargement = true;
    chargementDeLecture = false;
    rendre();
    afficherChargement(zoneAttente, { etape: 'Téléchargement du catalogue' });
    try {
      await api.rafraichirCatalogue();
      retirerChargement(zoneAttente);
    } catch (cause) {
      retirerChargement(zoneAttente, { echec: true });
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    telechargement = false;
    rendre();
    await recharger();
  }

  async function ouvrir() {
    rendre(); // premier rendu : le loader couvre la lecture du cache
    await contexte.charger('catalogue');
    if (rafraichiAutomatiquement) return;
    rafraichiAutomatiquement = true;
    if (courant === null || courant.entrees.length === 0) {
      await actualiser();
      return;
    }
    try {
      await api.rafraichirCatalogue();
    } catch {
      return;
    }
    await recharger();
  }

  /** Action proposée pour une entrée : icône, libellés, variante et fonction à appeler. */
  function choixAction(entree) {
    const etat = etatCatalogue(entree);
    const dossiers = entree.dossiers_installes ?? [];
    const choix = { icone: 'installer', libelle: `Installer ${entree.nom}`, court: 'Installer', action: () => installer(entree), variante: 'installer' };
    if (etat === 'maj_disponible') {
      Object.assign(choix, { icone: 'majHaut', libelle: `Mettre à jour ${entree.nom}`, court: 'Mettre à jour', variante: 'maj' });
    } else if (etat !== 'non_installe' && dossiers.length > 0) {
      Object.assign(choix, { icone: 'corbeille', libelle: `Désinstaller ${entree.nom}`, court: 'Supprimer', action: () => demanderSuppression(entree.nom, () => desinstaller(entree)), variante: 'corbeille' });
    }
    return choix;
  }

  function creerAction(entree) {
    const choix = choixAction(entree);
    const occupe = occupes.has(entree.nom);
    const bouton = el('button', { classe: `catalogue-action catalogue-action--${choix.variante}`, attrs: { type: 'button', 'aria-label': choix.libelle, title: choix.libelle } }, [icone(choix.icone)]);
    bouton.disabled = occupe || toutEnCours;
    if (occupe) bouton.setAttribute('aria-busy', 'true');
    bouton.addEventListener('click', choix.action);
    return bouton;
  }

  function creerActionDetail(entree) {
    const choix = choixAction(entree);
    const occupe = occupes.has(entree.nom);
    const bouton = el('button', { classe: `catalogue-detail__action catalogue-detail__action--${choix.variante}`, attrs: { type: 'button', title: choix.libelle } }, [
      icone(occupe ? 'actualiser' : choix.icone),
      el('span', { texte: occupe ? 'En cours' : choix.court }),
    ]);
    bouton.disabled = occupe || toutEnCours;
    if (occupe) bouton.setAttribute('aria-busy', 'true');
    bouton.addEventListener('click', choix.action);
    return bouton;
  }

  function classeEtat(entree) {
    if (occupes.has(entree.nom)) return 'cours';
    const etat = etatCatalogue(entree);
    if (etat === 'non_installe') return 'non';
    return etat === 'maj_disponible' ? 'maj' : 'installe';
  }

  function creerLigne(entree) {
    const choisie = entree.url === selection;
    const libelleNom = entree.version ? `${entree.nom} ${entree.version}` : entree.nom;
    const etat = libelleEtatLigne(entree, occupes.has(entree.nom));
    const choix = el('button', { classe: 'catalogue-ligne__choix', attrs: { type: 'button', title: entree.nom, 'aria-pressed': String(choisie) } }, [
      creerImage(entree.avatar, 'catalogue-ligne__vignette', () => el('span', { classe: 'catalogue-ligne__initiale', texte: initialeDe(entree.nom) })),
      el('span', { classe: 'catalogue-ligne__texte' }, [
        el('strong', { classe: 'catalogue-ligne__nom', texte: libelleNom }),
        el('span', { classe: 'catalogue-ligne__description', texte: entree.description }),
      ]),
      el('span', { classe: `catalogue-ligne__etat catalogue-ligne__etat--${classeEtat(entree)}`, texte: etat }),
    ]);
    choix.addEventListener('click', () => choisirEntree(entree));
    const ligne = el('li', { classe: `catalogue-ligne${choisie ? ' catalogue-ligne--choisie' : ''}`, attrs: { 'data-nom': entree.nom } }, [choix, creerAction(entree)]);
    return ligne;
  }

  function creerPastille(categorie) {
    const active = categorie.nom === categorieActive;
    const bouton = el('button', {
      classe: `catalogue-pastille${active ? ' catalogue-pastille--active' : ''}`,
      attrs: { type: 'button', title: categorie.nom, 'aria-pressed': String(active) },
      texte: `${categorie.nom} ${categorie.nombre}`,
    });
    bouton.addEventListener('click', () => choisirCategorie(categorie.nom));
    return bouton;
  }

  function rendreDetail(entree) {
    const defilementAvant = selectionDetail === (entree?.url ?? null) ? detailDefile.scrollTop : 0;
    selectionDetail = entree?.url ?? null;
    if (!entree) {
      remplacer(detailDefile);
      remplacer(detailPied);
      return;
    }
    const images = Array.isArray(entree.images) ? entree.images : [];
    const principale = imageChoisie !== null && images[imageChoisie] ? images[imageChoisie] : (entree.avatar || images[0] || null);
    const repli = () => el('span', { classe: 'catalogue-detail__initiale', texte: initialeDe(entree.nom) });
    const enfants = [
      creerImage(principale, 'catalogue-detail__image', repli),
      el('div', { classe: 'catalogue-detail__entete' }, [
        creerImage(entree.avatar, 'catalogue-detail__avatar', () => el('span', { classe: 'catalogue-ligne__initiale', texte: initialeDe(entree.nom) })),
        el('div', { classe: 'catalogue-detail__identite' }, [
          el('strong', { classe: 'catalogue-detail__titre', texte: entree.nom }),
          el('span', { classe: 'catalogue-detail__version', texte: ligneVersionDetail(entree) }),
          ...(entree.auteur ? [el('span', { classe: 'catalogue-detail__auteur', texte: `Auteur : ${entree.auteur}` })] : []),
        ]),
      ]),
      el('p', { classe: 'catalogue-detail__description', texte: texteDescription(entree) }),
    ];
    if (images.length >= 2) {
      enfants.push(el('div', { classe: 'catalogue-detail__miniatures' }, images.map((source, rang) => {
        const bouton = el('button', {
          classe: `catalogue-detail__miniature${source === principale ? ' catalogue-detail__miniature--active' : ''}`,
          attrs: { type: 'button', 'aria-label': `Image ${rang + 1}`, 'aria-pressed': String(source === principale) },
        }, [el('img', { attrs: { loading: 'lazy', alt: '', src: source } })]);
        bouton.addEventListener('click', () => {
          imageChoisie = rang;
          rendreDetail(entree);
        });
        return bouton;
      })));
    }
    remplacer(detailDefile, ...enfants);
    remplacer(detailPied, creerActionDetail(entree));
    detailDefile.scrollTop = defilementAvant;
  }

  function rendre() {
    boutonActualiser.disabled = telechargement;
    if (telechargement) boutonActualiser.setAttribute('aria-busy', 'true');
    else boutonActualiser.removeAttribute('aria-busy');
    // Loader : visible tant qu'il n'y a rien à montrer (un seul bloc par zone, afficher/retirer idempotents).
    // Un loader posé par Actualiser (avec ou sans contenu) reste géré par actualiser().
    if (doitAfficherChargement({ courant, telechargement, erreur: derniereErreur })) {
      chargementDeLecture = !telechargement;
      afficherChargement(zoneAttente, { etape: telechargement ? 'Téléchargement du catalogue' : 'Lecture du catalogue' });
    } else if (chargementDeLecture) {
      chargementDeLecture = false;
      retirerChargement(zoneAttente, { echec: true });
    }
    if (courant === null) {
      sousTitre.textContent = sousTitreDe(null, 0);
      remplacer(listeUl);
      listeUl.style.height = '0px';
      plageCourante = { debut: -1, fin: -1 };
      remplacer(pastilles);
      filtreesCourantes = [];
      compteMaj.hidden = true;
      groupeMaj.hidden = true;
      pied.hidden = true;
      rendreDetail(null);
      message.textContent = derniereErreur ? derniereErreur.message : '';
      message.hidden = message.textContent === '';
      return;
    }
    const categories = categoriesDuCatalogue(courant.entrees);
    if (!categories.some((categorie) => categorie.nom === categorieActive)) categorieActive = CATEGORIE_TOUTES;
    const uniques = categories[0].nombre;
    const misesAJour = compterMisesAJour(entreesDeCategorie(courant.entrees, CATEGORIE_TOUTES));
    if (misesAJour < 1) majSeulement = false;
    const dansCategorie = entreesDeCategorie(courant.entrees, categorieActive);
    const filtrees = trierParNom(filtrerCatalogue(filtrerMisesAJour(dansCategorie, majSeulement), recherche.valeur()));
    filtreesCourantes = filtrees;
    if (!filtrees.some((entree) => entree.url === selection)) {
      selection = filtrees[0]?.url ?? null;
      imageChoisie = null;
    }

    sousTitre.textContent = sousTitreDe(uniques, courant.invalides);
    const defilementPastilles = pastilles.scrollLeft;
    remplacer(pastilles, ...categories.map(creerPastille));
    pastilles.scrollLeft = defilementPastilles;
    if (categorieChoisieALAmener) {
      categorieChoisieALAmener = false;
      glisseurPastilles.amenerDansVue(pastilles.querySelector('.catalogue-pastille--active'));
    }
    compteMaj.textContent = libelleMisesAJour(misesAJour);
    compteMaj.setAttribute('aria-pressed', String(majSeulement));
    compteMaj.setAttribute('aria-label', majSeulement ? 'Afficher toutes les entrées' : `Afficher les ${libelleMisesAJour(misesAJour)}`);
    compteMaj.classList.toggle('catalogue-maj--active', majSeulement);
    compteMaj.hidden = misesAJour < 1;
    groupeMaj.hidden = misesAJour < 1;
    boutonToutMaj.disabled = toutEnCours || telechargement;
    if (toutEnCours) boutonToutMaj.setAttribute('aria-busy', 'true');
    else boutonToutMaj.removeAttribute('aria-busy');

    const defilement = zoneListe.scrollTop;
    listeUl.style.height = `${filtrees.length * HAUTEUR_LIGNE}px`;
    zoneListe.scrollTop = defilement;
    peindre(true);
    rendreDetail(filtrees.find((entree) => entree.url === selection) ?? null);

    const vide = majSeulement && filtrees.length === 0 && !recherche.valeur().trim() ? MESSAGE_AUCUNE_MAJ : messageCatalogueVide(courant.entrees.length, filtrees.length, recherche.valeur());
    const attente = telechargement && courant.entrees.length === 0;
    message.textContent = attente ? '' : (vide || (filtrees.length === 0 ? 'Aucun addon dans cette catégorie.' : ''));
    message.hidden = message.textContent === '';
  }

  return {
    racine,
    ouvrir,
    mettreAJour({ catalogue, erreurs }) {
      if (catalogue === courant && (erreurs.catalogue ?? null) === derniereErreur) return;
      courant = catalogue;
      derniereErreur = erreurs.catalogue ?? null;
      rendre();
    },
  };
}
