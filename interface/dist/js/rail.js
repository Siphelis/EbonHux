import { el } from './dom.js';
import { icone } from './icones.js';
import { monterFlammes } from './flamme-canevas.js';
import { YEUX, RAPPORT } from '../img/embleme-crane-yeux.js';

export const ECRANS = [
  { id: 'jouer', libelle: 'Jouer', icone: 'lecture' },
  { id: 'connexion', libelle: 'Connexion', icone: 'personne' },
  { id: 'addons', libelle: 'Addons', icone: 'grille' },
  { id: 'langue', libelle: 'Langue', icone: null },
  { id: 'diagnostic', libelle: 'Diagnostic', icone: 'pouls' },
];

export const PAGES = [
  ...ECRANS.map((ecran) => ({ id: ecran.id, libelle: ecran.libelle, rail: ecran.id })),
  { id: 'catalogue', libelle: 'Catalogue', rail: 'addons' },
  { id: 'preparation', libelle: 'Préparation', rail: 'jouer' },
];

function crane(enfants) {
  const noeud = el('span', { classe: 'rail-crane' }, enfants);
  noeud.style.aspectRatio = `${RAPPORT.largeur} / ${RAPPORT.hauteur}`;
  return noeud;
}

export function creerRail({ naviguer, ouvrirSite }) {
  const boutons = new Map();
  const bouton = (ecran) => {
    const contenu = ecran.icone ? icone(ecran.icone) : el('span', { classe: 'rail-langue', texte: 'FR' });
    const noeud = el('button', { classe: 'rail-bouton', attrs: { type: 'button', 'aria-label': ecran.libelle, title: ecran.libelle } }, [contenu]);
    noeud.addEventListener('click', () => naviguer(ecran.id));
    boutons.set(ecran.id, noeud);
    return noeud;
  };
  const [jouer, connexion, addons, langue, diagnostic] = ECRANS.map(bouton);

  const embleme = el('button', { classe: 'rail-embleme', attrs: { type: 'button', 'aria-label': 'Accueil' } }, [
    crane([
      el('img', { attrs: { src: 'img/embleme-crane.webp', alt: '' } }),
      el('canvas', { classe: 'rail-flammes', attrs: { 'aria-hidden': 'true' } }),
    ]),
  ]);
  // Les flammes démarrent quand le rail est dans la page (la taille du canvas vient de celle de l'image).
  const brancher = () => {
    if (!embleme.isConnected) return requestAnimationFrame(brancher);
    monterFlammes({ canvas: embleme.querySelector('canvas'), image: embleme.querySelector('img'), yeux: YEUX });
  };
  requestAnimationFrame(brancher);
  embleme.addEventListener('click', () => naviguer('jouer'));

  const site = el('button', { classe: 'rail-bouton', attrs: { type: 'button', 'aria-label': 'Site du projet', title: 'Site du projet' } }, [icone('globe')]);
  site.addEventListener('click', ouvrirSite);

  const racine = el('nav', { classe: 'rail', attrs: { 'aria-label': 'Navigation principale' } }, [
    el('div', { classe: 'rail-haut' }, [embleme, jouer, connexion, addons, langue]),
    el('div', { classe: 'rail-bas' }, [site, diagnostic]),
  ]);

  return {
    racine,
    marquer(page) {
      const actif = PAGES.find((candidate) => candidate.id === page)?.rail;
      for (const [id, noeud] of boutons) {
        if (id === actif) noeud.setAttribute('aria-current', 'page');
        else noeud.removeAttribute('aria-current');
      }
    },
  };
}
