import { DUREE_LANCEMENT_S } from './modeles.js';

const parametres = new URLSearchParams(window.location.search);
const FIGER = parametres.get('figer') === '1';
const MS = 1000;
const MINUTE_MS = 60 * MS;
const DEMARRAGE = Date.now();
const ESSAI = parametres.get('essai') === '1' && !window.__TAURI__;

// Crochet des essais d'interaction (?essai=1 seulement, jamais dans l'application réelle où window.__TAURI__ existe) : journal des appels, sans jamais la valeur d'un mot de passe,
// et erreurs JavaScript non gérées, lues par la page d'essai. Sans essai=1, rien n'est posé.
function sansSecret(valeur) {
  if (Array.isArray(valeur)) return valeur.map(sansSecret);
  if (valeur && typeof valeur === 'object') {
    return Object.fromEntries(Object.entries(valeur).map(([cle, contenu]) => [cle, cle === 'mot_de_passe' ? Boolean(contenu) : sansSecret(contenu)]));
  }
  return valeur;
}

if (ESSAI) {
  window.__appels = [];
  window.__erreurs = [];
  window.addEventListener('error', (evenement) => window.__erreurs.push(`erreur : ${evenement.message}`));
  window.addEventListener('unhandledrejection', (evenement) => window.__erreurs.push(`rejet non géré : ${evenement.reason?.message ?? evenement.reason}`));
}

// Pour tester « Parcourir » (?essai=1 seulement) : la boîte de dialogue du dossier rend le chemin donné par ?dialogue=<chemin>
// (vide = l'utilisateur annule) et garde les options reçues dans window.__ouvertures.
if (ESSAI) {
  const choix = parametres.get('dialogue');
  window.__ouvertures = [];
  window.__TAURI__ = {
    dialog: {
      open: async (options) => {
        window.__ouvertures.push(options);
        return choix || null;
      },
    },
  };
}

const VARIANTES = {
  'catalogue-erreur': { ecran: 'catalogue', dialogue: null, comptes: 'un-compte' },
  'un-compte': { ecran: 'jouer', dialogue: null },
  'catalogue-echec': { ecran: 'catalogue', dialogue: null, comptes: 'un-compte' },
  'catalogue-vide': { ecran: 'catalogue', dialogue: null, comptes: 'un-compte' },
  'langue-incomplete': { ecran: 'langue', dialogue: null, comptes: 'un-compte' },
  'diagnostic-un-compte': { ecran: 'diagnostic', dialogue: null, comptes: 'un-compte' },
  'addons-erreur': { ecran: 'addons', dialogue: null, comptes: 'un-compte' },
  'un-compte-en-jeu': { ecran: 'jouer', dialogue: null },
  'un-compte-mot-de-passe-demande': { ecran: 'connexion', dialogue: null },
  'mot-de-passe-refuse': { ecran: 'connexion', dialogue: null, comptes: 'un-compte' },
  'plusieurs-comptes': { ecran: 'jouer', dialogue: null },
  'mot-de-passe-demande': { ecran: 'connexion', dialogue: 'mot-de-passe-demande' },
  'se-deconnecter': { ecran: 'connexion', dialogue: 'se-deconnecter' },
  'premier-lancement': { ecran: null, dialogue: null, comptes: 'un-compte' },
  'premier-lancement-wine-absent': { ecran: null, dialogue: null, comptes: 'un-compte' },
  'premier-lancement-distribution-inconnue': { ecran: null, dialogue: null, comptes: 'un-compte' },
  'jeu-configure-a-preparer': { ecran: null, dialogue: null, comptes: 'un-compte' },
};

const variante = parametres.get('mock') in VARIANTES ? parametres.get('mock') : 'plusieurs-comptes';

const variantesComptes = VARIANTES[variante].comptes ?? variante;
const UN_SEUL_COMPTE = ['un-compte', 'un-compte-en-jeu', 'un-compte-mot-de-passe-demande'].includes(variantesComptes);

export const reglages = {
  ecran: parametres.get('ecran') || VARIANTES[variante].ecran,
  dialogue: VARIANTES[variante].dialogue,
  figer: FIGER,
  anim: parametres.get('anim'),
  temps: Number(parametres.get('temps')) || 0,
  deplier: parametres.get('deplier'),
  action: parametres.get('action'),
};

function maintenant() {
  return FIGER ? DEMARRAGE : Date.now();
}

function erreur(code, message) {
  return { code, message };
}

function aujourdhui2041() {
  const date = new Date();
  date.setHours(20, 41, 0, 0);
  return date.toISOString();
}

function nouveauCompte(nom, etat, extra = {}) {
  const enregistre = etat === 'connecte' || etat === 'lancement' || etat === 'en_jeu';
  return {
    nom,
    etat,
    debut: null,
    debutJeu: null,
    enregistrer: etat !== 'mot_de_passe_demande',
    motDePasse: enregistre,
    derniere: aujourdhui2041(),
    ...extra,
  };
}

function donneesInitiales() {
  const aldrik = nouveauCompte('Aldrik', 'connecte');
  const maelys = nouveauCompte('Maelys', 'lancement', { debut: DEMARRAGE - 10 * MS });
  const thorgal = nouveauCompte('Thorgal', 'mot_de_passe_demande');
  if (variantesComptes === 'un-compte') return [aldrik];
  if (variantesComptes === 'un-compte-en-jeu') return [{ ...aldrik, etat: 'en_jeu', debutJeu: DEMARRAGE - 12 * MINUTE_MS }];
  if (variantesComptes === 'un-compte-mot-de-passe-demande') return [{ ...aldrik, etat: 'mot_de_passe_demande', enregistrer: false, motDePasse: false }];
  return [{ ...aldrik, etat: 'en_jeu', debutJeu: DEMARRAGE - 12 * MINUTE_MS }, maelys, thorgal];
}

const comptes = donneesInitiales();
let nomParDefaut = null;

function avancer(compte) {
  if (compte.etat !== 'lancement') return;
  const ecoule = (maintenant() - compte.debut) / MS;
  if (ecoule >= DUREE_LANCEMENT_S) {
    compte.etat = 'en_jeu';
    compte.debutJeu = compte.debut + DUREE_LANCEMENT_S * MS;
  }
}

function exporter(compte) {
  avancer(compte);
  const lancement = compte.etat === 'lancement';
  return {
    nom: compte.nom,
    initiale: compte.nom.charAt(0).toUpperCase(),
    etat: compte.etat,
    progression: lancement ? Math.min(99, Math.floor(((maintenant() - compte.debut) / MS / DUREE_LANCEMENT_S) * 100)) : null,
    en_jeu_depuis_s: compte.etat === 'en_jeu' ? Math.floor((maintenant() - compte.debutJeu) / MS) : null,
    enregistrer_mot_de_passe: compte.enregistrer,
    mot_de_passe_enregistre: compte.motDePasse,
    derniere_connexion: compte.derniere,
    par_defaut: compte === (comptes.find((candidat) => candidat.nom === nomParDefaut) ?? comptes[0]),
  };
}

function trouver(nom) {
  const compte = comptes.find((candidat) => candidat.nom.toLowerCase() === String(nom).toLowerCase());
  if (!compte) throw erreur('compte_inconnu', `Le compte « ${nom} » n'existe pas.`);
  return compte;
}

function lancer(compte, motDePasse) {
  avancer(compte);
  if (compte.etat === 'lancement' || compte.etat === 'en_jeu') {
    throw erreur('deja_lance', `${compte.nom} est déjà lancé.`);
  }
  if (!compte.motDePasse && !motDePasse) {
    throw erreur('mot_de_passe_requis', `Le mot de passe de ${compte.nom} est demandé.`);
  }
  compte.etat = 'lancement';
  compte.debut = maintenant();
  compte.derniere = new Date(maintenant()).toISOString();
  if (compte.enregistrer && motDePasse) compte.motDePasse = true;
  return compte;
}

const LENT = parametres.get('lent') === '1';
const INSTALLES_LENT = Math.max(0, Number(parametres.get('installes-lent')) || 0);
const CATALOGUE_LENT = Math.max(0, Number(parametres.get('catalogue-lent')) || 0);
const DELAI_ACTION_MS = FIGER ? 0 : 600;
const DELAI_PREPARATION_MS = FIGER ? 0 : 900;
const DELAI_SANS_FIN_MS = 10 * MINUTE_MS;
const attendre = (duree) => new Promise((fin) => setTimeout(fin, duree));

function addon(dossier, titre, auteur, version, notes, extra = {}) {
  return { dossier, titre, notes, auteur, version, suivi: false, bibliotheque: false, modules: [], ...extra };
}

function module(dossier, suivi) {
  return { dossier, titre: dossier, suivi };
}

const listeAddons = [
  addon('DBM-Core', '|cff69ccf0Deadly Boss Mods|r', 'MysticalOS', 'r4521', 'Alertes, minuteurs et annonces pour les combats de donjon et de raid, de Naxxramas à la Citadelle de la Couronne de glace.', {
    suivi: true,
    modules: [module('DBM-Naxx', true), module('DBM-Ulduar', true), module('DBM-Party', true)],
  }),
  addon('WeakAuras', 'WeakAuras', 'The WeakAuras Team', '5.3.0', "Alertes visuelles personnalisées : icônes, barres et textes liés aux effets et aux temps de recharge.", {
    suivi: true,
    modules: [module('WeakAurasArchive', true), module('WeakAurasModelPaths', true), module('WeakAurasOptions', true)],
  }),
  addon('Recount', 'Recount', 'Elsia', 'v1.4.2', 'Mesure les dégâts, les soins et les dissipations de votre groupe.', { suivi: true }),
  addon('ElvUI', '|cff1784d1ElvUI|r', 'Elv', '1.15', "Refonte complète de l'interface : barres, cadres d'unités et sacs.", {
    suivi: true,
    modules: [module('ElvUI_Config', true), module('ElvUI_Libraries', true)],
  }),
  addon('Bagnon', 'Bagnon', 'Tuller', '9.3.3', 'Sacs et banque réunis dans une seule fenêtre, avec recherche.'),
  addon('Details', 'Details! Damage Meter', 'Tercioo', 'v1.0.8', "Compteur de dégâts et de soins en temps réel,|navec historique des combats.|n|cffaaaaaaPlus précis que les anciens compteurs.|r"),
  addon('Omen', 'Omen Threat Meter', 'Antiarc', 'r90', 'Affiche la menace de chaque joueur sur la cible.', { suivi: true }),
  addon('TomTom', 'TomTom', 'Cladhaire', '3.3.5', 'Flèche de navigation et coordonnées sur la carte.', { suivi: true }),
  addon('Bartender4', 'Bartender4', 'Nevcairiel', '4.4.2', "Barres d'action personnalisables : taille, position, visibilité.", { suivi: true }),
  addon('Titan', '|TInterface\\Icons\\Ability_Seal:16|t Titan Panel', 'TitanPanel Team', '3.5.0', "Barre d'informations en haut ou en bas de l'écran.", { suivi: true }),
  addon('GearScore', 'GearScore', 'Mirrikat45', '3.1.20', "Note d'équipement affichée dans les infobulles des joueurs.", { suivi: true }),
  addon('Questie', 'Questie', 'Aeroscout', '5.6.1', 'Affiche les quêtes disponibles et en cours sur la carte.', { suivi: true }),
  addon('Quartz', 'Quartz', 'Nevcairiel', 'r161', 'Barre de lancement de sorts personnalisable et lisible.'),
  addon('Auctioneer', 'Auctioneer', 'Auctioneer Team', '5.8.4', "Analyse des prix et recherche à l'hôtel des ventes."),
  addon('Gatherer', 'Gatherer', 'Esamynn', '3.1.14', 'Mémorise les herbes, minerais et trésors sur la carte.'),
  addon('Skada', 'Skada', 'Zarnivoop', '1.6.4', 'Compteur de dégâts léger et modulaire.'),
  addon('Prat-3.0', 'Prat 3.0', 'Sylvanaar', '3.5.1', 'Fenêtre de discussion améliorée : horodatage, couleurs et liens.'),
  addon('Atlas', 'Atlas', 'Dan Gilbert', '1.17.3', 'Cartes des donjons et des raids.'),
  addon('Postal', 'Postal', 'Xinhuan', '3.3.2', 'Courrier amélioré : tout récupérer en un clic.'),
  addon('MoveAnything', 'MoveAnything', 'Wagthaa', '3.3.5', "Déplace et redimensionne n'importe quel élément de l'interface."),
  addon('SexyMap', 'SexyMap', 'Funkydude', '3.5.0', 'Minicarte personnalisable : forme, bordure, boutons.'),
  addon('Mapster', 'Mapster', 'Hendrik Leppkes', '1.3.9', 'Carte du monde redimensionnable et plus lisible.'),
  addon('Pitbull4', 'PitBull Unit Frames 4.0', 'Shefki', '4.0.0', "Cadres d'unités modulaires.|nAssociez des modules pour personnaliser."),
  addon('Healbot', 'HealBot Continued', 'Strife', '3.4.5', 'Soins en un clic avec cadres de groupe configurables.'),
  addon('Grid', 'Grid', 'Phanx', '1.3.0', 'Cadres de groupe et de raid compacts.'),
  addon('Clique', 'Clique', 'Cladhaire', '1.2.1', 'Assigne des sorts aux clics sur les cadres.'),
  addon('Scrap', 'Scrap', 'Jaliborc', '2.4.0', 'Vend automatiquement les objets inutiles aux marchands.'),
  addon('Vendor', 'Vendor', 'Dolby', '1.0.4', "Règles de vente et de destruction d'objets."),
  addon('Chatter', 'Chatter', 'Antiarc', 'r150', 'Modules de discussion : onglets, copie, filtres.'),
  addon('Squire2', 'Squire2', 'Cladhaire', '2.0.9', 'Gère automatiquement la monture et le familier.'),
  addon('Altoholic', 'Altoholic', 'Thaoky', '3.3.003', "Suivi de tous vos personnages : sacs, quêtes et métiers."),
  addon('Align', 'Align', 'Aluminium', '1.0.1', 'Grille de placement pour aligner les éléments.'),
  addon('BigWigs', 'BigWigs Bossmods', 'BigWigs Team', 'r9921', 'Alertes de combat de boss, alternatives légères.'),
  addon('Dominos', 'Dominos', 'Tuller', '1.0.9', "Barres d'action et barre de menus configurables."),
  addon('Cartographer', 'Cartographer', 'Cartographer Team', '3.0.2', 'Notes et repères personnalisés sur la carte.'),
  addon('Examiner', 'Examiner', 'Sylvanaar', '1.7', "Inspecte l'équipement et les talents d'autres joueurs."),
  addon('Spy', 'Spy', 'Immolation', '2.8', "Détecte et note les joueurs ennemis à proximité."),
  addon('ReagentRestocker', 'Reagent Restocker', 'Zorg', '2.1', 'Réachète les composants manquants chez le marchand.'),
  addon('OmniCC', 'OmniCC', 'Tuller', '3.3.2', 'Affiche les temps de recharge en chiffres sur les icônes.'),
  addon('LightHeaded', 'LightHeaded', 'Moonhunter', '3.3.5', 'Guide de quêtes intégré avec étapes et coordonnées.'),
  addon('Enchantrix', 'Enchantrix', 'Auctioneer Team', '5.8.4', "Prévoit les résultats du désenchantement."),
  addon('ArkInventory', 'ArkInventory', 'Arkayenro', '3.03.30', 'Inventaire unifié pour tous vos personnages.'),
  addon('Ace3', 'Ace3', 'Ace3 Team', 'r1100', 'Ensemble de bibliothèques pour les addons.', { bibliotheque: true }),
  addon('AceGUI-3.0', 'AceGUI-3.0', 'Ace3 Team', 'r1100', 'Bibliothèque de composants graphiques.', { bibliotheque: true }),
  addon('AceDB-3.0', 'AceDB-3.0', 'Ace3 Team', 'r1100', 'Bibliothèque de sauvegarde des réglages.', { bibliotheque: true }),
  addon('CallbackHandler-1.0', 'CallbackHandler-1.0', 'Ace3 Team', 'r6', "Bibliothèque de gestion d'événements.", { bibliotheque: true }),
  addon('LibStub', 'LibStub', 'Kaelten', 'r95', 'Gestionnaire de versions des bibliothèques.', { bibliotheque: true }),
  addon('LibDataBroker-1.1', 'LibDataBroker-1.1', 'Tekkub', 'r5', 'Bibliothèque de données pour les barres.', { bibliotheque: true }),
  addon('LibSharedMedia-3.0', 'LibSharedMedia-3.0', 'Elkano', 'r90', 'Partage de polices, textures et sons.', { bibliotheque: true }),
  addon('LibDBIcon-1.0', 'LibDBIcon-1.0', 'Funkydude', 'r34', 'Bouton de minicarte standard.', { bibliotheque: true }),
  addon('LibGroupTalents-1.0', 'LibGroupTalents-1.0', 'Zerotorescue', 'r90', 'Talents des membres du groupe.', { bibliotheque: true }),
  addon('LibItemSearch-1.0', 'LibItemSearch-1.0', 'Jaliborc', 'r12', "Recherche d'objets dans les sacs.", { bibliotheque: true }),
  addon('LibRangeCheck-2.0', 'LibRangeCheck-2.0', 'mitch0', 'r84', 'Distance approximative à la cible.', { bibliotheque: true }),
  addon('LibButtonFacade', 'LibButtonFacade', 'Hendrik Leppkes', 'r8', "Habillage des boutons d'action.", { bibliotheque: true }),
];

const NOMS_COMPLEMENTAIRES = [
  'Accountant', 'AdiBags', 'AutoBar', 'BugSack', 'ButtonFacade', 'ClassTimer', 'Cooline', 'CowTip', 'DataStore', 'DoubleWide',
  'EavesDrop', 'EquipCompare', 'FishingBuddy', 'FuBar', 'GatherMate', 'HandyNotes', 'InspectEquip', 'ItemRack', 'Kui_Nameplates', 'LoseControl',
  'MikScrollingBattleText', 'MonkeySpeed', 'Natur', 'OneBag', 'oRA3', 'PallyPower', 'Quartermaster', 'QuestHelper', 'RatingBuster', 'Recipe Book',
  'SellFish', 'SmartRes', 'SpellFlash', 'TinyDPS', 'TidyPlates', 'Tukui', 'WIM', 'XLoot', 'Zygor Guides', 'ZOMGBuffs',
  'Bejeweled', 'CoolLine', 'DrDamage', 'Fizzle', 'GuildGreet', 'MogIt', 'OutfitComp', 'PetTracker', 'RareScanner', 'SimpleMail',
  'TipTac', 'UnderHood', 'Aloft', 'BadBoy', 'BeanCounter', 'Bongos', 'CooldownCount', 'DejaCharacterStats', 'EnhancedTooltip', 'FlightMap',
  'GroupCalendar', 'Informant', 'InstanceMaps', 'KeyBound', 'LittleWigs', 'MapCoords', 'NPCScan', 'OpenRaid', 'Pet Battle Helper', 'QuestGuru',
  'RaidAchievement', 'SilverDragon', 'Stubby', 'TourGuide', 'TrinketMenu', 'VuhDo', 'WhisperCast', 'YaCC', 'ZPerl', 'AtlasQuest',
  'Archy', 'CTMod', 'DXE',
];
const TOTAL_ATTENDU = 139;
for (const [rang, nom] of NOMS_COMPLEMENTAIRES.entries()) {
  if (compterDossiers() >= TOTAL_ATTENDU) break;
  listeAddons.push(addon(nom.replace(/ /g, ''), nom, 'Communauté', `r${100 + rang}`, `Complément ${nom}.`));
}

function compterDossiers() {
  return listeAddons.length; // en addons : les modules ne comptent pas
}

function compterSuivis() {
  return listeAddons.filter((entree) => entree.suivi).length;
}

const IMAGE_MOCK = 'https://launcher.way-of-elendil.fr/addons/img/mock.png';

// Entrée de catalogue : `dossiers_installes` suit `installe` (comme le service réel après une installation).
function entreeMock(nom, description, { dossier, dossiers = [dossier], categories, version, installe = false, maj = null, avatar }) {
  const entree = {
    nom, description, url: `https://github.com/mock/${encodeURIComponent(dossier)}`, source: 'github.com', dossier, avatar, categories, version, installe,
    mise_a_jour_disponible: installe ? maj : null,
  };
  Object.defineProperty(entree, 'dossiers_installes', { enumerable: true, get: () => (entree.installe ? dossiers : []) });
  Object.assign(entree, { description_longue: null, images: [], auteur: null });
  Object.defineProperty(entree, 'version_installee', { enumerable: true, get: () => (entree.installe ? (entree.version_installee_mock ?? entree.version) : null) });
  return entree;
}

const catalogue = [
  entreeMock('Pawn', 'Évalue les objets et indique les améliorations pour ton équipement.', { dossier: 'Pawn', categories: ['Outils', 'Combat'], version: '2.8', avatar: null }),
  entreeMock('Quartz', 'Barre de lancement de sorts personnalisable et lisible.', { dossier: 'Quartz', categories: ['Combat', 'Interface'], version: '3.1', installe: true, maj: true, avatar: IMAGE_MOCK }),
  entreeMock('Prat 3.0', 'Fenêtre de discussion améliorée : horodatage, couleurs et liens.', { dossier: 'Prat-3.0', categories: ['Interface'], version: '3.0.9', installe: true, maj: false, avatar: null }),
  entreeMock('Atlas Loot', 'Tables de butin des donjons et des raids.', { dossier: 'AtlasLoot', categories: ['Raids', 'Outils'], version: null, avatar: null }),
  entreeMock('Deadly Boss Mods', 'Alertes de boss et minuteurs de combat.', { dossier: 'DBM-Core', categories: ['Combat', 'Raids'], version: '9.2', avatar: IMAGE_MOCK }),
  entreeMock('Recount', 'Dégâts et soins du groupe, par joueur.', { dossier: 'Recount', categories: ['Combat'], version: '3.3', avatar: null }),
  entreeMock('ElvUI', 'Interface complète : barres, sacs, cartes.', { dossier: 'ElvUI', dossiers: ['ElvUI', 'Accountant'], categories: ['Interface'], version: '6.09', installe: true, maj: false, avatar: IMAGE_MOCK }),
  entreeMock('Bagnon', 'Sacs et banque réunis en une fenêtre.', { dossier: 'Bagnon', categories: ['Interface'], version: '2.14', installe: true, maj: true, avatar: null }),
  entreeMock('Details!', 'Mesures de combat : dégâts et soins.', { dossier: 'Details', categories: ['Combat'], version: '1.9', installe: true, maj: null, avatar: IMAGE_MOCK }),
  entreeMock('Auctionator', "Recherche et prix de l'hôtel des ventes.", { dossier: 'Auctionator', categories: ['Économie'], version: '2.6', avatar: null }),
  entreeMock('Questie', 'Carte des quêtes et objectifs sur le monde.', { dossier: 'Questie', categories: ['Quêtes'], version: '5.0.1', avatar: IMAGE_MOCK }),
  entreeMock('Mapster', 'Carte du monde redimensionnable (installé à la main, dossier inconnu).', { dossier: 'Mapster', dossiers: [], categories: ['Quêtes', 'Interface'], version: '1.3', installe: true, maj: false, avatar: null }),
  entreeMock('TradeSkillMaster', 'Métiers, enchères et profits au même endroit.', { dossier: 'TradeSkillMaster', categories: ['Économie', 'Outils'], version: '4.8', avatar: null }),
  entreeMock('BigWigs', 'Alertes de rencontres de raid, légères.', { dossier: 'BigWigs', categories: ['Raids', 'Combat'], version: '3.4', installe: true, maj: true, avatar: IMAGE_MOCK }),
  entreeMock('Omen', 'Menace des monstres sur le groupe.', { dossier: 'Omen', categories: ['Combat'], version: '3.0', avatar: null }),
  entreeMock('Titan Panel', 'Barre d\'informations en haut ou en bas de l\'écran.', { dossier: 'TitanPanel', categories: ['Interface', 'Outils'], version: '7.2', avatar: null }),
];

// Essai du rendu par lots (?gros=1) : 100 entrées de plus, toutes dans la catégorie « Volume ».
if (parametres.get('gros') === '1') {
  for (let rang = 1; rang <= 100; rang += 1) {
    const numero = String(rang).padStart(3, '0');
    catalogue.push(entreeMock(`Module ${numero}`, `Addon de volume numéro ${numero}.`, { dossier: `Module${numero}`, categories: ['Volume'], version: '1.0', avatar: null }));
  }
}

// Détails du catalogue (description longue, images, auteur) : une avec HTML, une très longue, une absente ; 0, 1 et 3 images.
function detailMock(nom, details) {
  const entree = catalogue.find((candidat) => candidat.nom === nom);
  if (details.version_installee_mock !== undefined) Object.defineProperty(entree, 'version_installee_mock', { value: details.version_installee_mock, enumerable: false });
  const { version_installee_mock: _ignore, ...reste } = details;
  Object.assign(entree, reste);
}
const PARAGRAPHE_LONG = 'Ce module suit chaque rencontre de raid, de la première minute à la dernière, et garde un historique complet par personnage. ';
detailMock('Quartz', {
  version_installee_mock: '3.0',
  auteur: 'Nymeria',
  description_longue: '<b>Quartz</b> remplace la barre de lancement de sorts.\n<script>window.__injecte = 1</script><img src="x" onerror="window.__injecte = 2">\nLe texte HTML reste affiché tel quel.',
  images: [`${IMAGE_MOCK}?q=1`, `${IMAGE_MOCK}?q=2`, `${IMAGE_MOCK}?q=3`],
});
detailMock('BigWigs', {
  version_installee_mock: '3.2',
  auteur: 'Funkydude',
  description_longue: Array.from({ length: 24 }, (_, rang) => `${PARAGRAPHE_LONG}Paragraphe ${rang + 1}.`).join('\n\n') + '\nFIN-DE-LA-DESCRIPTION',
  images: [`${IMAGE_MOCK}?b=1`],
});
detailMock('Pawn', { auteur: 'Vger', images: [`${IMAGE_MOCK}?p=1`] });
detailMock('Prat 3.0', { description_longue: 'Horodatage, couleurs et liens cliquables dans la discussion.', auteur: 'Sylvanaar' });

const DOSSIER_JEU = '/home/joueur/Jeux/World of Warcraft';
const VERSION_WINE = '11.20';
const SCENARIOS_INSTALLATION = {
  'premier-lancement': { dossier: null, wine: true, possible: true, prepare: false },
  'premier-lancement-wine-absent': { dossier: null, wine: false, possible: true, prepare: false },
  'premier-lancement-distribution-inconnue': { dossier: null, wine: false, possible: false, prepare: false },
  'jeu-configure-a-preparer': { dossier: DOSSIER_JEU, wine: true, possible: true, prepare: false },
};
const installationEtat = { ...(SCENARIOS_INSTALLATION[variante] ?? { dossier: DOSSIER_JEU, wine: true, possible: true, prepare: true }) };

function jeuConfigure() {
  return installationEtat.dossier !== null;
}

function exigerJeuConfigure() {
  if (!jeuConfigure()) throw erreur('jeu_non_configure', "Le dossier du jeu n'est pas encore configuré.");
}

const TITRES_INSTALLATION = { jeu: 'Dossier du jeu', logiciels: 'Logiciels nécessaires', preparation: 'Préparation du jeu' };

function etapeInstallation(id, etat, detail) {
  return { id, titre: TITRES_INSTALLATION[id], etat, detail };
}

function installationVue() {
  const { dossier, wine, possible, prepare } = installationEtat;
  const jeu = dossier !== null;
  const logiciels = wine ? 'fait' : (possible ? 'a_faire' : 'bloque');
  const pret = jeu && wine && prepare;
  let preparation = 'bloque';
  if (prepare) preparation = 'fait';
  else if (jeu && wine) preparation = 'a_faire';
  return {
    pret,
    jeu_configure: jeu,
    dossier_jeu: dossier,
    dossier_jeu_valide: jeu,
    distribution: possible ? 'Solus' : 'Inconnue',
    installation_possible: possible,
    wine: { present: wine, version: wine ? VERSION_WINE : null },
    prefixe_pret: prepare,
    dxvk_pose: prepare,
    etapes: [
      etapeInstallation('jeu', jeu ? 'fait' : 'a_faire', jeu ? '' : 'Indiquez où le jeu est installé.'),
      etapeInstallation('logiciels', logiciels, wine ? '' : 'Wine est absent.'),
      etapeInstallation('preparation', preparation, preparation === 'bloque' ? 'Terminez les étapes 1 et 2 avant.' : ''),
    ],
  };
}

function verifierDossierJeu(chemin) {
  const propre = String(chemin).trim().replace(/\/+$/, '');
  if (!propre.startsWith('/')) throw erreur('dossier_jeu_invalide', 'Le chemin doit être complet : il commence par « / ».');
  if (!propre.endsWith('World of Warcraft')) throw erreur('dossier_jeu_invalide', "Ce dossier ne contient pas le jeu : il manque Wow.exe et le dossier Data.");
  return propre;
}

function langue() {
  const incomplete = variante === 'langue-incomplete';
  return {
    code: 'frFR',
    nom: 'Français',
    etat: incomplete ? 'incomplet' : 'protege',
    elements: [
      { id: 'donnees', titre: 'Données françaises', detail: 'Data/frFR', present: true, protege: !incomplete, valeur: null },
      { id: 'correctif', titre: 'Correctif de langue', detail: 'patch-Z.MPQ', present: !incomplete, protege: !incomplete, valeur: null },
      { id: 'locale', titre: 'Langue du client', detail: 'locale dans Config.wtf', present: true, protege: !incomplete, valeur: incomplete ? 'enUS' : 'frFR' },
    ],
  };
}

function heureAujourdhui(heures, minutes, secondes) {
  const date = new Date();
  date.setHours(heures, minutes, secondes, 0);
  return date.toISOString();
}

function diagnostic() {
  const un = comptes.length === 1;
  const communes = [
    { heure: heureAujourdhui(21, 4, 12), compte: null, etape: 'Détection de Wine', resultat: 'ok', detail: '' },
    { heure: heureAujourdhui(21, 4, 13), compte: null, etape: 'Vérification de Config.wtf', resultat: 'ok', detail: '' },
    { heure: heureAujourdhui(21, 4, 14), compte: 'Aldrik', etape: 'Lancement du jeu', resultat: 'ok', detail: '' },
    { heure: heureAujourdhui(21, 4, 14), compte: null, etape: 'addon « Cheese » inutilisable : champ requis « Title » absent dans Cheese.toc, il ne sera pas proposé', resultat: 'avertissement', detail: 'addon « Cheese » inutilisable : champ requis « Title » absent dans Cheese.toc, il ne sera pas proposé' },
  ];
  const propres = un ? [] : [
    { heure: heureAujourdhui(21, 4, 15), compte: 'Maelys', etape: "Préparation de l'environnement", resultat: 'erreur', detail: 'Commande : lutris --prepare\nCode de sortie : 1\nMessage : préfixe introuvable dans ~/.../jeu' },
    { heure: heureAujourdhui(21, 4, 15), compte: 'Thorgal', etape: 'Mot de passe demandé', resultat: 'attente', detail: '' },
  ];
  const copies = Math.min(Number(parametres.get('lignes')) || 1, 50);
  const lignes = Array.from({ length: copies }, () => [...communes, ...propres]).flat();
  return {
    systeme: { wine: '11.20 (WoW64)', vulkan: '64 et 32 bits', pilote: 'Mesa', fichiers_ouverts: 'Limite 524288', session: 'Wayland' },
    lignes,
    comptes: comptes.map((compte) => compte.nom),
    etapes: lignes.length,
    erreurs: lignes.filter((ligne) => ligne.resultat === 'erreur').length,
    avertissements: lignes.filter((ligne) => ligne.resultat === 'avertissement').length,
  };
}

const COMMANDES = {
  comptes() {
    const vus = jeuConfigure() ? comptes.map(exporter) : [];
    return {
      comptes: vus,
      en_jeu: vus.filter((compte) => compte.etat === 'lancement' || compte.etat === 'en_jeu').length,
      trousseau_disponible: true,
      jeu_configure: jeuConfigure(),
    };
  },
  ajouter_compte({ nom, mot_de_passe: motDePasse, enregistrer }) {
    const propre = String(nom).trim();
    if (!propre) throw erreur('nom_invalide', 'Le nom du compte est vide.');
    if (!motDePasse) throw erreur('mot_de_passe_vide', 'Le mot de passe est vide.');
    if (comptes.some((compte) => compte.nom.toLowerCase() === propre.toLowerCase())) {
      throw erreur('compte_existe', `Le compte « ${propre} » existe déjà.`);
    }
    const compte = nouveauCompte(propre, enregistrer ? 'connecte' : 'mot_de_passe_demande');
    compte.derniere = null;
    comptes.push(compte);
    return { compte: exporter(compte) };
  },
  enregistrer_mot_de_passe({ compte: nom, mot_de_passe: motDePasse }) {
    const compte = trouver(nom);
    if (!motDePasse) throw erreur('mot_de_passe_vide', 'Le mot de passe est vide.');
    if (variante === 'mot-de-passe-refuse') throw erreur('trousseau_refuse', "Le mot de passe n'a pas pu être enregistré : accès refusé.");
    compte.motDePasse = true;
    compte.enregistrer = true;
    if (compte.etat === 'deconnecte' || compte.etat === 'mot_de_passe_demande') compte.etat = 'connecte';
    return { compte: exporter(compte) };
  },
  definir_enregistrement({ compte: nom, actif }) {
    const compte = trouver(nom);
    compte.enregistrer = actif;
    if (!actif) {
      compte.motDePasse = false;
      if (compte.etat === 'connecte' || compte.etat === 'deconnecte') compte.etat = 'mot_de_passe_demande';
    } else if (compte.etat === 'mot_de_passe_demande') {
      compte.etat = 'deconnecte';
    }
    return { compte: exporter(compte) };
  },
  definir_compte_par_defaut({ compte: nom }) {
    nomParDefaut = trouver(nom).nom;
    return COMMANDES.comptes();
  },
  se_deconnecter({ compte: nom }) {
    const compte = trouver(nom);
    compte.motDePasse = false;
    if (compte.etat === 'connecte') compte.etat = compte.enregistrer ? 'deconnecte' : 'mot_de_passe_demande';
    return { compte: exporter(compte) };
  },
  lancer({ compte: nom, mot_de_passe: motDePasse }) {
    return { compte: exporter(lancer(trouver(nom), motDePasse)) };
  },
  tout_lancer({ comptes: demandes }) {
    const resultats = demandes.map(({ nom, mot_de_passe: motDePasse }) => {
      try {
        lancer(trouver(nom), motDePasse);
        return { compte: nom, ok: true, message: 'Lancé.' };
      } catch (cause) {
        return { compte: nom, ok: false, message: cause.message };
      }
    });
    return { resultats };
  },
  async addons() {
    exigerJeuConfigure();
    if (INSTALLES_LENT) await attendre(INSTALLES_LENT);
    if (variante === 'addons-erreur') throw erreur('lecture_addons', "Impossible de lire le dossier des addons : accès refusé à Interface/AddOns.");
    return { total: compterDossiers(), suivis: compterSuivis(), addons: listeAddons, problemes: [] };
  },
  definir_suivi({ dossier, suivi }) {
    const parent = listeAddons.find((addon) => addon.dossier === dossier);
    const module = parent ? null : listeAddons.flatMap((addon) => addon.modules).find((candidat) => candidat.dossier === dossier);
    if (!parent && !module) throw erreur('addon_inconnu', `L'addon « ${dossier} » n'est pas installé.`);
    if (parent) {
      parent.suivi = suivi;
      parent.modules.forEach((sousModule) => { sousModule.suivi = suivi; });
    } else {
      module.suivi = suivi;
    }
    return { suivis: compterSuivis() };
  },
  async catalogue() {
    if (CATALOGUE_LENT) await attendre(CATALOGUE_LENT);
    return { entrees: (variante === 'catalogue-vide' || variante === 'catalogue-echec') ? [] : catalogue, invalides: 0 };
  },
  async rafraichir_catalogue() {
    await attendre(DELAI_ACTION_MS);
    if (variante === 'catalogue-echec') throw erreur('telechargement', 'Le catalogue n\'a pas pu être téléchargé : service indisponible.');
    return { entrees: variante === 'catalogue-vide' ? [] : catalogue, invalides: 0 };
  },
  async installer_addon({ nom, remplacer = false }) {
    const entree = catalogue.find((candidat) => candidat.nom === nom);
    if (!entree) throw erreur('addon_inconnu', `L'addon « ${nom} » n'est pas dans le catalogue.`);
    if (entree.installe && !remplacer) throw erreur('refus', `addon « ${entree.dossier} » déjà installé : refus (option « remplacer » non demandée)`);
    await attendre(DELAI_ACTION_MS);
    if (variante === 'catalogue-erreur') throw erreur('telechargement', `Le téléchargement de « ${nom} » a échoué : service indisponible.`);
    entree.installe = true;
    entree.mise_a_jour_disponible = false;
    return { installes: [entree.dossier] };
  },
  async desinstaller_addon({ dossier }) {
    const minuscule = String(dossier).toLowerCase();
    const rang = listeAddons.findIndex((candidat) => candidat.dossier.toLowerCase() === minuscule);
    if (rang === -1) {
      // Un module seul : on ne retire que ce module de son addon.
      const parent = listeAddons.find((candidat) => candidat.modules.some((m) => m.dossier.toLowerCase() === minuscule));
      if (!parent) throw erreur('addon_inconnu', `L'addon « ${dossier} » n'est pas installé.`);
      await attendre(DELAI_ACTION_MS);
      parent.modules = parent.modules.filter((m) => m.dossier.toLowerCase() !== minuscule);
      return { desinstalle: dossier };
    }
    await attendre(DELAI_ACTION_MS);
    const [retire] = listeAddons.splice(rang, 1);
    for (const entree of catalogue) {
      if ((entree.dossiers_installes ?? []).some((nom) => nom.toLowerCase() === retire.dossier.toLowerCase())) entree.installe = false;
    }
    return { desinstalle: dossier };
  },
  langue() {
    exigerJeuConfigure();
    return langue();
  },
  async reverifier_langue() {
    await attendre(DELAI_ACTION_MS);
    return langue();
  },
  diagnostic() {
    return diagnostic();
  },
  exporter_rapport() {
    return { chemin: '~/.local/share/ebonhux/rapports/rapport-20261024-210500.txt' };
  },
  installation() {
    return installationVue();
  },
  definir_dossier_jeu({ chemin }) {
    installationEtat.dossier = verifierDossierJeu(chemin);
    return installationVue();
  },
  async installer_logiciels() {
    if (installationEtat.wine) return { etapes: [], message: 'Wine est déjà installé.' };
    if (!installationEtat.possible) throw erreur('distribution_non_prise_en_charge', "Votre distribution n'est pas prise en charge pour l'installation automatique.");
    await attendre(LENT ? DELAI_SANS_FIN_MS : DELAI_ACTION_MS);
    installationEtat.wine = true;
    return {
      etapes: [
        { description: 'Mise à jour de la liste des logiciels', statut: 'reussie' },
        { description: 'Installation de Wine', statut: 'reussie' },
      ],
      message: 'Wine est installé.',
    };
  },
  async preparer_jeu() {
    if (!jeuConfigure() || !installationEtat.wine) throw erreur('preparation_impossible', 'Terminez les étapes 1 et 2 avant de préparer le jeu.');
    await attendre(LENT ? DELAI_SANS_FIN_MS : DELAI_PREPARATION_MS);
    installationEtat.prepare = true;
    return installationVue();
  },
  ouvrir_site() {
    throw erreur('site_non_configure', 'Adresse du site non configurée');
  },
};

export async function appeler(commande, args = {}) {
  if (ESSAI) window.__appels.push({ commande, args: sansSecret(args) });
  const action = COMMANDES[commande];
  if (!action) throw erreur('commande_inconnue', `Commande inconnue : ${commande}`);
  return structuredClone(await action(args));
}
