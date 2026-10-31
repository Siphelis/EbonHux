import { el, remplacer } from './dom.js';
import { icone } from './icones.js';
import {
  aLancer, boutonJouer, estLance, initialeDe, libelleAddons, libelleEnJeu, libelleEtatCourt, libelleLancement, libelleMotsDePasse,
  libelleResume, libelleSelection, motsDePasseManquants, selectionnes,
} from './modeles.js';

export function creerVueJouer(contexte) {
  const { flux } = contexte;

  const valeurCompte = el('div', { classe: 'valeur' });
  const etiquetteCompte = el('div', { classe: 'etiquette' });
  const detailCompte = el('div', { classe: 'detail' });
  const sectionCompte = el('div', { classe: 'dock-section' }, [icone('personne'), el('div', {}, [etiquetteCompte, valeurCompte]), detailCompte]);

  const valeurAddons = el('div', { classe: 'valeur' });
  const sectionAddons = el('div', { classe: 'dock-section' }, [
    icone('grille'),
    el('div', {}, [el('div', { classe: 'etiquette', texte: 'Addons' }), valeurAddons]),
  ]);

  const valeurLangue = el('div', { classe: 'valeur' });
  const sectionLangue = el('div', { classe: 'dock-section' }, [
    el('span', { classe: 'badge-langue', texte: 'FR' }),
    el('div', {}, [el('div', { classe: 'etiquette', texte: 'Langue' }), valeurLangue]),
  ]);

  const bouton = el('button', { classe: 'bouton bouton-jouer', attrs: { type: 'button' } });
  const legende = el('div', { classe: 'legende' });
  const action = el('div', { classe: 'action' }, [bouton, legende]);
  const dock = el('div', { classe: 'dock carte' }, [sectionCompte, sectionAddons, sectionLangue, action]);

  const suivi = el('div', { classe: 'suivi carte' });
  const menu = el('div', { classe: 'menu-selection carte', attrs: { role: 'group', 'aria-label': 'Comptes à lancer' } });
  menu.hidden = true;
  const racine = el('section', { classe: 'jouer' }, [suivi, menu, dock]);

  let actionBouton = () => {};
  bouton.addEventListener('click', () => actionBouton());
  document.addEventListener('click', (evenement) => {
    if (!menu.hidden && !menu.contains(evenement.target) && !evenement.target.closest('.selection')) menu.hidden = true;
  });

  function rendreSuivi(comptes) {
    const connectes = libelleResume(comptes);
    remplacer(suivi, 
      el('div', { classe: 'suivi-tete' }, [el('h2', { texte: 'Suivi des comptes' }), el('span', { texte: connectes })]),
      ...comptes.map((compte) => {
        const nom = el('div', { classe: 'zone-nom' }, [el('span', { texte: compte.nom })]);
        if (compte.etat === 'mot_de_passe_demande' || compte.etat === 'deconnecte') {
          const lien = el('button', { classe: 'lien', attrs: { type: 'button', 'data-repere': `saisir-mot-de-passe:${compte.nom}` }, texte: 'Saisir le mot de passe' });
          lien.addEventListener('click', () => flux.demanderMotDePasse(compte, true));
          nom.append(lien);
        }
        const lance = estLance(compte);
        // Lancable seulement si « connecté » (mot de passe su, rien d'autre à faire) : pour les états
        // qui affichent déjà le lien « Saisir le mot de passe » ci-dessus, la ligne reste un simple div
        // (une ligne cliquable ne peut pas contenir un autre bouton). Un badge « Connecté » ici ferait
        // croire à tort qu'on est déjà en jeu avec ce compte : le joueur sait déjà que ses infos sont enregistrées.
        const lancable = compte.etat === 'connecte';
        const droite = lance
          ? el('span', { classe: 'badge' }, [compte.etat === 'en_jeu' ? el('span', { classe: 'point' }) : null, el('span', { texte: libelleEtatCourt(compte) })])
          : lancable
            ? icone('lecture', 'icone-suivi-lancer')
            : null;
        const ligne = el(lancable ? 'button' : 'div', {
          classe: `suivi-ligne${lancable ? ' lancable' : ''}`,
          attrs: lancable ? { type: 'button', 'aria-label': `Jouer avec ${compte.nom}` } : {},
        }, [el('span', { classe: 'avatar', texte: initialeDe(compte.nom) }), nom, droite]);
        if (lancable) ligne.addEventListener('click', () => flux.lancerCompte(compte));
        if (compte.etat === 'lancement') {
          ligne.classList.add('avec-barre');
          ligne.append(el('div', { classe: 'progression', attrs: { role: 'progressbar', 'aria-valuenow': compte.progression ?? 0, 'aria-valuemin': 0, 'aria-valuemax': 100 } }, [progressionBarre(compte.progression)]));
        }
        return ligne;
      }),
    );
  }

  function progressionBarre(valeur) {
    const barre = el('span');
    barre.style.width = `${valeur ?? 0}%`;
    return barre;
  }

  function rendreMenu(comptes) {
    remplacer(menu, 
      ...comptes.map((compte) => {
        const case_ = el('input', { classe: 'case', attrs: { type: 'checkbox' } });
        case_.checked = !contexte.deselectionnes.has(compte.nom);
        case_.addEventListener('change', () => {
          if (case_.checked) contexte.deselectionnes.delete(compte.nom);
          else contexte.deselectionnes.add(compte.nom);
          contexte.rafraichirAffichage();
        });
        return el('label', {}, [case_, el('span', { texte: compte.nom })]);
      }),
    );
  }

  function rendreCompte(comptes) {
    detailCompte.textContent = '';
    if (comptes.length > 1) {
      etiquetteCompte.textContent = 'Comptes';
      const choisis = selectionnes(comptes, contexte.deselectionnes);
      const selection = el('button', { classe: 'selection', attrs: { type: 'button', 'aria-haspopup': 'true', 'aria-expanded': String(!menu.hidden) } }, [
        el('span', { texte: libelleSelection(choisis.length, comptes.length) }),
        icone('chevron', 'trait'),
      ]);
      selection.addEventListener('click', () => {
        menu.hidden = !menu.hidden;
        selection.setAttribute('aria-expanded', String(!menu.hidden));
      });
      remplacer(valeurCompte, selection);
      return;
    }
    etiquetteCompte.textContent = 'Compte';
    if (comptes.length === 0) {
      remplacer(valeurCompte, el('span', { texte: 'Aucun' }));
      return;
    }
    const compte = comptes[0];
    if (compte.etat === 'en_jeu') {
      remplacer(valeurCompte, el('span', { texte: compte.nom }), el('span', { classe: 'point' }));
      detailCompte.textContent = libelleEnJeu(compte.en_jeu_depuis_s ?? 0);
      return;
    }
    const texte = compte.etat === 'lancement' ? libelleLancement(compte.progression) : libelleEtatCourt(compte);
    remplacer(valeurCompte, el('span', { texte: compte.nom }), el('span', { classe: 'badge', texte }));
  }

  return {
    racine,
    mettreAJour({ comptes, addons, langue, installation }) {
      const plusieurs = comptes.length > 1;
      racine.classList.toggle('plusieurs', plusieurs);
      suivi.hidden = !plusieurs;
      if (plusieurs) {
        rendreSuivi(comptes);
        rendreMenu(comptes);
      } else {
        menu.hidden = true;
      }
      rendreCompte(comptes);
      valeurAddons.textContent = libelleAddons(addons?.suivis ?? 0);
      remplacer(valeurLangue, el('span', { texte: langue?.nom ?? '' }), langue?.etat === 'protege' ? icone('bouclier') : null);

      const modele = boutonJouer(comptes, contexte.deselectionnes, installation);
      bouton.disabled = !modele.actif;
      remplacer(bouton, 
        modele.point ? el('span', { classe: 'point' }) : null,
        modele.action === 'lancer' ? icone('lecture') : null,
        modele.action === 'tout_lancer' ? icone('doubleLecture') : null,
        el('span', { texte: modele.libelle }),
      );
      const lanceables = aLancer(comptes, contexte.deselectionnes);
      legende.textContent = plusieurs ? libelleMotsDePasse(motsDePasseManquants(lanceables).length) : '';
      actionBouton = () => {
        if (modele.action === 'ajouter') contexte.naviguer('connexion');
        else if (modele.action === 'preparation') contexte.naviguer('preparation');
        else if (modele.action === 'tout_lancer') flux.toutLancer(lanceables);
        else if (modele.action === 'lancer') flux.lancerCompte(modele.compte ?? comptes[0]);
      };
    },
  };
}
