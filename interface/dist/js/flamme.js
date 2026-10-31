// Logique pure des yeux du crâne (« Volute du Roi-Liche ») : aucune API du navigateur, aucun temps réel. Tout avance par
// le temps écoulé (dt), borné, et par un générateur pseudo-aléatoire passé en paramètre à la création (graine fixe dans
// les essais). Les phases s'accumulent modulo 2π : aucune boucle visible, aucune rafale à la reprise.
// Les positions sont des fractions de la largeur (x) et de la hauteur (y) de l'image du crâne.
// Ce que le dessin (flamme-canevas.js) en tire : les masques de la lueur et du cœur peints (analyserPixels), les intensités
// de la lueur (lueurs), la fente (fente) et les rubans de volute (ruban).

const DEUX_PI = 2 * Math.PI;

// Réglages choisis par le propriétaire dans la page d'essai (palette « Bleu glacé » = jetons --flamme-*).
export const REGLAGES = Object.freeze({
  pasMax: 0.1, // s : plus grand dt pris en compte (reprise après pause, onglet gelé)
  epaisseur: 0.01, // largeur d'un ruban, en hauteur de l'image
  epaisseurPetite: 3, // px : épaisseur minimale d'un ruban à petite taille (hauteur réelle < petit.seuil)
  fenteTaille: 0.85, // 0 = cœur peint seul
  longueur: 2, // longueur d'un ruban, en longueurs d'œil
  montee: 25, // degrés
  ondulation: 0.6,
  opacite: 1,
  respiration: 0.85,
  visibiliteVolutes: 1.05, // 0 = aucune volute ; au-delà de 1, un halo s'ajoute
  vitesseOnde: 1,
  // Constantes de la variante (non réglables).
  crepitement: 0.06,
  cadence: 45, // points émis par seconde et par ruban
  segments: 32, // points de la courbe guide d'un ruban
  vieRuban: 0.65, // s
  miseEnRoute: 3, // s simulées avant le premier affichage
  eclatFixe: 0.6, // part de la lueur affichée quand le mouvement est réduit
  // Traitement « petite taille » : canvas de moins de `seuil` pixels RÉELS de haut (le logo du rail : 56 px à densité 1,
  // 112 px à densité 2). À cette taille la lueur, la fente et les volutes se fondaient en une bande blanchâtre sur les orbites.
  petit: Object.freeze({
    seuil: 150, // px réels
    blancheur: 0.1, // 0 = lueur cyan pure, 1 = lueur blanche (cœur) ; sert aussi au poids du cœur peint, blanc, ajouté à la lueur
    lueur: 0.5, // part de la lueur peinte conservée (l'œil peint garde son contraste ; la respiration reste visible, plus faible)
    eclatFente: 1, // vivacité de la fente (1 = pleine)
    longueurFente: 0.55, // longueur de la fente par rapport au rendu normal (trait court, plus court que la lueur)
    contrasteVolutes: 1.1, // multiplicateur d'opacité des volutes (couleurs saturées, jamais plus claires que le jeton milieu)
    fonduVolutes: 0.6, // exposant d'extinction des volutes le long du ruban (1,25 en grand ; plus petit = la queue reste visible)
    halo: 0.3, // part du halo additif des volutes conservée
  }),
});

// Réglages effectifs : surcharge superficielle, sauf `petit` fusionné champ par champ.
export const fusionnerReglages = (surcharge = null) =>
  Object.freeze({ ...REGLAGES, ...surcharge, petit: Object.freeze({ ...REGLAGES.petit, ...surcharge?.petit }) });

// Traitement selon la hauteur réelle (pixels) du canvas : 'petit' sous le seuil, 'normal' au-dessus.
export const traitement = (hauteurPx, r = REGLAGES) => (hauteurPx < r.petit.seuil ? 'petit' : 'normal');

// Passes de dessin d'un ruban : halo large et pâle, puis trait fin et clair. Couleur = mélange des jetons selon la position u.
export const PASSES = Object.freeze([
  Object.freeze({ largeur: 2.4, alpha: 0.3, de: 'cyan', vers: 'bleu', vitesse: 1 }),
  Object.freeze({ largeur: 1, alpha: 1, de: 'coeur', vers: 'cyan', vitesse: 1.6 }),
]);

// Petite taille : mêmes passes, mais de --flamme-cyan (milieu) vers --flamme-bleu (extérieur), jamais le cœur blanc.
export const PASSES_PETIT = Object.freeze([
  Object.freeze({ largeur: 1.4, alpha: 0.35, de: 'cyan', vers: 'bleu', vitesse: 1 }),
  Object.freeze({ largeur: 1, alpha: 0.8, de: 'cyan', vers: 'bleu', vitesse: 1.6 }),
]);
export const passesPour = (mode) => (mode === 'petit' ? PASSES_PETIT : PASSES);

// mulberry32 : suite reproductible dans [0, 1[.
export function creerRng(graine) {
  let s = graine >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const borner = (v, a, b) => Math.min(b, Math.max(a, v));
export const melange = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const entre = (rng, a, b) => a + (b - a) * rng();
export const lisse = (a, b, x) => {
  const t = borner((x - a) / (b - a), 0, 1);
  return t * t * (3 - 2 * t);
};

export function analyserTrio(texte) {
  const valeurs = String(texte).split(',').map((v) => Number(v.trim()));
  if (valeurs.length !== 3 || valeurs.some((v) => !Number.isFinite(v))) throw new Error(`trio de couleur invalide : ${texte}`);
  return valeurs;
}

// dt négatif, NaN ou infini : 0 ou pasMax. Jamais plus que pasMax.
export function borneDt(dt, pasMax = REGLAGES.pasMax) {
  if (!(dt > 0)) return 0;
  return Math.min(dt, pasMax);
}

// Bruit lisse sans raccord : somme de trois sinus aux fréquences sans rapport simple ; chaque phase s'accumule modulo 2π.
export function creerBruit(rng, cadence) {
  const f = [0.83, 1.37, 2.29].map((v) => v * cadence * (0.85 + 0.3 * rng()));
  return { f, p: f.map(() => rng() * DEUX_PI) };
}
export function avancerBruit(b, dt) {
  for (let i = 0; i < 3; i += 1) b.p[i] = (b.p[i] + b.f[i] * dt) % DEUX_PI;
}
// Valeur dans [-1, 1] (le plus souvent entre -0,6 et 0,6).
export const valeurBruit = (b) => 0.5 * Math.sin(b.p[0]) + 0.3 * Math.sin(b.p[1]) + 0.2 * Math.sin(b.p[2]);

/* ---------- Masques tirés des pixels de l'image ---------- */

// tampon : { width, height, data } (RGBA, comme getImageData). Retourne, pour chaque œil, la fenêtre de la lueur peinte
// et le cœur mesuré ; et deux masques d'alpha (un octet par pixel) : la lueur (bleu - rouge > 40, opaque) et le cœur
// (la plus grande tache connexe de pixels très clairs près du centre de l'œil).
export function analyserPixels(tampon, yeux) {
  const { width: W, height: H, data: px } = tampon;
  const lueur = new Uint8ClampedArray(W * H);
  const masqueCoeur = new Uint8ClampedArray(W * H);
  const bleu = (i) => px[i + 3] > 200 && px[i + 2] - px[i] > 40 && px[i] < 0.55 * px[i + 2];
  const candCoeur = (i) => px[i + 3] > 200 && px[i] > 225 && px[i + 1] > 235 && px[i + 2] > 235;
  const lum = (i) => 0.299 * px[i] + 0.587 * px[i + 1] + 0.114 * px[i + 2];
  const fenetres = [];
  const coeurs = [];
  for (const o of yeux) {
    // Lueur : fenêtre élargie autour de l'ellipse de l'œil.
    const x0 = Math.max(0, Math.floor((o.x - 1.5 * o.rx) * W));
    const x1 = Math.min(W, Math.ceil((o.x + 1.5 * o.rx) * W));
    const y0 = Math.max(0, Math.floor((o.y - 2.2 * o.ry) * H));
    const y1 = Math.min(H, Math.ceil((o.y + 1.8 * o.ry) * H));
    for (let y = y0; y < y1; y += 1) {
      for (let x = x0; x < x1; x += 1) {
        const i = (y * W + x) * 4;
        if (bleu(i) && px[i + 2] > 120) {
          const w = borner((px[i + 1] - px[i] - 60) / 130, 0, 1);
          if (w > 0) lueur[y * W + x] = Math.round(255 * w);
        }
      }
    }
    // Cœur : fenêtre étroite autour du centre mesuré, plus grande tache connexe de pixels très clairs
    // (pas les arêtes de métal ni les éclairs fins).
    const cx0 = Math.max(0, Math.floor((o.x - 0.7 * o.rx) * W));
    const cx1 = Math.min(W, Math.ceil((o.x + 0.7 * o.rx) * W));
    const cy0 = Math.max(0, Math.floor((o.y - 1.2 * o.ry) * H));
    const cy1 = Math.min(H, Math.ceil((o.y + 0.1 * o.ry) * H));
    const vu = new Uint8Array(W * H);
    let meilleure = [];
    for (let y = cy0; y < cy1; y += 1) {
      for (let x = cx0; x < cx1; x += 1) {
        if (vu[y * W + x] || !candCoeur((y * W + x) * 4)) continue;
        const comp = [];
        const pile = [y * W + x];
        vu[y * W + x] = 1;
        while (pile.length) {
          const n = pile.pop();
          comp.push(n);
          const nx = n % W;
          const ny = (n - nx) / W;
          for (const [ax, ay] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
            const mx = nx + ax;
            const my = ny + ay;
            if (mx < cx0 || mx >= cx1 || my < cy0 || my >= cy1 || vu[my * W + mx]) continue;
            if (candCoeur((my * W + mx) * 4)) {
              vu[my * W + mx] = 1;
              pile.push(my * W + mx);
            }
          }
        }
        if (comp.length > meilleure.length) meilleure = comp;
      }
    }
    let sw = 0;
    let sx = 0;
    let sy = 0;
    const lots = [];
    for (const n of meilleure) {
      const x = n % W;
      const y = (n - x) / W;
      const w = Math.min(1, lum(n * 4) / 50 + 0.4);
      masqueCoeur[n] = Math.round(255 * w);
      sw += w;
      sx += w * x;
      sy += w * y;
      lots.push([x, w]);
    }
    if (sw > 0) {
      const mx = sx / sw;
      let vx = 0;
      for (const [x, w] of lots) vx += w * (x - mx) ** 2;
      const ecart = Math.sqrt(vx / sw) / W; // écart type horizontal, en fraction de la largeur
      coeurs.push({ x: mx / W, y: sy / sw / H, a: Math.max(2.2 * ecart, 0.012) });
    } else {
      coeurs.push(coeurRepli(o));
    }
    fenetres.push({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
  }
  return { width: W, height: H, lueur, masqueCoeur, fenetres, coeurs };
}

// Cœur d'un œil quand les pixels ne sont pas lisibles : celui de la mesure (img/embleme-crane-yeux.js), sinon le centre de l'œil.
export function coeurRepli(oeil) {
  return oeil.coeur ? { ...oeil.coeur } : { x: oeil.x, y: oeil.y, a: 0.03 };
}

/* ---------- État et simulation ---------- */

// État initial, déjà « en régime » : quelques secondes simulées avant le premier affichage.
// coeurs : cœurs mesurés sur les pixels (analyserPixels), sinon le repli. reglages : surcharge (essais).
export function creerEtat(yeux, rng, { coeurs = null, reglages = null, miseEnRoute = REGLAGES.miseEnRoute } = {}) {
  const r = fusionnerReglages(reglages);
  const etat = {
    r,
    yeux,
    coeurs: yeux.map((o, i) => (coeurs && coeurs[i]) || coeurRepli(o)),
    lueurs: yeux.map(() => ({
      respire: creerBruit(rng, 0.95),
      lent: creerBruit(rng, 0.31),
      crep: creerBruit(rng, 9),
      crep2: creerBruit(rng, 13),
    })),
    fentes: yeux.map(() => creerBruit(rng, 0.6)),
    rubans: yeux.map(() => Array.from({ length: 2 }, () => ({
      pts: [],
      reste: 0,
      phi: rng() * DEUX_PI,
      phi2: rng() * DEUX_PI,
      n1: creerBruit(rng, 1.9),
      n2: creerBruit(rng, 1.3),
      cap: creerBruit(rng, 0.7),
      vis: creerBruit(rng, 0.35),
      enr: rng() * DEUX_PI,
    }))),
  };
  for (let t = 0; t < miseEnRoute; t += 1 / 60) avancer(etat, 1 / 60);
  return etat;
}

// Les deux rubans d'un œil : le principal, et un intermittent un peu plus raide, décalé vers le haut.
export function configRubans(r) {
  const ruban = (opacite, montee, dx, dy, interm) => ({
    opacite, montee, dx, dy, interm,
    vie: r.vieRuban, longueur: r.longueur, epaisseur: r.epaisseur, ond: r.ondulation, omega: 8 * r.vitesseOnde, k: 7,
    croiss: 3, enroul: 0.15, jet: 0.3,
  });
  return [ruban(r.opacite, r.montee, 0, 0, false), ruban(r.opacite * 0.7, Math.min(90, r.montee + 18), -0.1, 0.1, true)];
}

export function avancer(etat, dt) {
  const r = etat.r;
  const pas = borneDt(dt, r.pasMax);
  const cfgs = configRubans(r);
  for (const e of etat.lueurs) {
    avancerBruit(e.respire, pas);
    avancerBruit(e.lent, pas);
    avancerBruit(e.crep, pas);
    avancerBruit(e.crep2, pas);
  }
  for (const b of etat.fentes) avancerBruit(b, pas);
  for (const rubans of etat.rubans) {
    rubans.forEach((st, k) => {
      const cfg = cfgs[k];
      st.phi = (st.phi + cfg.omega * pas) % DEUX_PI;
      st.phi2 = (st.phi2 + cfg.omega * 0.618 * pas) % DEUX_PI;
      st.enr = (st.enr + 0.9 * pas) % DEUX_PI;
      avancerBruit(st.n1, pas);
      avancerBruit(st.n2, pas);
      avancerBruit(st.cap, pas);
      avancerBruit(st.vis, pas);
      for (const p of st.pts) p.age += pas;
      st.pts = st.pts.filter((p) => p.age < cfg.vie);
      st.reste += r.cadence * pas;
      while (st.reste >= 1) {
        st.reste -= 1;
        st.pts.push({ age: st.reste / r.cadence, a: valeurBruit(st.n1) * 2, b: valeurBruit(st.n2) * 2 });
      }
    });
  }
  return etat;
}

/* ---------- Ce qu'on dessine ---------- */

// Opacités (dans [0, 1]) de la lueur peinte, du cœur peint et de la lueur au crépitement, pour l'œil i.
// fixe : mouvement réduit, éclat constant et pas de crépitement.
export function lueurs(etat, i, fixe = false, mode = 'normal') {
  const r = etat.r;
  const p = mode === 'petit' ? r.petit : null;
  const e = etat.lueurs[i];
  const n = fixe ? r.eclatFixe : borner(0.5 + 0.85 * valeurBruit(e.respire) + 0.5 * valeurBruit(e.lent), 0, 1);
  const c = valeurBruit(e.crep) * 1.6 + valeurBruit(e.crep2) * 0.8;
  const v = fixe ? 0 : lisse(0.25, 0.85, c) ** 2;
  const limite = (a) => Math.min(1, a);
  return {
    lueur: limite(0.8 * r.respiration * n * (p ? p.lueur : 1)),
    coeur: limite(0.75 * r.crepitement * v * (p ? p.blancheur : 1)),
    crepitement: limite(0.5 * r.crepitement * v * (p ? p.lueur : 1)),
  };
}

// Couleur de la lueur peinte : le jeton cyan (milieu) ; à petite taille, teinté vers le cœur blanc d'à peine `blancheur`.
export function couleurLueur(paliers, r, mode = 'normal') {
  return mode === 'petit' ? melange(paliers.cyan, paliers.coeur, r.petit.blancheur) : [...paliers.cyan];
}

// Couleur du cœur de la fente : le jeton cœur ; à petite taille, légèrement teinté de cyan (pas un aplat blanc).
export function couleurFente(paliers, mode = 'normal') {
  return mode === 'petit' ? melange(paliers.coeur, paliers.cyan, 0.3) : [...paliers.coeur];
}

// Fente en amande posée sur le cœur : demi-largeur a, demi-hauteur h (pixels), angle (radians), vivacité. Null si fenteTaille ~ 0.
export function fente(etat, i, L, H, dpr, fixe = false) {
  const r = etat.r;
  const petit = traitement(H, r) === 'petit';
  if (r.fenteTaille <= 0.01) return null;
  const sens = etat.yeux[i].x < 0.5 ? -1 : 1;
  const c = etat.coeurs[i];
  const vie = fixe ? 0.9 : 0.9 + 0.1 * valeurBruit(etat.fentes[i]) * 2;
  const angle = (-sens * 9 * Math.PI) / 180;
  if (petit) {
    // Trait court et fin, net, sans halo : plus court que la lueur pour se détacher.
    const a = Math.max(c.a * 4.5 * r.fenteTaille * L * r.petit.longueurFente, 1.5 * dpr);
    return { x: c.x * L, y: c.y * H, a, h: Math.max(a * 0.1, 0.5 * dpr), angle, vie: Math.min(1, vie * r.petit.eclatFente / 0.9), petit: true };
  }
  const a = Math.max(c.a * 4.5 * r.fenteTaille * L, 1.8 * dpr);
  const h = Math.max(a * 0.11, 0.7 * dpr);
  return { x: c.x * L, y: c.y * H, a, h, angle, vie, petit: false };
}

// Couleur d'une passe de ruban à la position u, d'après les trois jetons {coeur, cyan, bleu}.
export function couleurPasse(passe, u, paliers) {
  return melange(paliers[passe.de], paliers[passe.vers], Math.min(1, u * passe.vitesse));
}

// Segments d'un ruban (œil i, ruban k) : chaîne de points guidée par une courbe (jet horizontal puis montée), avec une onde
// qui court vers l'extérieur. Chaque segment : { depart, c, fin, w, a, u } en pixels (u = âge relatif, 0 à la naissance).
// Null s'il n'y a rien à dessiner. Le ruban naît au coin extérieur de l'orbite.
export function ruban(etat, i, k, L, H, dpr) {
  const r = etat.r;
  const cfg = configRubans(r)[k];
  const st = etat.rubans[i][k];
  if (!cfg || !st || st.pts.length < 2) return null;
  const vis = cfg.interm ? lisse(-0.05, 0.4, valeurBruit(st.vis) * 2) : 0.8 + 0.2 * valeurBruit(st.vis) * 2;
  if (vis <= 0.01) return null;
  const M = r.segments;
  const petit = traitement(H, r) === 'petit';
  const facteur = 0.6 + 0.4 * r.visibiliteVolutes; // épaisseur selon la visibilité (1 = rendu d'origine)
  const sens = etat.yeux[i].x < 0.5 ? -1 : 1;
  const eyeLen = 2 * etat.yeux[i].rx * L;
  const ce = etat.coeurs[i];
  const ox = (ce.x + sens * (0.07 + cfg.dx * 0.5)) * L;
  const oy = (ce.y - (0.03 - cfg.dy * 0.3)) * H;
  const montee = ((cfg.montee + valeurBruit(st.cap) * 22) * Math.PI) / 180;
  const ds = (cfg.longueur * eyeLen) / M;
  const tx = [0];
  const ty = [0];
  const th = [];
  for (let j = 0; j <= M; j += 1) {
    const u = j / M;
    const t = montee * lisse(0, cfg.jet, u) + cfg.enroul * 0.9 * Math.sin(u * 5.2 + st.enr) * u;
    th.push(t);
    if (j < M) {
      tx.push(tx[j] + ds * Math.cos(t) * sens);
      ty.push(ty[j] - ds * Math.sin(t));
    }
  }
  const pts = [{ x: ox, y: oy, u: 0 }];
  for (const p of [...st.pts].sort((a, b) => a.age - b.age)) {
    const u = Math.min(0.999, p.age / cfg.vie);
    const f = u * M;
    const j = Math.floor(f);
    const s = f - j;
    const gx = tx[j] + (tx[j + 1] - tx[j]) * s;
    const gy = ty[j] + (ty[j + 1] - ty[j]) * s;
    const t = th[j] + (th[j + 1] - th[j]) * s;
    const dx = Math.cos(t) * sens;
    const dy = -Math.sin(t);
    const amp = cfg.ond * eyeLen * 0.2 * u ** 0.85 * (1 + 0.35 * p.b);
    const onde = amp * (Math.sin(st.phi - cfg.k * u + p.a * 0.5) + 0.5 * Math.sin(st.phi2 - 1.43 * cfg.k * u));
    const lat = p.a * 0.07 * eyeLen * u;
    pts.push({ x: ox + gx - dy * (onde + lat), y: oy + gy + dx * (onde + lat), u });
  }
  const mil = [];
  for (let j = 0; j < pts.length - 1; j += 1) mil.push([(pts[j].x + pts[j + 1].x) / 2, (pts[j].y + pts[j + 1].y) / 2]);
  const segs = [];
  for (let j = 0; j < mil.length; j += 1) {
    const u = pts[j].u;
    let w = cfg.epaisseur * H * (0.6 + cfg.croiss * u);
    if (petit) w = Math.max(w, r.epaisseurPetite * dpr * (1 - 0.55 * u));
    w *= facteur;
    segs.push({
      depart: j === 0 ? [pts[0].x, pts[0].y] : mil[j - 1],
      c: [pts[j].x, pts[j].y],
      fin: mil[j],
      w,
      a: cfg.opacite * r.visibiliteVolutes * vis * (1 - u) ** (petit ? r.petit.fonduVolutes : 1.25) * Math.min(1, 0.2 + u * 16) * (petit ? r.petit.contrasteVolutes : 1),
      u,
    });
  }
  return segs;
}

// Ombre d'une couleur : le noir n'éclaire rien en mode « écran », donc la clarté code l'opacité.
export const sombre = (rgb, a) => `rgb(${rgb.map((v) => Math.round(v * borner(a, 0, 1))).join(',')})`;
