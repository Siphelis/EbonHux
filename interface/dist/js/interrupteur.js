import { el } from './dom.js';
import { icone } from './icones.js';

export function creerInterrupteur({ libelle, actif, quandChange, simple = false }) {
  const entree = el('input', { attrs: { type: 'checkbox', role: 'switch', 'aria-label': libelle } });
  entree.checked = actif;
  const curseur = el('span', { classe: 'curseur' }, simple ? [] : [icone('cadenasOuvert', 'cadenas-ouvert'), icone('cadenasFerme', 'cadenas-ferme')]);
  const points = el('span', { classe: 'points' }, [el('i'), el('i'), el('i'), el('i')]);
  const piste = el('span', { classe: 'piste' }, [curseur, el('span', { classe: 'onde' }), simple ? null : points]);
  const racine = el('label', { classe: simple ? 'interrupteur simple' : 'interrupteur' }, [entree, piste]);

  const animer = (classe) => {
    racine.classList.remove('activation', 'desactivation');
    void racine.offsetWidth;
    racine.classList.add(classe);
  };
  racine.addEventListener('animationend', (evenement) => {
    if (evenement.target.closest('.interrupteur') === racine) racine.classList.remove('activation', 'desactivation');
  });
  entree.addEventListener('change', () => {
    animer(entree.checked ? 'activation' : 'desactivation');
    quandChange(entree.checked);
  });

  return {
    racine,
    entree,
    mettreAJour(valeur) {
      if (entree.checked !== valeur) entree.checked = valeur;
    },
  };
}

// Essais seulement (?figer=1&anim=activer|desactiver&temps=<ms>) : joue le changement d'état puis
// met toutes les animations et transitions en pause à l'instant demandé, pour en prendre une capture.
export function figerAnimation(racine, anim, temps) {
  const entree = racine.querySelector('input');
  const activer = anim === 'activer';
  entree.checked = !activer;
  void racine.offsetWidth;
  entree.checked = activer;
  racine.classList.remove('activation', 'desactivation');
  racine.classList.add(activer ? 'activation' : 'desactivation');
  for (const animation of racine.getAnimations({ subtree: true })) {
    animation.pause();
    animation.currentTime = temps;
  }
}
