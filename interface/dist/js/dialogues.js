import { el } from './dom.js';
import { icone } from './icones.js';

const FOCALISABLES = 'button:not(:disabled), input:not(:disabled), [href], [tabindex]:not([tabindex="-1"])';

export function ouvrirDialogue({ entete, contenu, actions, titreId }) {
  const origine = document.activeElement;
  const repere = origine?.dataset?.repere ?? null;
  const conteneur = document.getElementById('dialogues');
  const appli = document.getElementById('appli');

  const boite = el('div', { classe: 'dialogue', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': titreId } }, [
    entete,
    ...contenu,
    el('div', { classe: 'actions' }, actions.map((action) => creerBouton(action, fermer))),
  ]);
  const fond = el('div', { classe: 'dialogue-fond' }, [boite]);

  function fermer() {
    fond.remove();
    appli.inert = false;
    const cible = origine?.isConnected ? origine : retrouver(repere);
    cible?.focus();
  }

  const annuler = actions.find((action) => action.annuler);
  fond.addEventListener('keydown', (evenement) => {
    if (evenement.key === 'Escape') {
      evenement.preventDefault();
      if (annuler) annuler.surClic();
      fermer();
      return;
    }
    if (evenement.key !== 'Tab') return;
    const cibles = [...boite.querySelectorAll(FOCALISABLES)];
    const premiere = cibles[0];
    const derniere = cibles[cibles.length - 1];
    if (evenement.shiftKey && document.activeElement === premiere) {
      evenement.preventDefault();
      derniere.focus();
    } else if (!evenement.shiftKey && document.activeElement === derniere) {
      evenement.preventDefault();
      premiere.focus();
    }
  });

  appli.inert = true;
  conteneur.append(fond);
  (boite.querySelector('input.saisie') || boite.querySelector('.actions .bouton')).focus();
  return { fermer };
}

// Le panneau derrière la fenêtre peut être reconstruit pendant qu'elle est ouverte : on retrouve alors l'élément équivalent par son repère.
function retrouver(repere) {
  if (!repere) return null;
  return [...document.querySelectorAll('[data-repere]')].find((noeud) => noeud.dataset.repere === repere) ?? null;
}

function creerBouton(action, fermer) {
  const bouton = el('button', { classe: 'bouton', attrs: { type: 'button' } }, [
    action.icone ? icone(action.icone) : null,
    el('span', { texte: action.libelle }),
  ]);
  bouton.addEventListener('click', async () => {
    if (action.annuler) {
      action.surClic();
      fermer();
      return;
    }
    const garder = await action.surClic();
    if (garder !== false) fermer();
  });
  return bouton;
}
