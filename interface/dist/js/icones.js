import { svg } from './dom.js';

const PLEIN = 'icone';
const TRAIT = 'icone trait';

const FORMES = {
  lecture: { classe: PLEIN, formes: [{ d: 'M7 4.5v15l12-7.5z' }] },
  personne: { classe: PLEIN, formes: [{ d: 'M12 4a4 4 0 1 1 0 8 4 4 0 0 1 0-8zM4.5 20.5c0-3.8 3.3-6.3 7.5-6.3s7.5 2.5 7.5 6.3z' }] },
  grille: {
    classe: PLEIN,
    formes: [
      { d: 'M4.5 4.5h5v5h-5zM14.5 4.5h5v5h-5zM4.5 14.5h5v5h-5zM14.5 14.5h5v5h-5z', stroke: 'currentColor', 'stroke-width': 2.4, 'stroke-linejoin': 'round' },
    ],
  },
  globe: {
    classe: TRAIT,
    formes: [
      { balise: 'circle', cx: 11, cy: 13, r: 8 },
      { d: 'M3 13h16M11 5c-2.4 2.2-3.4 5-3.4 8s1 5.8 3.4 8M11 5c2.4 2.2 3.4 5 3.4 8s-1 5.8-3.4 8M16 3h5v5M21 3l-6 6' },
    ],
  },
  pouls: { classe: TRAIT, formes: [{ d: 'M3 12h4l2.5-7 4 14 2.5-7H21' }] },
  cadenasFerme: {
    classe: PLEIN,
    formes: [{ balise: 'rect', x: 6, y: 11, width: 12, height: 9.5, rx: 2.5 }, { d: 'M8.6 11V8a3.4 3.4 0 0 1 6.8 0v3', fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linecap': 'round' }],
  },
  cadenasOuvert: {
    classe: PLEIN,
    formes: [{ balise: 'rect', x: 6, y: 11, width: 12, height: 9.5, rx: 2.5 }, { d: 'M8.6 11V8a3.4 3.4 0 0 1 6.6-1.2', fill: 'none', stroke: 'currentColor', 'stroke-width': 2, 'stroke-linecap': 'round' }],
  },
  oeil: { classe: TRAIT, formes: [{ d: 'M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z' }, { balise: 'circle', cx: 12, cy: 12, r: 3 }] },
  corbeille: { classe: TRAIT, formes: [{ d: 'M4 7h16M9 7V4.5h6V7' }, { d: 'M6 7l1 13h10l1-13' }, { d: 'M10 11v6M14 11v6' }] },
  chevron: { classe: TRAIT, formes: [{ d: 'M6 9l6 6 6-6' }] },
  chevronGauche: { classe: TRAIT, formes: [{ d: 'M14.5 6l-6 6 6 6' }] },
  chevronDroite: { classe: TRAIT, formes: [{ d: 'M9.5 6l6 6-6 6' }] },
  installer: { classe: TRAIT, formes: [{ d: 'M12 4v10M7.8 10.2l4.2 4.2 4.2-4.2' }, { d: 'M5 15.5V19h14v-3.5' }] },
  majHaut: { classe: TRAIT, formes: [{ d: 'M12 19V6M7 10.5L12 5.5l5 5' }] },
  actualiser: { classe: TRAIT, formes: [{ d: 'M20 11a8 8 0 0 0-14.3-4.5M5 3v4h4' }, { d: 'M4 13a8 8 0 0 0 14.3 4.5M19 21v-4h-4' }] },
  bouclier: {
    classe: PLEIN,
    formes: [{ d: 'M12 3l7 2.8v5.7c0 4.3-3 7.7-7 9.5-4-1.8-7-5.2-7-9.5V5.8z' }, { d: 'M8.8 12l2.3 2.3 4.2-4.4', class: 'coche-bouclier' }],
  },
  doubleLecture: { classe: PLEIN, formes: [{ d: 'M3.5 6v12l8-6zM12.5 6v12l8-6z' }] },
};

export function icone(nom, classeSupplementaire = '') {
  const modele = FORMES[nom];
  return svg(modele.formes, `${modele.classe} ${classeSupplementaire}`.trim());
}
