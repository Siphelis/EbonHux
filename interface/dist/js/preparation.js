import * as api from './api.js';
import { creerChamp } from './champs.js';
import { ouvrirDialogue } from './dialogues.js';
import { el, remplacer } from './dom.js';
import { icone } from './icones.js';
import {
  cartesPreparation, DELAI_OUVERTURE_MS, libelleStatutEtape, libelleWine, MESSAGE_DISTRIBUTION, modeLogiciels, peutPreparer,
  SOUS_TITRE_PREPARATION, TEXTE_CONFIRMATION_INSTALLATION, TEXTE_LOGICIELS, TEXTE_OCCUPE_INSTALLATION, TEXTE_OCCUPE_PREPARATION,
  TEXTE_PREPARATION, TEXTE_PRET, TITRE_CONFIRMATION_INSTALLATION, TITRE_PREPARATION, TITRE_PRET, validerCheminJeu,
} from './modeles.js';

const OPTIONS_PARCOURIR = { directory: true, multiple: false, title: 'Choisir le dossier du jeu' };

function creerCarte(id) {
  const numero = el('span', { classe: 'numero' });
  const titre = el('h2');
  const pastille = el('span');
  const detail = el('p', { classe: 'detail' });
  const corps = el('div', { classe: 'corps' });
  const racine = el('article', { classe: 'carte etape', attrs: { 'data-etape': id } }, [
    el('header', {}, [numero, titre]),
    pastille,
    detail,
    corps,
  ]);
  return { racine, numero, titre, pastille, detail, corps };
}

export function creerVuePreparation(contexte) {
  const ouvrirParcourir = window.__TAURI__?.dialog?.open;
  let courante = null;
  let etaitPret = null;
  let occupe = null;
  let messageChamp = null;
  let dossierAffiche = null;
  let etapesInstallation = [];
  const cartes = new Map(['jeu', 'logiciels', 'preparation'].map((id) => [id, creerCarte(id)]));

  const champ = creerChamp({ etiquette: 'Chemin du dossier du jeu', indice: '/chemin/du/jeu' });
  const erreurChamp = el('p', { classe: 'erreur-champ', attrs: { role: 'alert' } });
  erreurChamp.hidden = true;
  champ.entree.setAttribute('aria-describedby', 'erreur-dossier');
  erreurChamp.id = 'erreur-dossier';
  const parcourir = ouvrirParcourir ? el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Parcourir' }) : null;
  const valider = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Valider' });
  cartes.get('jeu').corps.append(champ.racine, erreurChamp, el('div', { classe: 'boutons' }, [parcourir, valider]));

  const texteLogiciels = el('p', { classe: 'texte', texte: TEXTE_LOGICIELS });
  const etatLogiciels = el('p', { classe: 'etat-logiciels' });
  const messageDistribution = el('p', { classe: 'note alerte-note', texte: MESSAGE_DISTRIBUTION });
  const listeEtapes = el('ul', { classe: 'etapes-installation' });
  const installer = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Installer' });
  const reverifier = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Revérifier' });
  cartes.get('logiciels').corps.append(texteLogiciels, etatLogiciels, messageDistribution, listeEtapes, el('div', { classe: 'boutons' }, [installer, reverifier]));

  const textePreparation = el('p', { classe: 'texte', texte: TEXTE_PREPARATION });
  const occupation = el('p', { classe: 'occupation', texte: TEXTE_OCCUPE_PREPARATION });
  const preparer = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Préparer' });
  cartes.get('preparation').corps.append(textePreparation, occupation, el('div', { classe: 'boutons' }, [preparer]));

  const ouvrirLauncher = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Ouvrir le launcher' });
  const prete = el('div', { classe: 'carte prete' }, [
    el('span', { classe: 'coche-grande', texte: '✓' }),
    el('div', { classe: 'prete-texte' }, [el('strong', { texte: TITRE_PRET }), el('span', { texte: TEXTE_PRET })]),
    ouvrirLauncher,
  ]);
  prete.hidden = true;

  const contenu = el('div', { classe: 'contenu' }, [el('div', { classe: 'etapes' }, [...cartes.values()].map((carte) => carte.racine)), prete]);
  contenu.hidden = true;
  const racine = el('section', { classe: 'ecran preparation' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: TITRE_PREPARATION }), el('p', { classe: 'sous-titre', texte: SOUS_TITRE_PREPARATION })]),
    contenu,
  ]);

  function montrerErreurChamp(texte) {
    messageChamp = texte;
    erreurChamp.textContent = texte ?? '';
    erreurChamp.hidden = texte === null;
    champ.entree.toggleAttribute('aria-invalid', texte !== null);
  }

  function marquerOccupe(bouton, enCours, libelle, libelleOccupe) {
    bouton.textContent = enCours ? libelleOccupe : libelle;
    if (enCours) bouton.setAttribute('aria-busy', 'true');
    else bouton.removeAttribute('aria-busy');
  }

  function rendreEtapes() {
    listeEtapes.hidden = etapesInstallation.length === 0;
    remplacer(
      listeEtapes,
      ...etapesInstallation.map((etape) => el('li', {}, [
        el('span', { classe: 'marque', texte: '✓' }),
        el('span', { texte: etape.description }),
        el('span', { classe: 'secondaire', texte: libelleStatutEtape(etape.statut) }),
      ])),
    );
  }

  function rendreLogiciels(mode) {
    const enInstallation = occupe === 'logiciels';
    const enVerification = occupe === 'verification';
    etatLogiciels.hidden = mode !== 'installe' && !enInstallation;
    etatLogiciels.textContent = enInstallation ? TEXTE_OCCUPE_INSTALLATION : libelleWine(courante.wine);
    etatLogiciels.classList.toggle('fait', mode === 'installe' && !enInstallation);
    messageDistribution.hidden = mode !== 'non_prise_en_charge' || enInstallation;
    installer.hidden = mode !== 'installable';
    installer.disabled = enInstallation;
    marquerOccupe(installer, enInstallation, 'Installer', 'Installation…');
    reverifier.hidden = mode !== 'non_prise_en_charge';
    reverifier.disabled = enVerification;
    marquerOccupe(reverifier, enVerification, 'Revérifier', 'Vérification…');
  }

  function rendre() {
    const lecture = courante;
    const occupeDossier = occupe === 'dossier';
    const enPreparation = occupe === 'preparation';
    for (const modele of cartesPreparation(lecture)) {
      const carte = cartes.get(modele.id);
      carte.racine.dataset.etat = modele.etat;
      carte.numero.textContent = modele.etat === 'fait' ? '✓' : String(modele.numero);
      carte.titre.textContent = modele.titre;
      carte.pastille.className = `pastille-etat ${modele.ton}`.trim();
      carte.pastille.textContent = modele.libelle;
      carte.detail.textContent = modele.detail;
    }
    const dossier = lecture?.dossier_jeu ?? null;
    if (dossier !== dossierAffiche) {
      dossierAffiche = dossier;
      if (dossier !== null) champ.entree.value = dossier;
    }
    montrerErreurChamp(messageChamp);
    valider.disabled = occupeDossier;
    marquerOccupe(valider, occupeDossier, 'Valider', 'Vérification…');
    if (parcourir) parcourir.disabled = occupeDossier;
    rendreLogiciels(modeLogiciels(lecture));
    rendreEtapes();
    preparer.disabled = enPreparation || !peutPreparer(lecture);
    marquerOccupe(preparer, enPreparation, 'Préparer', 'Préparation…');
    occupation.hidden = !enPreparation;
    prete.hidden = !lecture.pret;
    contenu.hidden = false;
  }

  async function choisirDossier() {
    let choix = null;
    try {
      choix = await ouvrirParcourir(OPTIONS_PARCOURIR);
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
      return;
    }
    if (typeof choix !== 'string' || choix === '') return;
    champ.entree.value = choix;
    montrerErreurChamp(null);
    valider.focus();
  }

  async function validerDossier() {
    const chemin = champ.valeur().trim();
    const probleme = validerCheminJeu(chemin);
    montrerErreurChamp(probleme);
    if (probleme) return;
    occupe = 'dossier';
    rendre();
    try {
      contexte.definir('installation', await api.definirDossierJeu(chemin));
      await Promise.all([contexte.charger('addons'), contexte.charger('langue')]);
    } catch (cause) {
      const erreur = api.erreurDe(cause);
      if (erreur.code === 'dossier_jeu_invalide') montrerErreurChamp(erreur.message);
      contexte.bandeau.montrer(erreur);
    }
    occupe = null;
    rendre();
  }

  async function installerLogiciels() {
    occupe = 'logiciels';
    etapesInstallation = [];
    rendre();
    try {
      const reponse = await api.installerLogiciels();
      etapesInstallation = reponse.etapes;
      await contexte.charger('installation');
      if (reponse.message) contexte.bandeau.montrerSucces(reponse.message);
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupe = null;
    rendre();
  }

  function demanderInstallation() {
    ouvrirDialogue({
      titreId: 'dialogue-titre',
      entete: el('div', { classe: 'dialogue-tete' }, [
        el('span', { classe: 'icone-titre' }, [icone('cadenasFerme')]),
        el('h2', { texte: TITRE_CONFIRMATION_INSTALLATION, attrs: { id: 'dialogue-titre' } }),
      ]),
      contenu: [el('p', { classe: 'texte', texte: TEXTE_CONFIRMATION_INSTALLATION })],
      actions: [
        { libelle: 'Annuler', annuler: true, surClic: () => {} },
        { libelle: 'Continuer', surClic: () => { installerLogiciels(); } },
      ],
    });
  }

  async function reverifierLogiciels() {
    occupe = 'verification';
    rendre();
    await contexte.charger('installation');
    occupe = null;
    rendre();
  }

  async function preparerJeu() {
    occupe = 'preparation';
    rendre();
    try {
      contexte.definir('installation', await api.preparerJeu());
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupe = null;
    rendre();
  }

  function programmerOuverture() {
    setTimeout(() => {
      if (racine.isConnected) contexte.naviguer('jouer');
    }, DELAI_OUVERTURE_MS);
  }

  valider.addEventListener('click', validerDossier);
  parcourir?.addEventListener('click', choisirDossier);
  champ.entree.addEventListener('keydown', (evenement) => {
    if (evenement.key === 'Enter') validerDossier();
  });
  installer.addEventListener('click', demanderInstallation);
  reverifier.addEventListener('click', reverifierLogiciels);
  preparer.addEventListener('click', preparerJeu);
  ouvrirLauncher.addEventListener('click', () => contexte.naviguer('jouer'));

  return {
    racine,
    lancerPreparation: preparerJeu,
    ouvrir: () => contexte.charger('installation'),
    mettreAJour({ installation }) {
      if (installation === null || installation === courante) return;
      courante = installation;
      rendre();
      if (installation.pret && etaitPret === false) programmerOuverture();
      etaitPret = installation.pret;
    },
  };
}
