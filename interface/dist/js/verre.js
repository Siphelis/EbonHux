// Effet « verre » sans backdrop-filter : calcule, pour chaque élément en verre,
// sa position dans la fenêtre afin que la couche floutée (voir composants.css)
// reproduise exactement le fond qui se trouve derrière lui.
const SELECTEUR = '.carte, .pastille, .bandeau, .onglets, .recherche, .dialogue';
const RATIO_FOND = 1200 / 675;

export function activerVerre(appli) {
  let prevu = false;

  function recalculer() {
    prevu = false;
    const cadre = appli.getBoundingClientRect();
    const l = cadre.width;
    const h = cadre.height;
    const echelle = Math.max(l / 1200, h / 675);
    const fondL = 1200 * echelle;
    const fondH = (1200 / RATIO_FOND) * echelle;
    const racine = appli.style;
    racine.setProperty('--fenetre-l', `${l}px`);
    racine.setProperty('--fenetre-h', `${h}px`);
    racine.setProperty('--fond-l', `${fondL}px`);
    racine.setProperty('--fond-h', `${fondH}px`);
    racine.setProperty('--fond-dx', `${(l - fondL) / 2}px`);
    racine.setProperty('--fond-dy', `${(h - fondH) / 2}px`);
    for (const noeud of appli.ownerDocument.querySelectorAll(SELECTEUR)) {
      const boite = noeud.getBoundingClientRect();
      noeud.style.setProperty('--verre-x', `${cadre.left - boite.left}px`);
      noeud.style.setProperty('--verre-y', `${cadre.top - boite.top}px`);
      noeud.style.setProperty('--verre-top', `${boite.top}px`);
      noeud.style.setProperty('--verre-left', `${boite.left}px`);
      noeud.style.setProperty('--verre-w', `${boite.width}px`);
      noeud.style.setProperty('--verre-h', `${boite.height}px`);
    }
  }

  function planifier() {
    if (prevu) return;
    prevu = true;
    requestAnimationFrame(recalculer);
  }

  new MutationObserver(planifier).observe(appli.ownerDocument.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'hidden', 'data-ecran'] });
  for (const evenement of ['animationend', 'transitionend', 'scroll']) document.addEventListener(evenement, planifier, true);
  window.addEventListener('resize', planifier);
  planifier();
}
