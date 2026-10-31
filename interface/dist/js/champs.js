import { el } from './dom.js';
import { icone } from './icones.js';

let compteur = 0;

export function creerChamp({ etiquette, indice, secret }) {
  compteur += 1;
  const identifiant = `champ-${compteur}`;
  const entree = el('input', {
    classe: 'saisie',
    attrs: {
      id: identifiant,
      type: secret ? 'password' : 'text',
      placeholder: indice || '',
      autocomplete: secret ? 'off' : 'username',
      spellcheck: 'false',
      autocapitalize: 'off',
    },
  });
  const zone = el('div', { classe: 'champ-saisie' }, [entree]);
  if (secret) {
    const oeil = el('button', { classe: 'oeil', attrs: { type: 'button', 'aria-label': 'Afficher le mot de passe', 'aria-pressed': 'false' } }, [icone('oeil')]);
    oeil.addEventListener('click', () => {
      const visible = entree.type === 'password';
      entree.type = visible ? 'text' : 'password';
      oeil.setAttribute('aria-pressed', String(visible));
      oeil.setAttribute('aria-label', visible ? 'Masquer le mot de passe' : 'Afficher le mot de passe');
    });
    zone.append(oeil);
  }
  const racine = el('label', { classe: 'champ', attrs: { for: identifiant } }, [el('span', { texte: etiquette }), zone]);
  return {
    racine,
    entree,
    valeur: () => entree.value,
    vider() {
      entree.value = '';
      if (secret) entree.type = 'password';
    },
  };
}
