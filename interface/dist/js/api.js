import * as mock from './mock.js';

const SERVICE_TAURI = window.__TAURI__?.core;

export const reglages = SERVICE_TAURI ? { ecran: null, dialogue: null, figer: false, anim: null, temps: 0, deplier: null, action: null } : mock.reglages;

export function erreurDe(cause) {
  if (cause && typeof cause === 'object' && 'message' in cause) {
    return { code: cause.code ?? 'inconnue', message: String(cause.message) };
  }
  return { code: 'inconnue', message: String(cause) };
}

function appeler(commande, args = {}) {
  return SERVICE_TAURI ? SERVICE_TAURI.invoke('appeler', { commande, args }) : mock.appeler(commande, args);
}

export const comptes = () => appeler('comptes');
export const ajouterCompte = (nom, motDePasse, enregistrer) => appeler('ajouter_compte', { nom, mot_de_passe: motDePasse, enregistrer });
export const enregistrerMotDePasse = (compte, motDePasse) => appeler('enregistrer_mot_de_passe', { compte, mot_de_passe: motDePasse });
export const definirEnregistrement = (compte, actif) => appeler('definir_enregistrement', { compte, actif });
export const definirCompteParDefaut = (compte) => appeler('definir_compte_par_defaut', { compte });
export const seDeconnecter = (compte) => appeler('se_deconnecter', { compte });
export const lancer = (compte, motDePasse) => appeler('lancer', { compte, mot_de_passe: motDePasse });
export const toutLancer = (liste) => appeler('tout_lancer', { comptes: liste });
export const ouvrirSite = () => appeler('ouvrir_site');
export const addons = () => appeler('addons');
export const langue = () => appeler('langue');
export const definirSuivi = (dossier, suivi) => appeler('definir_suivi', { dossier, suivi });
export const catalogue = () => appeler('catalogue');
export const rafraichirCatalogue = () => appeler('rafraichir_catalogue');
// `remplacer` : mise à jour d'un addon déjà installé ; champ omis sinon (première installation).
export const installerAddon = (nom, remplacer = false) => appeler('installer_addon', remplacer ? { nom, remplacer: true } : { nom });
export const desinstallerAddon = (dossier) => appeler('desinstaller_addon', { dossier });
export const reverifierLangue = () => appeler('reverifier_langue');
export const diagnostic = () => appeler('diagnostic');
export const installation = () => appeler('installation');
export const definirDossierJeu = (chemin) => appeler('definir_dossier_jeu', { chemin });
export const installerLogiciels = () => appeler('installer_logiciels');
export const preparerJeu = () => appeler('preparer_jeu');
export const exporterRapport = () => appeler('exporter_rapport');
