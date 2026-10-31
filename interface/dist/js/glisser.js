// Rangée horizontale défilable : cliquer-glisser (souris/stylet), balayage tactile natif (touch-action: pan-x),
// molette verticale convertie, flèches/Début/Fin entre boutons, fondu des bords par classes.
const CLASSE_GAUCHE = 'catalogue-pastilles--suite-gauche';
const CLASSE_DROITE = 'catalogue-pastilles--suite-droite';

export function rendreGlissable(conteneur, { seuil = 6 } = {}) {
  let id = null;
  let departX = 0;
  let departScroll = 0;
  let glisse = false;
  let capture = false;

  function majFondu() {
    const max = conteneur.scrollWidth - conteneur.clientWidth;
    conteneur.classList.toggle(CLASSE_GAUCHE, conteneur.scrollLeft > 1);
    conteneur.classList.toggle(CLASSE_DROITE, max > 1 && conteneur.scrollLeft < max - 1);
  }

  function amenerDansVue(element) {
    if (!element || !conteneur.contains(element)) return;
    const c = conteneur.getBoundingClientRect();
    const e = element.getBoundingClientRect();
    const marge = 28;
    if (e.left < c.left + marge) conteneur.scrollLeft += e.left - c.left - marge;
    else if (e.right > c.right - marge) conteneur.scrollLeft += e.right - c.right + marge;
    majFondu();
  }

  function bloquerClic(evt) {
    evt.stopPropagation();
    evt.preventDefault();
  }

  function fin(evt) {
    if (id === null || (evt && evt.pointerId !== id)) return;
    if (capture) {
      try { conteneur.releasePointerCapture(id); } catch { /* déjà relâché */ }
    }
    if (glisse) {
      conteneur.addEventListener('click', bloquerClic, { capture: true, once: true });
      setTimeout(() => conteneur.removeEventListener('click', bloquerClic, { capture: true }), 0);
    }
    id = null;
    glisse = false;
    capture = false;
    conteneur.classList.remove('catalogue-pastilles--glisse');
  }

  function bas(evt) {
    if (evt.pointerType === 'touch' || (evt.pointerType === 'mouse' && evt.button !== 0) || id !== null) return;
    id = evt.pointerId;
    departX = evt.clientX;
    departScroll = conteneur.scrollLeft;
    glisse = false;
  }

  function bouge(evt) {
    if (evt.pointerId !== id) return;
    const dx = evt.clientX - departX;
    if (!glisse) {
      if (Math.abs(dx) <= seuil) return;
      glisse = true;
      try { conteneur.setPointerCapture(id); capture = true; } catch { capture = false; }
      conteneur.classList.add('catalogue-pastilles--glisse');
    }
    conteneur.scrollLeft = departScroll - dx;
    evt.preventDefault();
  }

  function molette(evt) {
    if (evt.ctrlKey || Math.abs(evt.deltaY) <= Math.abs(evt.deltaX)) return;
    const max = conteneur.scrollWidth - conteneur.clientWidth;
    if (max <= 1) return;
    const cible = conteneur.scrollLeft + evt.deltaY;
    if ((evt.deltaY < 0 && conteneur.scrollLeft <= 0) || (evt.deltaY > 0 && conteneur.scrollLeft >= max)) return;
    evt.preventDefault();
    conteneur.scrollLeft = cible;
  }

  function touche(evt) {
    const boutons = [...conteneur.querySelectorAll('button')];
    const i = boutons.indexOf(document.activeElement);
    if (i < 0) return;
    let j = null;
    if (evt.key === 'ArrowRight') j = Math.min(boutons.length - 1, i + 1);
    else if (evt.key === 'ArrowLeft') j = Math.max(0, i - 1);
    else if (evt.key === 'Home') j = 0;
    else if (evt.key === 'End') j = boutons.length - 1;
    if (j === null) return;
    evt.preventDefault();
    boutons[j].focus({ preventScroll: true });
    amenerDansVue(boutons[j]);
  }

  function focus(evt) {
    if (evt.target !== conteneur) amenerDansVue(evt.target);
  }

  const observateur = typeof ResizeObserver === 'function' ? new ResizeObserver(majFondu) : null;
  observateur?.observe(conteneur);
  const mutations = typeof MutationObserver === 'function' ? new MutationObserver(majFondu) : null;
  mutations?.observe(conteneur, { childList: true });

  conteneur.addEventListener('pointerdown', bas);
  conteneur.addEventListener('pointermove', bouge);
  conteneur.addEventListener('pointerup', fin);
  conteneur.addEventListener('pointercancel', fin);
  conteneur.addEventListener('wheel', molette, { passive: false });
  conteneur.addEventListener('keydown', touche);
  conteneur.addEventListener('focusin', focus);
  conteneur.addEventListener('scroll', majFondu, { passive: true });
  majFondu();

  return {
    amenerDansVue,
    detruire() {
      observateur?.disconnect();
      mutations?.disconnect();
      conteneur.removeEventListener('pointerdown', bas);
      conteneur.removeEventListener('pointermove', bouge);
      conteneur.removeEventListener('pointerup', fin);
      conteneur.removeEventListener('pointercancel', fin);
      conteneur.removeEventListener('wheel', molette);
      conteneur.removeEventListener('keydown', touche);
      conteneur.removeEventListener('focusin', focus);
      conteneur.removeEventListener('scroll', majFondu);
      conteneur.classList.remove(CLASSE_GAUCHE, CLASSE_DROITE);
    },
  };
}
