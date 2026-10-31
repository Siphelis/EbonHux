import { el } from './dom.js';
import { icone } from './icones.js';
import { ouvrirDialogue } from './dialogues.js';
import { demandeSuppression } from './modeles.js';

// Boîte de confirmation avant suppression d'un addon (même composant que la déconnexion).
export function demanderSuppression(nom, appeler, modules = 0) {
  const demande = demandeSuppression(nom, appeler, modules);
  ouvrirDialogue({
    titreId: 'dialogue-titre',
    entete: el('div', { classe: 'dialogue-tete' }, [
      el('span', { classe: 'icone-titre' }, [icone('corbeille')]),
      el('h2', { texte: demande.titre, attrs: { id: 'dialogue-titre' } }),
    ]),
    contenu: [],
    actions: [
      { libelle: 'Annuler', annuler: true, surClic: demande.annuler },
      { libelle: demande.confirmer, icone: 'corbeille', surClic: demande.valider },
    ],
  });
}
