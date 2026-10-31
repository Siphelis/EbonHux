import { el } from './dom.js';
import { monterFlammes } from './flamme-canevas.js';
import { YEUX, RAPPORT } from '../img/embleme-crane-yeux.js';
import { suivreProgression, creerEstimation, lireDuree, ecrireDuree, libelleNombre } from './chargement-etat.js';

// Un bloc par conteneur : on garde le crâne et ses flammes d'une mise à jour à l'autre.
const blocs = new WeakMap();

function construire() {
  const image = el('img', { attrs: { src: 'img/embleme-crane.webp', alt: '' } });
  const canvas = el('canvas', { classe: 'chargement-flammes', attrs: { 'aria-hidden': 'true' } });
  const crane = el('span', { classe: 'chargement-crane' }, [image, canvas]);
  crane.style.aspectRatio = `${RAPPORT.largeur} / ${RAPPORT.hauteur}`;
  const etape = el('p', { classe: 'chargement-etape' });
  const nombre = el('p', { classe: 'chargement-nombre', attrs: { 'aria-hidden': 'true' } });
  const remplissage = el('span', { classe: 'chargement-remplissage' });
  const barre = el('div', { classe: 'chargement-barre', attrs: { role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-label': 'Chargement du catalogue' } }, [remplissage]);
  const noeud = el('div', { classe: 'chargement', attrs: { role: 'status', 'aria-live': 'polite', 'aria-atomic': 'true' } }, [crane, etape, nombre, barre]);
  return { noeud, image, canvas, etape, nombre, barre, remplissage, flammes: null, retire: false, enRetrait: false, minuterie: null, delai: null, estimation: null, debut: 0 };
}

// Monte le crâne quand le bloc est dans la page et l'image chargée (sinon canvas 0x0).
function brancher(bloc) {
  const essayer = async () => {
    if (bloc.retire) return;
    if (!bloc.noeud.isConnected) return void requestAnimationFrame(essayer);
    if (!bloc.image.complete) {
      try { await bloc.image.decode(); } catch { /* image absente : on monte quand même */ }
      if (bloc.retire) return;
    }
    const mouvementReduit = mouvementReduitActif;
    bloc.flammes = monterFlammes({ canvas: bloc.canvas, image: bloc.image, yeux: YEUX, mouvementReduit });
  };
  requestAnimationFrame(essayer);
}

const PAS_ESTIMATION_MS = 100;
const DELAI_RETRAIT_MS = 200;
const mouvementReduitActif = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
const maintenantMs = () => (typeof performance !== 'undefined' ? performance.now() : Date.now());
const stockageLocal = () => { try { return localStorage; } catch { return null; } };

function arreterMinuterie(bloc) {
  if (bloc.minuterie) clearInterval(bloc.minuterie);
  bloc.minuterie = null;
}

// Écrit le nombre et la barre. AFFICHAGE : « 42 % » sans signe ≈, pour l'estimation comme pour le réel.
// Attention : un nombre d'estimation est une ESTIMATION, pas une mesure (courbe du temps écoulé).
// L'absence de ≈ à l'écran est un choix du propriétaire (« 42% ça suffit ») ; l'honnêteté passe par ce
// commentaire. Seul un pourcentage réel porte aria-valuenow ; le libellé d'étape reste la voix d'état.
function poserNombre(bloc, valeur, reel) {
  bloc.nombre.textContent = libelleNombre(valeur);
  bloc.barre.style.setProperty('--chargement-avancement', `${valeur}%`);
  if (reel) bloc.barre.setAttribute('aria-valuenow', String(valeur));
  else bloc.barre.removeAttribute('aria-valuenow');
}

function poserIndetermine(bloc) {
  bloc.nombre.textContent = '';
  bloc.barre.removeAttribute('aria-valuenow');
  bloc.barre.style.removeProperty('--chargement-avancement');
}

// Retrait définitif et immédiat du nœud : minuterie et délai libérés, flammes arrêtées.
function demonter(conteneur, bloc) {
  if (blocs.get(conteneur) === bloc) blocs.delete(conteneur);
  arreterMinuterie(bloc);
  if (bloc.delai) clearTimeout(bloc.delai);
  bloc.delai = null;
  bloc.retire = true;
  if (bloc.flammes) bloc.flammes.arreter();
  bloc.flammes = null;
  bloc.noeud.remove();
}

// Mode : pourcent absent = estimation (minuterie interne) ; entier 0-100 = réel ; null = indéterminé.
export function afficherChargement(conteneur, { etape, pourcent } = {}) {
  let bloc = blocs.get(conteneur);
  // Un retrait est en cours (délai du « 100 % ») : on le finit tout de suite, puis on recrée.
  if (bloc && bloc.enRetrait) { demonter(conteneur, bloc); bloc = null; }
  if (bloc && (!bloc.noeud.isConnected || bloc.noeud.parentNode !== conteneur)) { demonter(conteneur, bloc); bloc = null; }
  if (!bloc) {
    bloc = construire();
    bloc.debut = maintenantMs();
    blocs.set(conteneur, bloc);
    conteneur.append(bloc.noeud);
    brancher(bloc);
  }
  bloc.etape.textContent = etape || '';
  const reel = typeof pourcent === 'number' && Number.isFinite(pourcent) && pourcent >= 0 && pourcent <= 100;
  const estime = pourcent === undefined;
  bloc.noeud.classList.toggle('chargement-indetermine', !reel && !estime);
  if (reel) {
    arreterMinuterie(bloc);
    poserNombre(bloc, Math.round(pourcent), true);
  } else if (estime) {
    // Une seule minuterie par zone : un 2e appel ne touche ni au temps ni à l'estimation.
    if (!bloc.minuterie) {
      if (!bloc.estimation) bloc.estimation = creerEstimation({ attenduMs: lireDuree(stockageLocal()), maintenant: maintenantMs });
      poserNombre(bloc, bloc.estimation.pourcent(), false);
      bloc.minuterie = setInterval(() => poserNombre(bloc, bloc.estimation.pourcent(), false), PAS_ESTIMATION_MS);
    }
  } else {
    arreterMinuterie(bloc);
    poserIndetermine(bloc);
  }
}

// Fin normale : mesure la durée réelle (gardée pour la prochaine estimation), « 100 % » (les données
// sont arrivées : ce 100 est vrai, il ne vient jamais de la courbe), puis retrait en <= 200 ms.
// { echec: true } : retrait direct, sans 100 % ni écriture de durée. Idempotent.
export function retirerChargement(conteneur, { echec = false } = {}) {
  const bloc = blocs.get(conteneur);
  if (!bloc) return;
  if (bloc.enRetrait) return;
  arreterMinuterie(bloc);
  if (echec) return demonter(conteneur, bloc);
  const stockage = stockageLocal();
  if (stockage) ecrireDuree(stockage, maintenantMs() - bloc.debut);
  bloc.enRetrait = true;
  bloc.noeud.classList.remove('chargement-indetermine');
  poserNombre(bloc, 100, bloc.barre.hasAttribute('aria-valuenow'));
  if (mouvementReduitActif()) return demonter(conteneur, bloc);
  bloc.delai = setTimeout(() => demonter(conteneur, bloc), DELAI_RETRAIT_MS);
}

export function suivreChargement(conteneur, lireProgression, { intervalle = 150 } = {}) {
  let arrete = false;
  const arretBoucle = suivreProgression(lireProgression, (etat) => {
    if (!arrete) afficherChargement(conteneur, etat);
  }, { intervalle });
  return () => {
    if (arrete) return;
    arrete = true;
    arretBoucle();
    retirerChargement(conteneur);
  };
}
