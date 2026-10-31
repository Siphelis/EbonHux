import * as api from './api.js';
import { afficherChargement, retirerChargement } from './chargement.js';
import { el, remplacer, svg } from './dom.js';
import {
  cleLigneJournal, elementsSysteme, filtrerJournal, heureLocale, initialeDe, libelleCompteLigne, libelleFiltreTous, libelleJournal, libelleRapportExporte,
  libelleResultat, messageJournalVide, plusieursComptesDiagnostic, sousTitreDiagnostic, tonResultat,
} from './modeles.js';

const AVERTISSEMENT = [{ d: 'M12 4.5l9 15.5H3z' }, { d: 'M12 10v4.5M12 17.2v.1' }];
const COLONNES_COMPLETES = 'colonnes-compte';

export function creerVueDiagnostic(contexte) {
  const fermes = new Set();
  let courant = null;
  let derniereErreur = null;
  let filtre = null;
  let occupe = false;

  const sousTitre = el('p', { classe: 'sous-titre' });
  const systeme = el('div', { classe: 'carte systeme' });
  const communA = el('p', { classe: 'legende-commun', texte: 'Commun à tous les comptes' });
  const resume = el('span', { classe: 'resume' });
  const exporter = el('button', { classe: 'bouton', attrs: { type: 'button' }, texte: 'Exporter le rapport' });
  const filtres = el('div', { classe: 'filtres', attrs: { role: 'group', 'aria-label': 'Filtrer le journal par compte' } });
  const entetes = el('div', { classe: 'ligne-journal entetes' });
  const lignes = el('ul', { classe: 'liste-defilante lignes-journal' });
  const message = el('p', { classe: 'message-liste' });
  const zoneAttente = el('div', { classe: 'attente-ecran' });
  let chargementAffiche = false;

  const journal = el('div', { classe: 'carte journal' }, [
    el('div', { classe: 'journal-tete' }, [
      el('h2', { texte: 'Journal' }),
      resume,
      el('span', { classe: 'note-rapport', texte: 'Les chemins personnels sont masqués dans le rapport.' }),
      exporter,
    ]),
    filtres,
    entetes,
    el('div', { classe: 'zone-liste' }, [lignes, zoneAttente, message]),
  ]);
  const racine = el('section', { classe: 'ecran diagnostic' }, [
    el('div', { classe: 'titre' }, [el('h1', { texte: 'Diagnostic' }), sousTitre]),
    systeme,
    communA,
    journal,
  ]);

  exporter.addEventListener('click', async () => {
    occupe = true;
    rendreExport();
    try {
      const { chemin } = await api.exporterRapport();
      contexte.bandeau.montrerSucces(libelleRapportExporte(chemin));
    } catch (cause) {
      contexte.bandeau.montrer(api.erreurDe(cause));
    }
    occupe = false;
    rendreExport();
  });

  function rendreExport() {
    exporter.disabled = occupe;
    exporter.textContent = occupe ? 'Export en cours…' : 'Exporter le rapport';
    if (occupe) exporter.setAttribute('aria-busy', 'true');
    else exporter.removeAttribute('aria-busy');
  }

  function puceFiltre(libelle, valeur) {
    const bouton = el('button', { classe: 'filtre', attrs: { type: 'button', 'aria-pressed': String(filtre === valeur) }, texte: libelle });
    bouton.addEventListener('click', () => {
      filtre = valeur;
      rendre();
    });
    return bouton;
  }

  function celluleCompte(ligne) {
    if (ligne.compte === null) return el('span', { classe: 'secondaire', texte: libelleCompteLigne(null) });
    return el('span', { classe: 'compte-journal' }, [el('span', { classe: 'avatar', texte: initialeDe(ligne.compte) }), el('span', { texte: ligne.compte })]);
  }

  function creerLigne(ligne, avecCompte) {
    const erreur = ligne.resultat === 'erreur' || ligne.resultat === 'avertissement';
    const cle = cleLigneJournal(ligne);
    const ouverte = erreur && !fermes.has(cle);
    const cellules = [
      el('span', { classe: 'heure', texte: heureLocale(ligne.heure) }),
      avecCompte ? celluleCompte(ligne) : null,
      el('span', { classe: 'etape' }, [erreur ? svg(AVERTISSEMENT, 'icone trait icone-erreur') : null, el('span', { classe: 'texte-coupe', texte: ligne.etape, attrs: { title: ligne.etape } })]),
      el('span', { classe: `pastille-resultat ${tonResultat(ligne.resultat)}`, texte: libelleResultat(ligne.resultat) }),
    ];
    const classes = `ligne-journal ${avecCompte ? COLONNES_COMPLETES : ''}`;
    if (!erreur) return el('li', { classe: 'entree' }, [el('div', { classe: classes }, cellules)]);
    const bouton = el('button', { classe: `${classes} ouvrable`, attrs: { type: 'button', 'aria-expanded': String(ouverte) } }, cellules);
    bouton.addEventListener('click', () => {
      if (fermes.has(cle)) fermes.delete(cle);
      else fermes.add(cle);
      rendre();
    });
    const bloc = el('li', { classe: `entree erreur ${ligne.resultat}${ouverte ? ' ouverte' : ''}` }, [bouton]);
    if (ouverte && ligne.detail) bloc.append(el('pre', { classe: 'detail-journal', texte: ligne.detail }));
    return bloc;
  }

  // Loader (crâne + pourcentage) tant qu'il n'y a rien à montrer, comme le Catalogue.
  function majAttente() {
    if (courant === null && !derniereErreur) {
      chargementAffiche = true;
      afficherChargement(zoneAttente, { etape: 'Lecture du diagnostic' });
    } else if (chargementAffiche) {
      chargementAffiche = false;
      retirerChargement(zoneAttente, { echec: courant === null });
    }
  }

  function rendre() {
    rendreExport();
    majAttente();
    if (courant === null) {
      sousTitre.textContent = '';
      remplacer(systeme);
      remplacer(lignes);
      remplacer(filtres);
      entetes.replaceChildren();
      resume.textContent = '';
      message.textContent = derniereErreur ? derniereErreur.message : '';
      message.hidden = message.textContent === '';
      return;
    }
    const plusieurs = plusieursComptesDiagnostic(courant.comptes);
    if (filtre !== null && !courant.comptes.includes(filtre)) filtre = null;
    sousTitre.textContent = sousTitreDiagnostic(courant.comptes.length);
    communA.hidden = !plusieurs;
    remplacer(systeme, ...elementsSysteme(courant.systeme).map(({ etiquette, valeur }) => el('div', { classe: 'cellule-systeme' }, [el('span', { classe: 'etiquette', texte: etiquette }), el('strong', { texte: valeur })])));
    resume.textContent = libelleJournal(courant.etapes, courant.erreurs, courant.avertissements ?? 0, courant.comptes.length);
    filtres.hidden = !plusieurs;
    remplacer(filtres, ...(plusieurs ? [puceFiltre(libelleFiltreTous(courant.comptes.length), null), ...courant.comptes.map((nom) => puceFiltre(nom, nom))] : []));
    entetes.className = `ligne-journal entetes ${plusieurs ? COLONNES_COMPLETES : ''}`;
    remplacer(entetes, ...['Heure', plusieurs ? 'Compte' : null, 'Étape', 'Résultat'].filter(Boolean).map((texte) => el('span', { texte })));
    const visibles = filtrerJournal(courant.lignes, filtre);
    remplacer(lignes, ...visibles.map((ligne) => creerLigne(ligne, plusieurs)));
    const vide = messageJournalVide(courant.lignes.length, visibles.length);
    message.textContent = vide;
    message.hidden = vide === '';
  }

  return {
    racine,
    ouvrir: () => contexte.charger('diagnostic'),
    mettreAJour({ diagnostic, erreurs }) {
      if (diagnostic === courant && (erreurs.diagnostic ?? null) === derniereErreur) return;
      courant = diagnostic;
      derniereErreur = erreurs.diagnostic ?? null;
      rendre();
    },
  };
}
