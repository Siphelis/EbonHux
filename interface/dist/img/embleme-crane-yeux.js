// Position des yeux du crâne (dist/img/embleme-crane.webp), en fractions de la largeur (x, rx) et de la hauteur (y, ry)
// de l'image rognée. Donnée de l'image : à refaire si l'image change.
// Mesure faite sur la source 1141 x 1379 (marge transparente rognée : cadre (116, 12, 1023, 1374), soit 907 x 1362) :
//  - centre : barycentre des pixels bleus (bleu - rouge > 40, opaques) de luminosité >= 90 % du maximum, dans une fenêtre de
//    +-60 px autour du cœur lumineux de chaque œil repéré à vue (source : (410, 695) et (725, 698)), entre 40 % et 60 % de la hauteur ;
//  - rayons : 1,5 fois l'écart type, en x et en y, des pixels bleus de luminosité >= 150 dans une fenêtre de +-170 x +-110 px
//    autour de ce centre (la lueur de l'orbite), ce qui donne l'ellipse de l'œil (fenêtres de lecture de la lueur peinte).
// Le cœur peint (la tache très claire au centre de l'orbite, où se pose la fente) n'est PAS figé ici : flamme.js le mesure à
// l'exécution sur les pixels de l'image (plus grande tache connexe de pixels très clairs, ellipse de l'œil comme fenêtre).
// Il se trouve vers y = 0,505 de l'image rognée, au-dessus du centre de l'ellipse (0,5236). Le champ `coeur` ci-dessous n'est
// qu'un REPLI, utilisé si les pixels ne sont pas lisibles (mesuré sur l'image 400 x 601 : x, y du barycentre ; a = 2,2 fois
// l'écart type horizontal, en fraction de la largeur).
export const YEUX = Object.freeze([
  Object.freeze({"x": 0.3228, "y": 0.5236, "rx": 0.1158, "ry": 0.057, "coeur": Object.freeze({"x": 0.3265, "y": 0.5050, "a": 0.0231})}),
  Object.freeze({"x": 0.6721, "y": 0.5257, "rx": 0.1191, "ry": 0.0544, "coeur": Object.freeze({"x": 0.6713, "y": 0.5049, "a": 0.0245})}),
]);
export const RAPPORT = Object.freeze({ largeur: 907, hauteur: 1362 }); // rapport largeur / hauteur de l'image rognée
