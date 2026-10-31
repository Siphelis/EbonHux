import { el, remplacer } from './dom.js';
import { icone } from './icones.js';

export function rendrePastille(conteneur, modele) {
  const chevron = icone('chevron', 'chevron');
  if (modele.type === 'un') {
    remplacer(conteneur, 
      el('span', { classe: 'avatar', texte: modele.initiale }),
      el('span', { classe: 'nom', texte: modele.nom }),
      modele.enJeu ? el('span', { classe: 'point' }) : null,
      icone(modele.verrouFerme ? 'cadenasFerme' : 'cadenasOuvert', 'verrou'),
      chevron,
    );
    return;
  }
  if (modele.type === 'plusieurs') {
    remplacer(conteneur, 
      el('span', { classe: 'avatars' }, modele.initiales.map((initiale) => el('span', { classe: 'avatar', texte: initiale }))),
      el('span', { classe: 'nom', texte: modele.texte }),
      chevron,
    );
    return;
  }
  remplacer(conteneur, el('span', { classe: 'nom', texte: modele.texte }), chevron);
}
