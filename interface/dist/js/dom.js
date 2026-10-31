const ESPACE_SVG = 'http://www.w3.org/2000/svg';

export function el(balise, options = {}, enfants = []) {
  const noeud = document.createElement(balise);
  if (options.classe) noeud.className = options.classe;
  if (options.texte !== undefined) noeud.textContent = options.texte;
  for (const [nom, valeur] of Object.entries(options.attrs || {})) noeud.setAttribute(nom, valeur);
  for (const [evenement, action] of Object.entries(options.sur || {})) noeud.addEventListener(evenement, action);
  noeud.append(...enfants.filter(Boolean));
  return noeud;
}

export function svg(formes, classe) {
  const racine = document.createElementNS(ESPACE_SVG, 'svg');
  racine.setAttribute('viewBox', '0 0 24 24');
  racine.setAttribute('aria-hidden', 'true');
  racine.setAttribute('class', classe);
  for (const forme of formes) {
    const trace = document.createElementNS(ESPACE_SVG, forme.balise || 'path');
    for (const [nom, valeur] of Object.entries(forme)) {
      if (nom !== 'balise') trace.setAttribute(nom, valeur);
    }
    racine.append(trace);
  }
  return racine;
}

export function remplacer(noeud, ...enfants) {
  noeud.replaceChildren(...enfants.filter(Boolean));
}
