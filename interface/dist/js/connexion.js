import { el, remplacer } from './dom.js';
import { icone } from './icones.js';
import { creerChamp } from './champs.js';
import { creerInterrupteur } from './interrupteur.js';
import {
  aLancer, boutonsCompte, initialeDe, libelleDerniereConnexion, libelleEtat, libelleEtatCourt, libelleMotsDePasse, libelleResume,
  libelleSousLigne, libelleToutLancer, motsDePasseManquants, noteCompte, selectionnes,
} from './modeles.js';

const LIBELLE_ENREGISTRER = 'Enregistrer le mot de passe';
const SOUS_TITRE_UN = 'Ton compte reste connecté tant que tu le souhaites.';
const SOUS_TITRE_PLUSIEURS = "Chaque compte garde sa connexion. Tu restes connecté jusqu'à ce que tu te déconnectes.";

function barre(valeur) {
  const remplissage = el('span');
  remplissage.style.width = `${valeur ?? 0}%`;
  return el('div', { classe: 'progression', attrs: { role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-valuenow': valeur ?? 0 } }, [remplissage]);
}

function creerCarteUnCompte({ flux }) {
  let courant = null;
  const avatar = el('span', { classe: 'avatar avatar-grand' });
  const nom = el('span');
  const badge = el('span', { classe: 'badge' });
  const secret = el('div', { classe: 'compte-etat-secret' });
  const etat = el('dd');
  const derniere = el('dd');
  const aide = el('div', { classe: 'aide', texte: 'Le mot de passe sera demandé à chaque lancement.' });
  const interrupteur = creerInterrupteur({ libelle: LIBELLE_ENREGISTRER, actif: true, quandChange: (actif) => flux.definirEnregistrement(courant, actif) });
  const bloc = el('div', { classe: 'bloc-interrupteur' }, [el('div', {}, [el('div', { texte: LIBELLE_ENREGISTRER }), aide]), interrupteur.racine]);
  const lancer = el('button', { classe: 'bouton', attrs: { type: 'button' } });
  const deconnecter = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Se déconnecter' });
  lancer.addEventListener('click', () => flux.lancerCompte(courant));
  deconnecter.addEventListener('click', () => flux.demanderDeconnexion(courant));
  const changer = el('button', { classe: 'bouton changer-mdp-carte', attrs: { type: 'button' }, texte: 'Changer le mot de passe' });
  changer.addEventListener('click', () => flux.changerMotDePasse(courant));
  const note = el('div', { classe: 'note' });

  const racine = el('div', { classe: 'carte carte-compte defilante' }, [
    el('h2', { texte: 'Compte' }),
    el('div', { classe: 'compte-tete' }, [avatar, el('div', {}, [el('div', { classe: 'compte-nom' }, [nom, badge]), secret])]),
    el('dl', { classe: 'faits' }, [el('dt', { texte: 'État' }), etat, el('dt', { texte: 'Dernière connexion' }), derniere]),
    bloc,
    el('div', { classe: 'actions-compte' }, [lancer, deconnecter, changer]),
    note,
  ]);

  return {
    racine,
    mettreAJour(compte, maintenant) {
      courant = compte;
      const boutons = boutonsCompte(compte);
      avatar.textContent = initialeDe(compte.nom);
      nom.textContent = compte.nom;
      badge.textContent = libelleEtat(compte.etat);
      remplacer(secret, icone(compte.mot_de_passe_enregistre ? 'cadenasFerme' : 'cadenasOuvert'), el('span', { texte: libelleSousLigne(compte) }));
      etat.textContent = libelleEtatCourt(compte);
      derniere.textContent = libelleDerniereConnexion(compte.derniere_connexion, maintenant);
      interrupteur.mettreAJour(compte.enregistrer_mot_de_passe);
      bloc.dataset.actif = String(compte.enregistrer_mot_de_passe);
      remplacer(lancer, icone('lecture'), el('span', { texte: 'Lancer le jeu' }));
      lancer.disabled = !boutons.lancer.actif;
      deconnecter.hidden = !boutons.deconnecter.visible;
      deconnecter.disabled = !boutons.deconnecter.actif;
      changer.hidden = !boutons.changerMotDePasse.visible;
      const modeleNote = noteCompte(compte);
      remplacer(note, icone(modeleNote.verrouFerme ? 'cadenasFerme' : 'cadenasOuvert'), el('span', { texte: modeleNote.texte }));
    },
  };
}

function creerLigne({ flux, deselectionnes, rafraichirAffichage }) {
  let courant = null;
  const choix = el('input', { classe: 'case', attrs: { type: 'checkbox' } });
  const defaut = el('input', { classe: 'radio', attrs: { type: 'radio', name: 'compte-defaut' } });
  const avatar = el('span', { classe: 'avatar' });
  const nom = el('span');
  const badge = el('span', { classe: 'badge' });
  const sousLigne = el('div', { classe: 'sous-ligne' });
  const changer = el('button', { classe: 'bouton petit changer-mdp', attrs: { type: 'button' }, texte: 'Changer le mot de passe' });
  const identite = el('div', { classe: 'identite' }, [el('div', { classe: 'nom' }, [nom, badge]), sousLigne, el('label', { classe: 'compte-defaut' }, [defaut, el('span', { texte: 'Compte par défaut' })]), changer]);
  const interrupteur = creerInterrupteur({ libelle: LIBELLE_ENREGISTRER, actif: true, quandChange: (actif) => flux.definirEnregistrement(courant, actif) });
  const lancer = el('button', { classe: 'bouton', attrs: { type: 'button' } });
  const deconnecter = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Se déconnecter' });
  const progression = el('div', { classe: 'progression-zone' });
  const racine = el('li', { classe: 'ligne-compte' }, [choix, avatar, identite, interrupteur.racine, el('div', { classe: 'actions-ligne' }, [lancer, deconnecter])]);

  choix.addEventListener('change', () => {
    if (choix.checked) deselectionnes.delete(courant.nom);
    else deselectionnes.add(courant.nom);
    rafraichirAffichage();
  });
  defaut.addEventListener('change', () => {
    if (defaut.checked) flux.definirCompteParDefaut(courant);
  });
  lancer.addEventListener('click', () => flux.lancerCompte(courant));
  deconnecter.addEventListener('click', () => flux.demanderDeconnexion(courant));
  changer.addEventListener('click', () => flux.changerMotDePasse(courant));

  return {
    racine,
    mettreAJour(compte) {
      courant = compte;
      const boutons = boutonsCompte(compte);
      choix.checked = !deselectionnes.has(compte.nom);
      choix.setAttribute('aria-label', `Sélectionner ${compte.nom}`);
      defaut.checked = compte.par_defaut === true;
      defaut.setAttribute('aria-label', `Compte par défaut : ${compte.nom}`);
      avatar.textContent = initialeDe(compte.nom);
      avatar.classList.toggle('terne', compte.etat === 'mot_de_passe_demande' || compte.etat === 'deconnecte');
      nom.textContent = compte.nom;
      remplacer(badge, compte.etat === 'en_jeu' ? el('span', { classe: 'point' }) : null, el('span', { texte: libelleEtatCourt(compte) }));
      remplacer(sousLigne, icone('personne'), el('span', { texte: libelleSousLigne(compte) }));
      if (compte.etat === 'lancement') {
        remplacer(progression, barre(compte.progression));
        if (!progression.isConnected) identite.append(progression);
      } else {
        progression.remove();
      }
      interrupteur.mettreAJour(compte.enregistrer_mot_de_passe);
      lancer.classList.toggle('carre', boutons.lancer.forme === 'icone');
      remplacer(lancer, icone('lecture'), boutons.lancer.forme === 'texte' ? el('span', { texte: 'Lancer' }) : null);
      lancer.setAttribute('aria-label', `Lancer ${compte.nom}`);
      lancer.disabled = !boutons.lancer.actif;
      deconnecter.hidden = !boutons.deconnecter.visible;
      deconnecter.setAttribute('aria-label', `Se déconnecter de ${compte.nom}`);
      changer.hidden = !boutons.changerMotDePasse.visible;
      changer.setAttribute('aria-label', `Changer le mot de passe de ${compte.nom}`);
    },
  };
}

function creerCarteListe(contexte) {
  const { flux } = contexte;
  const resume = el('div', { classe: 'resume' });
  const toutLancer = el('button', { classe: 'bouton', attrs: { type: 'button' } });
  const legendeMdp = el('div');
  const toutSelectionner = el('input', { classe: 'case', attrs: { type: 'checkbox' } });
  const lignes = el('ul', { classe: 'lignes' });
  const lignesParNom = new Map();
  let aLancerCourant = [];
  let choisisCourants = 0;
  let comptesCourants = [];

  toutLancer.addEventListener('click', () => {
    if (choisisCourants === 1 && aLancerCourant.length === 1) flux.lancerCompte(aLancerCourant[0]);
    else flux.toutLancer(aLancerCourant);
  });
  toutSelectionner.addEventListener('change', () => {
    if (toutSelectionner.checked) contexte.deselectionnes.clear();
    else comptesCourants.forEach((compte) => contexte.deselectionnes.add(compte.nom));
    contexte.rafraichirAffichage();
  });

  const racine = el('div', { classe: 'carte carte-liste defilante' }, [
    el('div', { classe: 'liste-tete' }, [
      el('div', {}, [
        el('h2', { texte: 'Comptes' }),
        resume,
        el('label', { classe: 'tout-selectionner' }, [toutSelectionner, el('span', { texte: 'Tout sélectionner' })]),
      ]),
      el('div', { classe: 'tout-lancer' }, [toutLancer, legendeMdp]),
    ]),
    el('div', { classe: 'legende-colonne', texte: 'Mot de passe enregistré' }),
    lignes,
    el('div', { classe: 'note liste-note' }, [
      icone('cadenasOuvert'),
      el('span', { texte: "Se déconnecter oublie le mot de passe enregistré. L'interrupteur décide s'il est enregistré." }),
    ]),
  ]);

  return {
    racine,
    mettreAJour(comptes) {
      comptesCourants = comptes;
      resume.textContent = libelleResume(comptes);
      const choisis = selectionnes(comptes, contexte.deselectionnes);
      aLancerCourant = aLancer(comptes, contexte.deselectionnes);
      choisisCourants = choisis.length;
      toutSelectionner.checked = choisis.length === comptes.length;
      toutSelectionner.indeterminate = choisis.length > 0 && choisis.length < comptes.length;
      remplacer(toutLancer, icone(choisis.length === 1 ? 'lecture' : 'doubleLecture'), el('span', { texte: libelleToutLancer(choisis.length) }));
      toutLancer.disabled = aLancerCourant.length === 0;
      legendeMdp.textContent = libelleMotsDePasse(motsDePasseManquants(aLancerCourant).length);

      const noms = new Set(comptes.map((compte) => compte.nom));
      for (const [cle, ligne] of lignesParNom) {
        if (!noms.has(cle)) {
          ligne.racine.remove();
          lignesParNom.delete(cle);
        }
      }
      comptes.forEach((compte, rang) => {
        if (!lignesParNom.has(compte.nom)) lignesParNom.set(compte.nom, creerLigne(contexte));
        const ligne = lignesParNom.get(compte.nom);
        ligne.mettreAJour(compte);
        if (lignes.children[rang] !== ligne.racine) lignes.insertBefore(ligne.racine, lignes.children[rang] ?? null);
      });
    },
  };
}

function creerCarteAjout({ flux, bandeau }) {
  const identifiant = creerChamp({ etiquette: 'Identifiant', indice: 'Nom de compte' });
  const motDePasse = creerChamp({ etiquette: 'Mot de passe', secret: true });
  const aide = el('p', { classe: 'aide', texte: 'Avec deux comptes, la liste et « Tout lancer » apparaissent.' });
  const interrupteur = creerInterrupteur({ libelle: LIBELLE_ENREGISTRER, actif: true, quandChange: () => {} });
  const enregistrer = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Enregistrer le compte' });
  const annuler = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Annuler' });

  const vider = () => {
    identifiant.vider();
    motDePasse.vider();
    interrupteur.mettreAJour(true);
  };
  annuler.addEventListener('click', vider);
  enregistrer.addEventListener('click', async () => {
    const nom = identifiant.valeur().trim();
    const secret = motDePasse.valeur();
    if (!nom || !secret) {
      bandeau.montrer({ code: 'saisie', message: nom ? 'Saisis un mot de passe.' : 'Saisis un identifiant.' });
      return;
    }
    const demande = flux.ajouterCompte(nom, secret, interrupteur.entree.checked);
    motDePasse.vider();
    if (await demande) vider();
  });

  const racine = el('div', { classe: 'carte carte-ajout defilante' }, [
    el('h2', { texte: 'Ajouter un compte' }),
    aide,
    identifiant.racine,
    motDePasse.racine,
    el('div', { classe: 'ligne-interrupteur' }, [
      interrupteur.racine,
      el('div', {}, [el('div', { classe: 'libelle', texte: LIBELLE_ENREGISTRER }), el('div', { classe: 'aide', texte: 'Sinon, il sera demandé à chaque lancement.' })]),
    ]),
    el('div', { classe: 'actions-ajout' }, [enregistrer, annuler]),
  ]);
  return { racine, afficherAide: (visible) => { aide.hidden = !visible; } };
}

export function creerVueConnexion(contexte) {
  const sousTitre = el('p', { classe: 'sous-titre' });
  const ajout = creerCarteAjout(contexte);
  const unCompte = creerCarteUnCompte(contexte);
  const liste = creerCarteListe(contexte);
  const vide = el('div', { classe: 'carte carte-compte' }, [el('h2', { texte: 'Compte' }), el('p', { classe: 'texte-2', texte: "Aucun compte pour l'instant. Ajoute-en un avec le formulaire." })]);
  const emplacement = el('div', { classe: 'emplacement' });
  const racine = el('section', { classe: 'connexion' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: 'Connexion' }), sousTitre]),
    el('div', { classe: 'cartes' }, [emplacement, ajout.racine]),
  ]);
  let mode = null;

  return {
    racine,
    mettreAJour({ comptes }, maintenant) {
      const nouveau = comptes.length > 1 ? 'liste' : comptes.length === 1 ? 'un' : 'vide';
      if (nouveau !== mode) {
        mode = nouveau;
        remplacer(emplacement, { liste: liste.racine, un: unCompte.racine, vide }[nouveau]);
        racine.classList.toggle('plusieurs', nouveau === 'liste');
        sousTitre.textContent = nouveau === 'liste' ? SOUS_TITRE_PLUSIEURS : SOUS_TITRE_UN;
        ajout.afficherAide(nouveau === 'un');
      }
      if (nouveau === 'liste') liste.mettreAJour(comptes);
      if (nouveau === 'un') unCompte.mettreAJour(comptes[0], maintenant);
    },
  };
}
