// Dessin mince des yeux du crâne sur un <canvas> 2D posé sur l'image (« Volute du Roi-Liche »). Toute la logique est dans
// flamme.js ; ici : taille (devicePixelRatio), lecture des pixels de l'image, boucle d'images, pause quand la page est
// cachée, mouvement réduit (image fixe : lueur peinte à éclat fixe et fente, sans volutes).
import { REGLAGES, creerRng, creerEtat, avancer, analyserPixels, analyserTrio, lueurs, fente, ruban, couleurPasse, couleurLueur, couleurFente, passesPour, traitement, sombre, borneDt } from './flamme.js';

const VARIABLES = { coeur: '--flamme-coeur', cyan: '--flamme-cyan', bleu: '--flamme-bleu' };
const REQUETE_MOUVEMENT_REDUIT = '(prefers-reduced-motion: reduce)';
const DEUX_PI = 2 * Math.PI;

function lirePaliers(noeud) {
  const style = getComputedStyle(noeud);
  return Object.fromEntries(Object.entries(VARIABLES).map(([cle, variable]) => [cle, analyserTrio(style.getPropertyValue(variable))]));
}

const rgba = (rgb, alpha) => `rgba(${rgb.map(Math.round).join(',')},${Math.min(1, Math.max(0, alpha)).toFixed(3)})`;

function tache(ctx, x, y, rayon, rgb, alpha) {
  const degrade = ctx.createRadialGradient(x, y, 0, x, y, rayon);
  degrade.addColorStop(0, rgba(rgb, alpha));
  degrade.addColorStop(1, rgba(rgb, 0));
  ctx.fillStyle = degrade;
  ctx.beginPath();
  ctx.arc(x, y, rayon, 0, DEUX_PI);
  ctx.fill();
}

// Lit les pixels de l'<img> (même origine) et en tire les masques. Null si la lecture est impossible : le dessin se dégrade
// alors en lueur sans masque et fente, sans exception.
function lirePeint(doc, image, yeux) {
  try {
    const W = image.naturalWidth;
    const H = image.naturalHeight;
    if (!W || !H) return null;
    const c = doc.createElement('canvas');
    c.width = W;
    c.height = H;
    const cx = c.getContext('2d', { willReadFrequently: true });
    cx.drawImage(image, 0, 0);
    const analyse = analyserPixels(cx.getImageData(0, 0, W, H), yeux);
    const masque = (alphas) => {
      const m = doc.createElement('canvas');
      m.width = W;
      m.height = H;
      const mx = m.getContext('2d');
      const d = mx.createImageData(W, H);
      for (let n = 0; n < alphas.length; n += 1) {
        if (alphas[n]) {
          d.data[n * 4] = d.data[n * 4 + 1] = d.data[n * 4 + 2] = 255;
          d.data[n * 4 + 3] = alphas[n];
        }
      }
      mx.putImageData(d, 0, 0);
      return m;
    };
    // Silhouette du crâne : alpha de l'image légèrement flouté (1,5 % de la hauteur), pour garder les volutes sur le crâne.
    const silhouette = doc.createElement('canvas');
    silhouette.width = W;
    silhouette.height = H;
    const sx = silhouette.getContext('2d');
    try {
      sx.filter = `blur(${Math.max(1, Math.round(0.015 * H))}px)`;
    } catch {
      /* sans filtre : bord net */
    }
    sx.drawImage(image, 0, 0);
    return { ...analyse, canvasLueur: masque(analyse.lueur), canvasCoeur: masque(analyse.masqueCoeur), silhouette };
  } catch {
    return null;
  }
}

function teinter(doc, masque, rgb) {
  const c = doc.createElement('canvas');
  c.width = masque.width;
  c.height = masque.height;
  const x = c.getContext('2d');
  x.drawImage(masque, 0, 0);
  x.globalCompositeOperation = 'source-in';
  x.fillStyle = rgba(rgb, 1);
  x.fillRect(0, 0, c.width, c.height);
  return c;
}

// image : l'<img> du crâne (le canvas a la même taille) ; yeux : fractions de l'image (voir img/embleme-crane-yeux.js).
// dpr : densité forcée (défaut : celle de la fenêtre) ; reglages : surcharge des REGLAGES (page de réglage).
export function monterFlammes({ canvas, image, yeux, graine = Date.now(), dpr: dprForce = null, reglages = null, mouvementReduit = () => matchMedia(REQUETE_MOUVEMENT_REDUIT).matches }) {
  const doc = canvas.ownerDocument;
  const fenetre = doc.defaultView;
  const ctx = canvas.getContext('2d');
  const rng = creerRng(graine);
  canvas.style.mixBlendMode = 'screen'; // les lumières s'ajoutent à l'image, comme dans la page d'essai
  let peint = null;
  let etat = creerEtat(yeux, rng, { reglages });
  let paliers = lirePaliers(canvas);
  let teintes = null;
  let calque = null;
  let image_ = 0;
  let dernier = null;
  let reduit = mouvementReduit();

  function preparer() {
    peint = lirePeint(doc, image, yeux);
    teintes = null;
    etat = creerEtat(yeux, rng, { coeurs: peint?.coeurs, reglages });
  }

  function ajuster() {
    const dpr = dprForce || fenetre.devicePixelRatio || 1;
    const l = Math.max(1, Math.round(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== l || canvas.height !== h) {
      canvas.width = l;
      canvas.height = h;
    }
    return dpr;
  }

  function teintesCourantes(mode) {
    const cle = `${paliers.cyan}|${paliers.coeur}|${mode}|${etat.r.petit.blancheur}`;
    if (!teintes || teintes.cle !== cle) {
      teintes = { cle, lueur: teinter(doc, peint.canvasLueur, couleurLueur(paliers, etat.r, mode)), coeur: teinter(doc, peint.canvasCoeur, paliers.coeur) };
    }
    return teintes;
  }

  function masque(L, H, m, i, alpha) {
    if (alpha <= 0.003) return;
    const f = peint.fenetres[i];
    ctx.globalAlpha = Math.min(1, alpha);
    ctx.drawImage(m, f.x, f.y, f.w, f.h, (f.x * L) / peint.width, (f.y * H) / peint.height, (f.w * L) / peint.width, (f.h * H) / peint.height);
    ctx.globalAlpha = 1;
  }

  function dessinerLueurs(L, H, fixe, mode) {
    etat.yeux.forEach((oeil, i) => {
      const q = lueurs(etat, i, fixe, mode);
      if (peint) {
        const t = teintesCourantes(mode);
        masque(L, H, t.lueur, i, q.lueur);
        masque(L, H, t.coeur, i, q.coeur);
        masque(L, H, t.lueur, i, q.crepitement);
      } else {
        // Sans masque : une tache ronde étirée aux dimensions de l'œil.
        ctx.save();
        ctx.translate(oeil.x * L, oeil.y * H);
        ctx.scale(oeil.rx * L, oeil.ry * H);
        tache(ctx, 0, 0, 1.6, paliers.cyan, 0.7 * q.lueur);
        ctx.restore();
      }
    });
  }

  function dessinerVolutes(L, H, dpr, mode) {
    const cle = `${L}x${H}`;
    if (!calque || calque.cle !== cle) {
      const cv = doc.createElement('canvas');
      cv.width = L;
      cv.height = H;
      calque = { cle, cv };
    }
    const cx = calque.cv.getContext('2d');
    cx.globalCompositeOperation = 'source-over';
    cx.globalAlpha = 1;
    cx.clearRect(0, 0, L, H);
    cx.globalCompositeOperation = 'lighten';
    cx.lineCap = 'round';
    cx.lineJoin = 'round';
    etat.yeux.forEach((_, i) => {
      for (let k = 0; k < etat.rubans[i].length; k += 1) {
        const segs = ruban(etat, i, k, L, H, dpr);
        if (!segs) continue;
        // Traits opaques (la clarté code l'opacité) : ni recouvrement visible, ni stries.
        for (const passe of passesPour(mode)) {
          for (let s = segs.length - 1; s >= 0; s -= 1) {
            const sg = segs[s];
            cx.strokeStyle = sombre(couleurPasse(passe, sg.u, paliers), sg.a * passe.alpha);
            cx.lineWidth = sg.w * passe.largeur;
            cx.beginPath();
            cx.moveTo(sg.depart[0], sg.depart[1]);
            cx.quadraticCurveTo(sg.c[0], sg.c[1], sg.fin[0], sg.fin[1]);
            cx.stroke();
          }
        }
      }
    });
    // Masqué par la silhouette du crâne (bords adoucis), fondu près des bords du canvas.
    cx.globalCompositeOperation = 'destination-in';
    cx.drawImage(peint.silhouette, 0, 0, L, H);
    cx.drawImage(peint.silhouette, 0, 0, L, H);
    for (const [x1, y1, x2, y2] of [[0, 0, 0.04 * L, 0], [L, 0, 0.96 * L, 0], [0, 0, 0, 0.03 * H], [0, H, 0, 0.97 * H]]) {
      const d = cx.createLinearGradient(x1, y1, x2, y2);
      d.addColorStop(0, 'rgba(0,0,0,0)');
      d.addColorStop(1, 'rgba(0,0,0,1)');
      cx.fillStyle = d;
      cx.fillRect(0, 0, L, H);
    }
    cx.globalCompositeOperation = 'source-over';
    ctx.globalCompositeOperation = 'source-over';
    ctx.drawImage(calque.cv, 0, 0);
    const v = etat.r.visibiliteVolutes;
    if (v > 1) {
      // Halo additif : le calque masqué, flouté, ajouté par-dessus (plus la visibilité dépasse 1, plus il pèse).
      ctx.save();
      try {
        ctx.filter = `blur(${Math.max(1, 0.012 * H)}px)`;
      } catch {
        /* sans filtre : halo net */
      }
      ctx.globalCompositeOperation = 'lighter';
      ctx.globalAlpha = Math.min(1, (v - 1) * 0.6 * (mode === 'petit' ? etat.r.petit.halo : 1));
      ctx.drawImage(calque.cv, 0, 0);
      ctx.restore();
    }
  }

  function dessinerFentes(L, H, dpr, fixe, mode) {
    ctx.globalCompositeOperation = 'source-over';
    etat.yeux.forEach((_, i) => {
      const f = fente(etat, i, L, H, dpr, fixe);
      if (!f) return;
      ctx.save();
      ctx.translate(f.x, f.y);
      ctx.rotate(f.angle);
      const amande = (k) => {
        ctx.beginPath();
        ctx.moveTo(-f.a * k, 0);
        ctx.quadraticCurveTo(0, -2 * f.h * k, f.a * k, 0);
        ctx.quadraticCurveTo(0, 2 * f.h * k, -f.a * k, 0);
        ctx.closePath();
      };
      if (!f.petit) {
        amande(1.5);
        ctx.fillStyle = rgba(paliers.cyan, 0.3 * f.vie);
        ctx.fill();
      }
      amande(1);
      ctx.fillStyle = rgba(couleurFente(paliers, mode), 0.92 * f.vie);
      ctx.fill();
      ctx.restore();
    });
  }

  function dessiner(fixe) {
    const dpr = ajuster();
    const { width: L, height: H } = canvas;
    ctx.globalCompositeOperation = 'source-over';
    ctx.globalAlpha = 1;
    ctx.clearRect(0, 0, L, H);
    const mode = traitement(H, etat.r);
    dessinerLueurs(L, H, fixe, mode);
    if (peint && !fixe && etat.r.visibiliteVolutes > 0.001) dessinerVolutes(L, H, dpr, mode);
    dessinerFentes(L, H, dpr, fixe, mode);
  }

  function image_suivante(maintenant) {
    image_ = 0;
    const dt = dernier === null ? 0 : (maintenant - dernier) / 1000;
    dernier = maintenant;
    avancer(etat, borneDt(dt, REGLAGES.pasMax));
    dessiner(false);
    planifier();
  }

  function planifier() {
    if (image_ || reduit || doc.hidden) return;
    image_ = fenetre.requestAnimationFrame(image_suivante);
  }

  function arreter() {
    if (image_) fenetre.cancelAnimationFrame(image_);
    image_ = 0;
    dernier = null; // à la reprise, dt repart de 0 : pas de rafale
  }

  function relancer() {
    arreter();
    paliers = lirePaliers(canvas);
    if (reduit) dessiner(true);
    else if (!doc.hidden) planifier();
  }

  function surChargement() {
    preparer();
    relancer();
  }

  const surVisibilite = () => (doc.hidden ? arreter() : relancer());
  doc.addEventListener('visibilitychange', surVisibilite);
  const requete = fenetre.matchMedia?.(REQUETE_MOUVEMENT_REDUIT);
  const surMouvement = () => {
    reduit = mouvementReduit();
    relancer();
  };
  requete?.addEventListener?.('change', surMouvement);
  image.addEventListener('load', surChargement);
  if (image.complete && image.naturalWidth) preparer();
  relancer();

  return {
    arreter() {
      arreter();
      doc.removeEventListener('visibilitychange', surVisibilite);
      requete?.removeEventListener?.('change', surMouvement);
      image.removeEventListener('load', surChargement);
    },
  };
}
