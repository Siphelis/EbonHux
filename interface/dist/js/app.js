import * as api from './api.js';
import { creerBandeau } from './bandeau.js';
import { creerVueAddons } from './addons.js';
import { creerVueCatalogue } from './catalogue.js';
import { creerVueConnexion } from './connexion.js';
import { creerFlux } from './flux.js';
import { creerVueDiagnostic } from './diagnostic.js';
import { creerVueJouer } from './jouer.js';
import { figerAnimation } from './interrupteur.js';
import { creerVueLangue } from './langue.js';
import { creerVuePreparation } from './preparation.js';
import { boutonsCompte, intervalleInterrogation, motsDePasseManquants, pastille, preparationRequise, VERSION_CLIENT } from './modeles.js';
import { rendrePastille } from './pastille.js';
import { creerRail, PAGES } from './rail.js';
import { activerVerre } from './verre.js';

const reglages = api.reglages;
const donnees = { comptes: [], en_jeu: 0, addons: null, langue: null, catalogue: null, diagnostic: null, installation: null, erreurs: {} };
const CHARGEMENTS = { addons: api.addons, langue: api.langue, catalogue: api.catalogue, diagnostic: api.diagnostic, installation: api.installation };
const deselectionnes = new Set();
const vues = new Map();
const chargementsEnCours = new Map();
const appli = document.getElementById('appli');
activerVerre(appli);
const conteneurVue = document.getElementById('vue');
const conteneurPastille = document.getElementById('pastille');
let vueCourante = null;
let minuteur = null;
let selectionInitialisee = false;

const bandeau = creerBandeau();

// Tant que le jeu n'est pas configuré, les lectures qui en dépendent échouent : l'écran Préparation l'explique déjà.
function signaler(erreur) {
  if (erreur.code === 'jeu_non_configure' && appli.dataset.ecran === 'preparation') return;
  bandeau.montrer(erreur);
}
appli.append(...bandeau.racines);
document.getElementById('pied').textContent = VERSION_CLIENT;

const flux = creerFlux({ rafraichir, signaler: (cause) => signaler(api.erreurDe(cause)), signalerSucces: (texte) => bandeau.montrerSucces(texte) });
const contexte = { flux, deselectionnes, naviguer, rafraichirAffichage: appliquer, charger, definir, bandeau };
const rail = creerRail({ naviguer, ouvrirSite: flux.ouvrirSite });
appli.prepend(rail.racine);

const FABRIQUES = {
  jouer: () => creerVueJouer(contexte),
  connexion: () => creerVueConnexion(contexte),
  addons: () => creerVueAddons(contexte),
  catalogue: () => creerVueCatalogue(contexte),
  langue: () => creerVueLangue(contexte),
  diagnostic: () => creerVueDiagnostic(contexte),
  preparation: () => creerVuePreparation(contexte),
};

function appliquer() {
  rendrePastille(conteneurPastille, pastille(donnees.comptes, donnees.en_jeu));
  vueCourante?.mettreAJour(donnees, new Date());
}

async function lire(cle) {
  try {
    donnees[cle] = await CHARGEMENTS[cle]();
    donnees.erreurs[cle] = null;
  } catch (cause) {
    donnees.erreurs[cle] = api.erreurDe(cause);
    signaler(donnees.erreurs[cle]);
  }
  chargementsEnCours.delete(cle);
  appliquer();
}

function charger(cle) {
  if (!chargementsEnCours.has(cle)) chargementsEnCours.set(cle, lire(cle));
  return chargementsEnCours.get(cle);
}

function definir(cle, valeur) {
  donnees[cle] = valeur;
  donnees.erreurs[cle] = null;
  appliquer();
}

function naviguer(id, prendreLeFocus = true) {
  const page = PAGES.find((candidate) => candidate.id === id);
  if (!vues.has(id)) vues.set(id, FABRIQUES[id]());
  vueCourante = vues.get(id);
  appli.dataset.ecran = id;
  document.title = `${page.libelle} · EbonHux`;
  conteneurVue.replaceChildren(vueCourante.racine);
  rail.marquer(id);
  appliquer();
  vueCourante.ouvrir?.();
  if (prendreLeFocus) {
    const cible = conteneurVue.querySelector('h1') ?? conteneurVue;
    cible.tabIndex = -1;
    cible.focus({ preventScroll: true });
  }
}

async function rafraichir() {
  try {
    const reponse = await api.comptes();
    donnees.comptes = reponse.comptes;
    donnees.en_jeu = reponse.en_jeu;
    // Une seule fois par session : seul le compte par défaut démarre sélectionné pour « Tout lancer ».
    if (!selectionInitialisee && reponse.comptes.length > 1) {
      selectionInitialisee = true;
      reponse.comptes.filter((compte) => !compte.par_defaut).forEach((compte) => deselectionnes.add(compte.nom));
    }
  } catch (cause) {
    signaler(api.erreurDe(cause));
  }
  appliquer();
}

function planifier() {
  clearTimeout(minuteur);
  if (reglages.figer || document.visibilityState !== 'visible') return;
  minuteur = setTimeout(async () => {
    await rafraichir();
    planifier();
  }, intervalleInterrogation(donnees.comptes));
}

function ouvrirDialogueDemande() {
  if (reglages.dialogue === 'se-deconnecter') {
    const compte = donnees.comptes.find((candidat) => boutonsCompte(candidat).deconnecter.visible);
    if (compte) flux.demanderDeconnexion(compte);
  }
  if (reglages.dialogue === 'mot-de-passe-demande') {
    const [compte] = motsDePasseManquants(donnees.comptes);
    if (compte) flux.demanderMotDePasse(compte);
  }
}

document.addEventListener('visibilitychange', async () => {
  if (document.visibilityState === 'visible') await rafraichir();
  planifier();
});

function ecranDeDemarrage() {
  if (PAGES.some((page) => page.id === reglages.ecran)) return reglages.ecran;
  return preparationRequise(donnees.installation) ? 'preparation' : 'jouer';
}

await charger('installation');
const ecranDemande = ecranDeDemarrage();
conteneurVue.tabIndex = -1;
naviguer(ecranDemande, false);
await Promise.all([charger('addons'), charger('langue')]);
await rafraichir();
ouvrirDialogueDemande();
planifier();
if (reglages.action === 'preparer') vueCourante.lancerPreparation?.();
if (reglages.figer && reglages.anim) figerAnimation(conteneurVue.querySelector('.interrupteur'), reglages.anim, reglages.temps);
