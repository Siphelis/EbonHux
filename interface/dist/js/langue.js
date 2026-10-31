import * as api from './api.js';
import { afficherChargement, retirerChargement } from './chargement.js';
import { el, remplacer, svg } from './dom.js';
import { icone } from './icones.js';
import { badgeLangue, libelleEtatLangue, pastilleElementLangue, piedLangue, sousTitreLangue, titreLangue } from './modeles.js';

const ICONES_ELEMENT = {
  donnees: [{ d: 'M3.5 7.5a2 2 0 0 1 2-2h4l2 2.5h7a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z' }],
  correctif: [{ d: 'M6.5 3.5h8l4 4v13h-12z' }, { d: 'M9.5 12h6M9.5 15.5h6' }],
  locale: [{ d: 'M3.5 7h9M17.5 7h3M3.5 17h3M11.5 17h9' }, { balise: 'circle', cx: 15, cy: 7, r: 2.5 }, { balise: 'circle', cx: 9, cy: 17, r: 2.5 }],
};

export function creerVueLangue(contexte) {
  let courante = null;
  let derniereErreur = null;
  let occupe = false;
  const sousTitre = el('p', { classe: 'sous-titre' });
  const badge = el('span', { classe: 'badge-langue-grand' });
  const titre = el('div', { classe: 'langue-titre' });
  const etat = el('div', { classe: 'langue-etat' });
  const bouton = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Revérifier la protection' });
  const lignes = el('ul', { classe: 'liste-defilante elements' });
  const pied = el('p', { classe: 'pied-langue' });
  const message = el('p', { classe: 'message-liste' });
  const zoneAttente = el('div', { classe: 'attente-ecran' });
  let chargementAffiche = false;

  bouton.addEventListener('click', async () => {
    occupe = true;
    rendre();
    try {
      contexte.definir('langue', await api.reverifierLangue());
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupe = false;
    rendre();
  });

  const racine = el('section', { classe: 'ecran langue' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: 'Langue' }), sousTitre]),
    el('div', { classe: 'carte carte-langue' }, [badge, el('div', { classe: 'langue-texte' }, [titre, etat]), bouton]),
    el('div', { classe: 'carte carte-elements' }, [el('h2', { texte: 'Éléments protégés' }), lignes, zoneAttente, message, pied]),
  ]);

  function ligneElement(element) {
    const modele = pastilleElementLangue(element);
    return el('li', { classe: 'element' }, [
      svg(ICONES_ELEMENT[element.id] ?? ICONES_ELEMENT.donnees, 'icone trait icone-element'),
      el('div', { classe: 'element-texte' }, [el('strong', { texte: element.titre }), el('span', { classe: 'secondaire', texte: element.detail })]),
      modele.valeur ? el('span', { classe: 'valeur-locale', texte: modele.valeur }) : null,
      el('span', { classe: `pastille-etat ${modele.ton}`, texte: modele.libelle }),
    ]);
  }

  // Loader (crâne + pourcentage) tant qu'il n'y a rien à montrer, comme le Catalogue.
  function majAttente() {
    if (courante === null && !derniereErreur) {
      chargementAffiche = true;
      afficherChargement(zoneAttente, { etape: 'Lecture de la langue' });
    } else if (chargementAffiche) {
      chargementAffiche = false;
      retirerChargement(zoneAttente, { echec: courante === null });
    }
  }

  function rendre() {
    bouton.disabled = occupe;
    bouton.textContent = occupe ? 'Vérification…' : 'Revérifier la protection';
    if (occupe) bouton.setAttribute('aria-busy', 'true');
    else bouton.removeAttribute('aria-busy');
    majAttente();
    if (courante === null) {
      sousTitre.textContent = '';
      badge.textContent = '';
      titre.textContent = '';
      remplacer(etat);
      remplacer(lignes);
      pied.textContent = '';
      message.textContent = derniereErreur ? derniereErreur.message : '';
      message.hidden = message.textContent === '';
      return;
    }
    message.hidden = true;
    sousTitre.textContent = sousTitreLangue(courante.etat);
    badge.textContent = badgeLangue(courante);
    titre.textContent = titreLangue(courante);
    remplacer(etat, icone('bouclier', courante.etat === 'protege' ? '' : 'alerte'), el('span', { texte: libelleEtatLangue(courante.etat) }));
    remplacer(lignes, ...courante.elements.map(ligneElement));
    pied.textContent = piedLangue(courante.etat);
  }

  return {
    racine,
    ouvrir: () => contexte.charger('langue'),
    mettreAJour({ langue, erreurs }) {
      if (langue === courante && (erreurs.langue ?? null) === derniereErreur) return;
      courante = langue;
      derniereErreur = erreurs.langue ?? null;
      rendre();
    },
  };
}
