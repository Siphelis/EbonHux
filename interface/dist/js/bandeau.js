import { el } from './dom.js';

function creerMessage({ role, classe, libelleFermer }) {
  const message = el('span', { classe: 'message' });
  const fermer = el('button', { attrs: { type: 'button', 'aria-label': libelleFermer }, texte: '✕' });
  const racine = el('div', { classe: `bandeau ${classe}`, attrs: { role } }, [message, fermer]);
  racine.hidden = true;
  fermer.addEventListener('click', () => {
    racine.hidden = true;
  });
  return {
    racine,
    montrer(texte) {
      message.textContent = texte;
      racine.hidden = false;
    },
  };
}

export function creerBandeau() {
  const erreur = creerMessage({ role: 'alert', classe: 'bandeau-erreur', libelleFermer: "Fermer le message d'erreur" });
  const succes = creerMessage({ role: 'status', classe: 'bandeau-succes', libelleFermer: 'Fermer le message' });
  return {
    racines: [erreur.racine, succes.racine],
    montrer(cause) {
      succes.racine.hidden = true;
      erreur.montrer(cause.message);
    },
    montrerSucces(texte) {
      erreur.racine.hidden = true;
      succes.montrer(texte);
    },
  };
}
