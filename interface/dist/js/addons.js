import * as api from './api.js';
import { el, remplacer, svg } from './dom.js';
import { rendreGlissable } from './glisser.js';
import { icone } from './icones.js';
import { afficherChargement, retirerChargement } from './chargement.js';
import { creerInterrupteur } from './interrupteur.js';
import { demanderSuppression } from './confirmation.js';
import {
  CATEGORIE_TOUTES, basculerMisesAJour, choisirCategorieEtat, initialeDe, libelleAddonsInstalles, libelleAffichagePlage, libelleProblemes, messageAddonsVide, texteDescription, titreAddon,
} from './modeles.js';
import {
  addonsVisibles, aplatirLignes, categoriesInstalles, compterMisesAJourInstalles, comptesInstalles, defilementPourVoir, dossiersOuvertsParRecherche, fichesInstalles, filtrerInstalles,
  libelleEtatInstalle, libelleNomInstalle, libelleSousTitreLigne, ligneVersionInstalle, modulesDe, nomsAMettreAJour, plageLignes,
} from './installes-modele.js';

const LOUPE = [{ balise: 'circle', cx: 11, cy: 11, r: 6.5 }, { d: 'M16 16l5 5' }];

export function creerChampRecherche({ libelle, quandChange }) {
  const entree = el('input', { attrs: { type: 'text', placeholder: libelle, 'aria-label': libelle, autocomplete: 'off', spellcheck: 'false' } });
  entree.addEventListener('input', () => quandChange(entree.value));
  const racine = el('label', { classe: 'recherche' }, [svg(LOUPE, 'icone trait'), entree]);
  return { racine, valeur: () => entree.value };
}

export function creerOnglets({ actif, naviguer }) {
  const onglet = (id, libelle) => {
    const bouton = el('button', { classe: 'onglet', attrs: { type: 'button', role: 'tab', 'aria-selected': String(id === actif) }, texte: libelle });
    bouton.addEventListener('click', () => {
      if (id !== actif) naviguer(id);
    });
    return bouton;
  };
  return el('div', { classe: 'onglets', attrs: { role: 'tablist', 'aria-label': 'Addons' } }, [onglet('addons', 'Installés'), onglet('catalogue', 'Catalogue')]);
}

// Liste virtualisée à hauteurs variables : addon = carte de 56 px + 8 px (pas du Catalogue), module = 28 px (voir installes-modele.js).
const MARGE = 6;
const DECALAGE_HAUT = 12;
const LIBELLE_TOUT_METTRE_A_JOUR = 'Tout mettre à jour';
const MESSAGE_AUCUNE_MAJ = 'Aucune mise à jour dans cette sélection.';

const coche = () => svg([{ balise: 'circle', cx: 12, cy: 12, r: 9 }, { d: 'M8 12.3l2.8 2.8L16 9.6' }], 'icone trait');
const info = () => svg([{ balise: 'circle', cx: 12, cy: 12, r: 9 }, { d: 'M12 11v5.5M12 7.6v.1' }], 'icone trait');
const libelleMisesAJour = (n) => `${n} ${n === 1 ? 'mise à jour' : 'mises à jour'}`;

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

export function creerVueAddons(contexte) {
  let courantes = null;
  let catalogueCourant = null;
  let derniereErreur = null;
  let occupe = false;
  const enCours = new Set(); // dossiers en cours d'action
  let categorieActive = CATEGORIE_TOUTES;
  let majSeulement = false;
  let fichesFiltrees = [];
  let plageCourante = { debut: -1, fin: -1 };
  let rafEnAttente = 0;
  let selection = null;
  let imageChoisie = null;
  let selectionDetail = null;
  let categorieAAmener = false;
  let lignes = []; // lignes à plat (addons et modules ouverts) avec leur décalage cumulé
  let hauteurListe = 0;
  const ouverts = new Set(); // addons dont le volet de modules est ouvert (en mémoire)
  let reinitVolets = true; // tous repliés au démarrage et après changement de filtre/tri/recherche

  const sousTitre = el('p', { classe: 'sous-titre' });
  const recherche = creerChampRecherche({
    libelle: 'Rechercher un addon',
    quandChange: () => {
      reinitVolets = true;
      zoneListe.scrollTop = 0;
      rendre();
    },
  });
  recherche.racine.classList.add('carte');
  const bascule = creerInterrupteur({ libelle: 'Afficher les bibliothèques', actif: false, simple: true, quandChange: () => { reinitVolets = true; zoneListe.scrollTop = 0; rendre(); } });
  const groupeBascule = el('label', { classe: 'bascule installes-bascule', attrs: { title: 'Afficher les bibliothèques' } }, [icone('grille'), bascule.racine]);
  const onglets = creerOnglets({ actif: 'addons', naviguer: contexte.naviguer });

  const compteMaj = el('button', { classe: 'catalogue-maj', attrs: { type: 'button', 'aria-pressed': 'false' } });
  compteMaj.addEventListener('click', () => {
    reinitVolets = true;
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
    groupeBascule,
    compteMaj,
    groupeMaj,
  ]);

  const pastilles = el('div', { classe: 'catalogue-pastilles', attrs: { role: 'group', 'aria-label': 'Catégories' } });
  const glisseurPastilles = rendreGlissable(pastilles);

  const legende = el('p', { classe: 'installes-legende' });
  const listeUl = el('ul', { classe: 'catalogue-liste__lignes', attrs: { role: 'list' } });
  const zoneListe = el('div', { classe: 'catalogue-liste__defile' }, [listeUl]);
  const message = el('p', { classe: 'message-liste catalogue-message' });
  const pied = el('p', { classe: 'catalogue-liste__pied' });
  const panneauListe = el('section', { classe: 'carte catalogue-liste', attrs: { 'aria-label': 'Liste des addons' } }, [
    legende,
    el('div', { classe: 'catalogue-liste__corps' }, [zoneListe, message]),
    pied,
  ]);
  const detailDefile = el('div', { classe: 'catalogue-detail__defile' });
  const detailPied = el('div', { classe: 'catalogue-detail__pied installes-detail__pied' });
  const panneauDetail = el('section', { classe: 'carte catalogue-detail', attrs: { 'aria-label': 'Détail de l’addon' } }, [detailDefile, detailPied]);
  const zoneAttente = el('div', { classe: 'catalogue-attente' });
  let chargementAffiche = false;
  const racine = el('section', { classe: 'ecran catalogue installes-vue' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: 'Addons' }), sousTitre]),
    onglets,
    barre,
    pastilles,
    el('div', { classe: 'catalogue-zone' }, [panneauListe, panneauDetail, zoneAttente]),
  ]);
  legende.append(
    el('span', { classe: 'installes-legende__el' }, [icone('oeil'), el('span', { texte: 'suivre' })]),
    el('span', { classe: 'installes-legende__el' }, [icone('majHaut'), el('span', { texte: 'mettre à jour' })]),
    el('span', { classe: 'installes-legende__el' }, [icone('corbeille'), el('span', { texte: 'supprimer' })]),
  );

  zoneListe.addEventListener('scroll', planifierPeinture, { passive: true });
  if (typeof ResizeObserver === 'function') new ResizeObserver(planifierPeinture).observe(zoneListe);

  function planifierPeinture() {
    if (rafEnAttente) return;
    rafEnAttente = requestAnimationFrame(() => {
      rafEnAttente = 0;
      peindre(false);
    });
  }

  function amenerDansVue(rang) {
    const index = lignes.findIndex((ligne) => ligne.type === 'addon' && ligne.rangAddon === rang);
    zoneListe.scrollTop = defilementPourVoir(lignes, index, { defilement: zoneListe.scrollTop, hauteurFenetre: zoneListe.clientHeight, decalageHaut: DECALAGE_HAUT });
  }

  const cleLigne = (ligne) => (ligne.type === 'module' ? `${ligne.fiche.addon.dossier}\u0000${ligne.module.dossier}` : ligne.fiche.addon.dossier);

  /** Recalcule les lignes à plat et la hauteur de la liste (défilement conservé). */
  function calculerLignes() {
    ({ lignes, hauteurTotale: hauteurListe } = aplatirLignes(fichesFiltrees, ouverts));
    const defilement = zoneListe.scrollTop;
    listeUl.style.height = `${hauteurListe}px`;
    zoneListe.scrollTop = defilement;
  }

  function peindre(complet) {
    const total = fichesFiltrees.length;
    const fenetre = zoneListe.clientHeight;
    const defilement = Math.max(0, zoneListe.scrollTop - DECALAGE_HAUT);
    const plage = plageLignes(lignes, { defilement, hauteurFenetre: fenetre, marge: MARGE });
    const vue = addonsVisibles(lignes, { defilement, hauteurFenetre: fenetre });
    pied.hidden = total === 0;
    pied.textContent = libelleAffichagePlage(vue.premiere, vue.derniere, total);
    if (!complet && plage.debut === plageCourante.debut && plage.fin === plageCourante.fin) return;
    plageCourante = { debut: plage.debut, fin: plage.fin };

    const actif = document.activeElement;
    const ligneFocus = actif && listeUl.contains(actif) ? actif.closest('li') : null;
    const refocus = complet && ligneFocus ? { dossier: ligneFocus._dossier, classe: actif.className } : null;
    const existantes = new Map();
    for (const li of [...listeUl.children]) {
      const garde = !complet && (li._rang >= plage.debut && li._rang < plage.fin ? true : li === ligneFocus);
      if (garde) existantes.set(li._dossier, li);
      else li.remove();
    }
    for (let rang = plage.debut; rang < plage.fin; rang += 1) {
      const ligne = lignes[rang];
      const cle = cleLigne(ligne);
      if (existantes.get(cle)?._rang === rang) continue;
      const li = ligne.type === 'module' ? creerLigneModule(ligne.fiche, ligne.module) : creerLigne(ligne.fiche);
      li._rang = rang;
      li._dossier = cle;
      li.style.top = `${ligne.haut}px`;
      const suivante = [...listeUl.children].find((c) => c._rang > rang) ?? null;
      listeUl.insertBefore(li, suivante);
    }
    if (refocus) {
      const li = [...listeUl.children].find((c) => c._dossier === refocus.dossier);
      const cible = li && [...li.querySelectorAll('button')].find((c) => c.className === refocus.classe);
      if (cible && !cible.disabled) cible.focus({ preventScroll: true });
    }
  }

  function choisirCategorie(nom) {
    ({ categorie: categorieActive, majSeulement } = choisirCategorieEtat({ categorie: categorieActive, majSeulement }, nom));
    reinitVolets = true;
    categorieAAmener = true;
    zoneListe.scrollTop = 0;
    rendre();
  }

  function choisirFiche(fiche) {
    if (selection === fiche.addon.dossier) return;
    const avant = selection;
    selection = fiche.addon.dossier;
    imageChoisie = null;
    for (const li of listeUl.children) {
      if (li._dossier !== avant && li._dossier !== selection) continue;
      const choisie = li._dossier === selection;
      li.classList.toggle('catalogue-ligne--choisie', choisie);
      li.firstElementChild.setAttribute('aria-pressed', String(choisie));
    }
    const rang = fichesFiltrees.findIndex((f) => f.addon.dossier === selection);
    if (rang >= 0) amenerDansVue(rang);
    rendreDetail(fiche);
  }

  /** Clic sur un addon à modules : sélectionne l'addon (détail) ET ouvre/replie son volet. */
  function cliquerFiche(fiche) {
    choisirFiche(fiche);
    if (modulesDe(fiche).length === 0) return;
    if (ouverts.has(fiche.addon.dossier)) ouverts.delete(fiche.addon.dossier);
    else ouverts.add(fiche.addon.dossier);
    calculerLignes();
    peindre(true);
    if (!ouverts.has(fiche.addon.dossier)) return;
    // Volet ouvert : amener le dernier module en vue sans faire sortir l'addon par le haut.
    const index = lignes.findIndex((l) => l.type === 'addon' && l.fiche === fiche);
    const dernier = index + modulesDe(fiche).length;
    const cible = defilementPourVoir(lignes, dernier, { defilement: zoneListe.scrollTop, hauteurFenetre: zoneListe.clientHeight, decalageHaut: DECALAGE_HAUT, ecart: 0 });
    zoneListe.scrollTop = Math.min(cible, Math.max(0, DECALAGE_HAUT + lignes[index].haut - DECALAGE_HAUT));
  }

  async function recharger() {
    await Promise.all([contexte.charger('addons'), contexte.charger('catalogue')]);
  }

  /** Une action à la fois ; `dossiers` marque les lignes concernées « en cours ». */
  async function executer(dossiers, action) {
    if (occupe) return;
    occupe = true;
    for (const d of dossiers) enCours.add(d);
    rendre();
    try {
      await action();
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupe = false;
    enCours.clear();
    await recharger();
    rendre();
  }

  const changerSuivi = (addon) => executer([addon.dossier], async () => {
    const reponse = await api.definirSuivi(addon.dossier, !addon.suivi);
    if (courantes) courantes.suivis = reponse.suivis;
  });
  const supprimer = (addon) => {
    const nombre = (addon.modules ?? []).length;
    demanderSuppression(titreAddon(addon), () => executer([addon.dossier], async () => {
      try {
        await api.desinstallerAddon(addon.dossier);
      } catch (cause) {
        const { code, message } = api.erreurDe(cause);
        if (nombre === 0) throw cause;
        throw Object.assign(new Error(`La suppression de ${titreAddon(addon)} et de ses modules est incomplète : ${message} La liste a été relue depuis le disque.`), { code });
      }
    }), nombre);
  };
  const supprimerModule = (addon, module) => demanderSuppression(
    titreAddon({ titre: module.titre, dossier: module.dossier }),
    () => executer([`${addon.dossier}\u0000${module.dossier}`, module.dossier], () => api.desinstallerAddon(module.dossier)),
  );
  const mettreAJour = (fiche) => executer([fiche.addon.dossier], () => api.installerAddon(fiche.entree.nom, true));

  async function toutMettreAJour() {
    const toutes = fichesInstalles(courantes?.addons, catalogueCourant?.entrees, { bibliotheques: bascule.entree.checked });
    const noms = nomsAMettreAJour(toutes);
    if (occupe || noms.length === 0) return;
    await executer(toutes.filter((f) => f.miseAJour).map((f) => f.addon.dossier), async () => {
      for (const nom of noms) await api.installerAddon(nom, true);
    });
  }

  function creerBouton(classe, nom, libelle, { actif = false, presse, desactive = false, quandClic }) {
    const bouton = el('button', { classe, attrs: { type: 'button', 'aria-label': libelle, title: libelle } }, [icone(nom)]);
    if (actif) bouton.classList.add('installes-actif');
    if (presse !== undefined) bouton.setAttribute('aria-pressed', String(presse));
    bouton.disabled = desactive;
    bouton.addEventListener('click', quandClic);
    return bouton;
  }

  function creerLigne(fiche) {
    const { addon } = fiche;
    const choisie = addon.dossier === selection;
    const travail = enCours.has(addon.dossier);
    const nombreModules = modulesDe(fiche).length;
    const ouvert = nombreModules > 0 && ouverts.has(addon.dossier);
    const choix = el('button', { classe: `catalogue-ligne__choix installes-ligne__choix${nombreModules > 0 ? ' installes-ligne__choix--modules' : ''}`, attrs: { type: 'button', title: addon.dossier, 'aria-pressed': String(choisie) } }, [
      creerImage(fiche.image, 'catalogue-ligne__vignette', () => el('span', { classe: 'catalogue-ligne__initiale', texte: initialeDe(fiche.titre) })),
      el('span', { classe: 'catalogue-ligne__texte' }, [
        el('strong', { classe: 'catalogue-ligne__nom', texte: libelleNomInstalle(fiche) }),
        el('span', { classe: 'catalogue-ligne__description', texte: libelleSousTitreLigne(fiche.description, nombreModules) }),
      ]),
      nombreModules > 0 ? el('span', { classe: `installes-chevron${ouvert ? ' ouvert' : ''}`, attrs: { 'aria-hidden': 'true' } }, [icone('chevron')]) : null,
      el('span', { classe: `catalogue-ligne__etat installes-etat catalogue-ligne__etat--${travail ? 'cours' : (fiche.miseAJour ? 'maj' : 'installe')}` }, [
        travail ? null : (fiche.miseAJour ? info() : coche()),
        el('span', { texte: libelleEtatInstalle(fiche, travail) }),
      ]),
    ]);
    if (nombreModules > 0) choix.setAttribute('aria-expanded', String(ouvert));
    choix.addEventListener('click', () => cliquerFiche(fiche));
    const actions = el('div', { classe: 'installes-actions' }, [
      creerBouton('catalogue-action', 'oeil', `Suivre ${fiche.titre}`, { actif: addon.suivi, presse: addon.suivi, desactive: occupe, quandClic: () => changerSuivi(addon) }),
      creerBouton('catalogue-action catalogue-action--maj', 'majHaut', `Mettre à jour ${fiche.titre}`, { actif: fiche.miseAJour, desactive: occupe || !fiche.miseAJour, quandClic: () => mettreAJour(fiche) }),
      creerBouton('catalogue-action catalogue-action--corbeille', 'corbeille', `Supprimer ${fiche.titre}`, { desactive: occupe, quandClic: () => supprimer(addon) }),
    ]);
    return el('li', { classe: `catalogue-ligne${choisie ? ' catalogue-ligne--choisie' : ''}`, attrs: { 'data-nom': addon.dossier } }, [choix, actions]);
  }

  /** Ligne de module d'un volet ouvert : nom et corbeille seulement ; un clic sur la ligne ne fait rien. */
  function creerLigneModule(fiche, module) {
    const { addon } = fiche;
    const nom = titreAddon({ titre: module.titre, dossier: module.dossier });
    const corbeille = creerBouton('installes-module__corbeille', 'corbeille', `Supprimer ${nom}`, {
      desactive: occupe,
      quandClic: () => supprimerModule(addon, module),
    });
    return el('li', { classe: 'catalogue-ligne installes-module', attrs: { 'data-nom': module.dossier } }, [
      el('span', { classe: 'installes-module__nom', texte: nom, attrs: { title: module.dossier } }),
      corbeille,
    ]);
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

  function rendreDetail(fiche) {
    const cle = fiche?.addon.dossier ?? null;
    const defilementAvant = selectionDetail === cle ? detailDefile.scrollTop : 0;
    selectionDetail = cle;
    if (!fiche) {
      remplacer(detailDefile);
      remplacer(detailPied);
      return;
    }
    const { addon, entree } = fiche;
    const images = Array.isArray(entree?.images) ? entree.images : [];
    const principale = imageChoisie !== null && images[imageChoisie] ? images[imageChoisie] : (fiche.image || images[0] || null);
    const repli = () => el('span', { classe: 'catalogue-detail__initiale', texte: initialeDe(fiche.titre) });
    const description = entree ? texteDescription(entree) : fiche.description;
    const enfants = [
      creerImage(principale, 'catalogue-detail__image', repli),
      el('div', { classe: 'catalogue-detail__entete' }, [
        creerImage(fiche.image, 'catalogue-detail__avatar', () => el('span', { classe: 'catalogue-ligne__initiale', texte: initialeDe(fiche.titre) })),
        el('div', { classe: 'catalogue-detail__identite' }, [
          el('strong', { classe: 'catalogue-detail__titre', texte: fiche.titre }),
          el('span', { classe: 'catalogue-detail__version', texte: ligneVersionInstalle(fiche) }),
          ...(addon.auteur ? [el('span', { classe: 'catalogue-detail__auteur', texte: `Auteur : ${addon.auteur}` })] : []),
        ]),
      ]),
      el('p', { classe: 'catalogue-detail__description', texte: description }),
    ];
    if (images.length >= 2) {
      enfants.push(el('div', { classe: 'catalogue-detail__miniatures' }, images.map((source, rang) => {
        const bouton = el('button', {
          classe: `catalogue-detail__miniature${source === principale ? ' catalogue-detail__miniature--active' : ''}`,
          attrs: { type: 'button', 'aria-label': `Image ${rang + 1}`, 'aria-pressed': String(source === principale) },
        }, [el('img', { attrs: { loading: 'lazy', alt: '', src: source } })]);
        bouton.addEventListener('click', () => {
          imageChoisie = rang;
          rendreDetail(fiche);
        });
        return bouton;
      })));
    }
    remplacer(detailDefile, ...enfants);
    const travail = enCours.has(addon.dossier);
    const boutonMaj = el('button', { classe: 'catalogue-detail__action catalogue-detail__action--maj', attrs: { type: 'button', title: `Mettre à jour ${fiche.titre}` } }, [
      icone(travail ? 'actualiser' : 'majHaut'),
      el('span', { texte: travail ? 'En cours' : 'Mettre à jour' }),
    ]);
    boutonMaj.disabled = occupe || !fiche.miseAJour;
    if (travail) boutonMaj.setAttribute('aria-busy', 'true');
    boutonMaj.addEventListener('click', () => mettreAJour(fiche));
    remplacer(
      detailPied,
      boutonMaj,
      creerBouton('catalogue-detail__action installes-detail__icone', 'oeil', `Suivre ${fiche.titre}`, { actif: addon.suivi, presse: addon.suivi, desactive: occupe, quandClic: () => changerSuivi(addon) }),
      creerBouton('catalogue-detail__action installes-detail__icone', 'corbeille', `Supprimer ${fiche.titre}`, { desactive: occupe, quandClic: () => supprimer(addon) }),
    );
    detailDefile.scrollTop = defilementAvant;
  }

  function rendre() {
    // Loader : visible tant qu'il n'y a rien à montrer (lecture en cours, sans erreur) ; même règle que le Catalogue.
    const attente = courantes === null && !derniereErreur;
    if (attente) {
      chargementAffiche = true;
      afficherChargement(zoneAttente, { etape: 'Lecture des addons' });
    } else if (chargementAffiche) {
      chargementAffiche = false;
      retirerChargement(zoneAttente, { echec: courantes === null });
    }
    if (courantes === null) {
      sousTitre.textContent = '';
      remplacer(listeUl);
      listeUl.style.height = '0px';
      lignes = [];
      plageCourante = { debut: -1, fin: -1 };
      remplacer(pastilles);
      fichesFiltrees = [];
      compteMaj.hidden = true;
      groupeMaj.hidden = true;
      pied.hidden = true;
      legende.hidden = true;
      rendreDetail(null);
      message.textContent = derniereErreur ? derniereErreur.message : '';
      message.hidden = message.textContent === '';
      return;
    }
    legende.hidden = false;
    const toutes = fichesInstalles(courantes.addons, catalogueCourant?.entrees, { bibliotheques: bascule.entree.checked });
    const categories = categoriesInstalles(toutes);
    if (!categories.some((categorie) => categorie.nom === categorieActive)) categorieActive = CATEGORIE_TOUTES;
    const misesAJour = compterMisesAJourInstalles(toutes);
    if (misesAJour < 1) majSeulement = false;
    const filtrees = filtrerInstalles(toutes, { categorie: categorieActive, requete: recherche.valeur(), majSeulement });
    fichesFiltrees = filtrees;
    if (reinitVolets) {
      reinitVolets = false;
      ouverts.clear();
      for (const dossier of dossiersOuvertsParRecherche(filtrees, recherche.valeur())) ouverts.add(dossier);
    }
    if (!filtrees.some((fiche) => fiche.addon.dossier === selection)) {
      selection = filtrees[0]?.addon.dossier ?? null;
      imageChoisie = null;
    }

    const problemes = libelleProblemes(courantes.problemes);
    const comptes = comptesInstalles(courantes.addons);
    sousTitre.textContent = libelleAddonsInstalles(comptes.total, comptes.suivis) + (problemes ? ` · ${problemes}` : '');
    const defilementPastilles = pastilles.scrollLeft;
    remplacer(pastilles, ...categories.map(creerPastille));
    pastilles.scrollLeft = defilementPastilles;
    if (categorieAAmener) {
      categorieAAmener = false;
      glisseurPastilles.amenerDansVue(pastilles.querySelector('.catalogue-pastille--active'));
    }
    compteMaj.textContent = libelleMisesAJour(misesAJour);
    compteMaj.setAttribute('aria-pressed', String(majSeulement));
    compteMaj.setAttribute('aria-label', majSeulement ? 'Afficher tous les addons' : `Afficher les ${libelleMisesAJour(misesAJour)}`);
    compteMaj.classList.toggle('catalogue-maj--active', majSeulement);
    compteMaj.hidden = misesAJour < 1;
    groupeMaj.hidden = misesAJour < 1;
    boutonToutMaj.disabled = occupe;

    calculerLignes();
    peindre(true);
    rendreDetail(filtrees.find((fiche) => fiche.addon.dossier === selection) ?? null);

    const vide = majSeulement && filtrees.length === 0 && !recherche.valeur().trim()
      ? MESSAGE_AUCUNE_MAJ
      : messageAddonsVide(courantes.addons.length, filtrees.length, recherche.valeur());
    message.textContent = vide || (filtrees.length === 0 ? 'Aucun addon dans cette catégorie.' : '');
    message.hidden = message.textContent === '';
  }

  return {
    racine,
    ouvrir: () => recharger(),
    mettreAJour({ addons, catalogue, erreurs }) {
      if (addons === courantes && catalogue === catalogueCourant && (erreurs.addons ?? null) === derniereErreur) return;
      courantes = addons;
      catalogueCourant = catalogue ?? null;
      derniereErreur = erreurs.addons ?? null;
      rendre();
    },
  };
}
