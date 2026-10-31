import * as api from './api.js';
import { el } from './dom.js';
import { icone } from './icones.js';
import { creerChamp } from './champs.js';
import { creerInterrupteur } from './interrupteur.js';
import { ouvrirDialogue } from './dialogues.js';
import { libelleDeconnexion, libelleDialogueMotDePasse, libelleDialogueMotsDePasse, libelleLancerComptes, libelleMotDePasseChange, motsDePasseManquants, initialeDe } from './modeles.js';

const LIBELLE_ENREGISTRER = 'Enregistrer le mot de passe';
const AIDE_ENREGISTRER = 'Active-le pour ne plus le ressaisir.';

export function creerFlux({ rafraichir, signaler, signalerSucces = () => {} }) {
  async function executer(action) {
    let reussi = true;
    try {
      await action();
    } catch (cause) {
      reussi = false;
      signaler(cause);
    }
    await rafraichir();
    return reussi;
  }

  function ligneEnregistrement(compte) {
    const interrupteur = creerInterrupteur({
      libelle: LIBELLE_ENREGISTRER,
      actif: compte.enregistrer_mot_de_passe,
      quandChange: (actif) => executer(() => api.definirEnregistrement(compte.nom, actif)),
    });
    return el('div', { classe: 'ligne-interrupteur' }, [
      interrupteur.racine,
      el('div', {}, [el('div', { classe: 'libelle', texte: LIBELLE_ENREGISTRER }), el('div', { classe: 'aide', texte: AIDE_ENREGISTRER })]),
    ]);
  }

  function demanderDeconnexion(compte) {
    const modele = libelleDeconnexion(compte);
    ouvrirDialogue({
      titreId: 'dialogue-titre',
      entete: el('div', { classe: 'dialogue-tete' }, [
        el('span', { classe: 'icone-titre' }, [icone('cadenasOuvert')]),
        el('h2', { texte: modele.titre, attrs: { id: 'dialogue-titre' } }),
      ]),
      contenu: [
        el('p', { classe: 'texte', texte: modele.texte }),
        modele.precision ? el('p', { classe: 'note' }, [el('span', { classe: 'point' }), el('span', { texte: modele.precision })]) : null,
      ],
      actions: [
        { libelle: 'Annuler', annuler: true, surClic: () => {} },
        { libelle: 'Se déconnecter', icone: 'cadenasOuvert', surClic: () => executer(() => api.seDeconnecter(compte.nom)) },
      ],
    });
  }

  function ouvrirSaisie({ manquants, titre, sousTitre, libelleAction, iconeAction, envoyer, avecEnregistrement = true, messageVide = null, garderSiEchec = false }) {
    const champs = manquants.map((compte) => ({
      compte,
      champ: creerChamp({ etiquette: manquants.length > 1 ? `Mot de passe de ${compte.nom}` : 'Mot de passe', secret: true }),
    }));
    const vider = () => champs.forEach(({ champ }) => champ.vider());
    const avatar = manquants.length === 1 ? el('span', { classe: 'avatar', texte: initialeDe(manquants[0].nom) }) : null;
    ouvrirDialogue({
      titreId: 'dialogue-titre',
      entete: el('div', { classe: 'dialogue-tete' }, [
        avatar,
        el('div', {}, [el('h2', { texte: titre, attrs: { id: 'dialogue-titre' } }), el('p', { classe: 'sous-titre', texte: sousTitre })]),
      ]),
      contenu: champs.map(({ compte, champ }) => el('div', { classe: 'groupe' }, [champ.racine, avecEnregistrement ? ligneEnregistrement(compte) : null])),
      actions: [
        { libelle: 'Annuler', annuler: true, surClic: vider },
        {
          libelle: libelleAction,
          icone: iconeAction,
          surClic() {
            const vide = champs.find(({ champ }) => !champ.valeur());
            if (vide) {
              signaler({ code: 'saisie', message: messageVide ?? `Saisis le mot de passe de ${vide.compte.nom}.` });
              return false;
            }
            const saisies = new Map(champs.map(({ compte, champ }) => [compte.nom, champ.valeur()]));
            vider();
            const suite = envoyer(saisies);
            saisies.clear();
            return garderSiEchec ? suite : true;
          },
        },
      ],
    });
  }

  function demanderMotDePasse(compte, modeConnexion = false) {
    const modele = libelleDialogueMotDePasse(compte, modeConnexion);
    ouvrirSaisie({
      manquants: [compte],
      titre: modele.titre,
      sousTitre: modele.sousTitre,
      libelleAction: modele.action,
      iconeAction: modeConnexion ? null : 'lecture',
      envoyer: (saisies) => {
        const saisie = saisies.get(compte.nom);
        return executer(() => (modeConnexion ? api.enregistrerMotDePasse(compte.nom, saisie) : api.lancer(compte.nom, saisie)));
      },
    });
  }

  function changerMotDePasse(compte) {
    const modele = libelleDialogueMotDePasse(compte, 'changer');
    ouvrirSaisie({
      manquants: [compte],
      titre: modele.titre,
      sousTitre: modele.sousTitre,
      libelleAction: modele.action,
      iconeAction: null,
      avecEnregistrement: false,
      messageVide: 'Saisis un mot de passe.',
      garderSiEchec: true,
      envoyer: (saisies) => executer(async () => {
        await api.enregistrerMotDePasse(compte.nom, saisies.get(compte.nom));
        signalerSucces(libelleMotDePasseChange(compte));
      }),
    });
  }

  function demanderMotsDePasse(aLancer, manquants) {
    const modele = libelleDialogueMotsDePasse(manquants);
    ouvrirSaisie({
      manquants,
      titre: modele.titre,
      sousTitre: modele.sousTitre,
      libelleAction: libelleLancerComptes(aLancer.length),
      iconeAction: 'doubleLecture',
      envoyer: (saisies) => envoyerToutLancer(aLancer.map((compte) => ({ nom: compte.nom, mot_de_passe: saisies.get(compte.nom) ?? null }))),
    });
  }

  function envoyerToutLancer(liste) {
    return executer(async () => {
      const { resultats } = await api.toutLancer(liste);
      const echecs = resultats.filter((resultat) => !resultat.ok).map((resultat) => `${resultat.compte} : ${resultat.message}`);
      if (echecs.length) throw { code: 'lancement_partiel', message: echecs.join(' · ') };
    });
  }

  function lancerCompte(compte) {
    if (!compte.mot_de_passe_enregistre) {
      demanderMotDePasse(compte);
      return Promise.resolve();
    }
    return executer(() => api.lancer(compte.nom, null));
  }

  function toutLancer(aLancer) {
    const manquants = motsDePasseManquants(aLancer);
    if (manquants.length === 0) return envoyerToutLancer(aLancer.map((compte) => ({ nom: compte.nom, mot_de_passe: null })));
    demanderMotsDePasse(aLancer, manquants);
    return Promise.resolve();
  }

  return {
    demanderDeconnexion,
    demanderMotDePasse,
    changerMotDePasse,
    lancerCompte,
    toutLancer,
    definirEnregistrement: (compte, actif) => executer(() => api.definirEnregistrement(compte.nom, actif)),
    definirCompteParDefaut: (compte) => executer(() => api.definirCompteParDefaut(compte.nom)),
    ajouterCompte: (nom, motDePasse, enregistrer) => executer(() => api.ajouterCompte(nom, motDePasse, enregistrer)),
    ouvrirSite: () => executer(() => api.ouvrirSite()),
    executer,
  };
}
