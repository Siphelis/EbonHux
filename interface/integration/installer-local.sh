#!/bin/sh
# Installe EbonHux pour l'utilisateur courant uniquement (aucune élévation de droits).
# Copie : le binaire, les quatre tailles de l'icône et l'entrée de menu. Relancer le script est sans danger.
set -eu

ICI=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
RACINE_COQUE=$(CDPATH= cd -- "$ICI/../src-tauri" && pwd -P)
BINAIRE_SOURCE="$RACINE_COQUE/target/release/ebonhux-launcher"
ICONES_SOURCES="32x32:32x32.png 128x128:128x128.png 256x256:256x256.png 512x512:icon.png"
MODELE="$ICI/ebonhux-launcher.desktop"

DONNEES=${XDG_DATA_HOME:-"$HOME/.local/share"}
DOSSIER_BINAIRE="$DONNEES/ebonhux"
DOSSIER_THEME="$DONNEES/icons/hicolor"
DOSSIER_MENU="$DONNEES/applications"
BINAIRE="$DOSSIER_BINAIRE/ebonhux-launcher"
ENTREE="$DOSSIER_MENU/ebonhux-launcher.desktop"

if [ "$(id -u)" -eq 0 ]; then
    echo "Erreur : ne pas lancer ce script avec les droits administrateur ; il installe pour l'utilisateur courant." >&2
    exit 1
fi
for paire in $ICONES_SOURCES; do
    fichier="$RACINE_COQUE/icons/${paire#*:}"
    if [ ! -f "$fichier" ]; then
        echo "Erreur : image de l'icône introuvable : $fichier" >&2
        exit 1
    fi
done
for fichier in "$BINAIRE_SOURCE" "$MODELE"; do
    if [ ! -f "$fichier" ]; then
        echo "Erreur : fichier introuvable : $fichier" >&2
        echo "Le binaire se construit avec : cargo build --release (dossier App/interface/src-tauri)." >&2
        exit 1
    fi
done

# Le nom de l'icône vient de son contenu : une image changée reçoit un nouveau nom, que le menu relit à neuf.
if ! command -v sha256sum >/dev/null 2>&1; then
    echo "Erreur : l'outil sha256sum est introuvable, il sert à nommer l'icône. Rien n'est installé." >&2
    exit 1
fi
SOURCES_ICONES="$RACINE_COQUE/icons"
suffixe=$(cat -- "$SOURCES_ICONES/32x32.png" "$SOURCES_ICONES/128x128.png" "$SOURCES_ICONES/256x256.png" "$SOURCES_ICONES/icon.png" | sha256sum | cut -c1-12) || suffixe=
case "$suffixe" in
    *[!0-9a-f]*|'')
        echo "Erreur : le nom de l'icône n'a pas pu être calculé. Rien n'est installé." >&2
        exit 1
        ;;
esac
if [ "${#suffixe}" -ne 12 ]; then
    echo "Erreur : le nom de l'icône n'a pas pu être calculé. Rien n'est installé." >&2
    exit 1
fi
NOM_ICONE="ebonhux-launcher-$suffixe"

# Le menu lit l'entrée avec le chemin entre guillemets ; ces caractères y ont un sens spécial (ou gênent la préparation de l'entrée).
case "$BINAIRE" in
    *'|'*|*'&'*|*'\'*|*'"'*|*'`'*|*'$'*|*'%'*)
        echo "Erreur : le chemin de l'installation contient un caractère non pris en charge (| & \\ \" \` \$ %). Rien n'est installé." >&2
        exit 1
        ;;
esac
for paire in $ICONES_SOURCES; do
    destination="$DOSSIER_THEME/${paire%%:*}/apps/$NOM_ICONE.png"
    if [ -L "$destination" ]; then
        echo "Erreur : $destination est un lien ; on n'écrit pas à travers un lien. Rien n'est installé." >&2
        exit 1
    fi
done

mkdir -p "$DOSSIER_BINAIRE" "$DOSSIER_MENU"
cp -f -- "$BINAIRE_SOURCE" "$BINAIRE"
chmod 755 "$BINAIRE"
echo "Binaire installé : $BINAIRE"
for paire in $ICONES_SOURCES; do
    taille=${paire%%:*}
    dossier="$DOSSIER_THEME/$taille/apps"
    mkdir -p "$dossier"
    cp -f -- "$RACINE_COQUE/icons/${paire#*:}" "$dossier/$NOM_ICONE.png"
    chmod 644 "$dossier/$NOM_ICONE.png"
    echo "Icône installée : $dossier/$NOM_ICONE.png"
done

# Retire les icônes laissées par les installations précédentes (ancien nom, ou autre image) ; jamais un lien ni un autre fichier.
for paire in $ICONES_SOURCES; do
    dossier="$DOSSIER_THEME/${paire%%:*}/apps"
    for ancien in "$dossier"/ebonhux-launcher.png "$dossier"/ebonhux-launcher-????????????.png; do
        nom=${ancien##*/}
        [ "$nom" = "$NOM_ICONE.png" ] && continue
        if [ "$nom" != "ebonhux-launcher.png" ]; then
            code=${nom#ebonhux-launcher-}
            code=${code%.png}
            case "$code" in
                *[!0-9a-f]*) continue ;;
            esac
        fi
        if [ -L "$ancien" ]; then
            echo "Laissé en place (c'est un lien) : $ancien"
        elif [ -f "$ancien" ]; then
            rm -f -- "$ancien"
            echo "Ancienne icône retirée : $ancien"
        fi
    done
done

# Le chemin du binaire est absolu (le menu ne connaît pas $HOME). « | » sépare le motif : le chemin n'en contient pas (vérifié plus haut).
sed -e "s|@EXEC@|\"$BINAIRE\"|" -e "s|@ICON@|$NOM_ICONE|" "$MODELE" > "$ENTREE"
chmod 644 "$ENTREE"
echo "Entrée de menu installée : $ENTREE"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DOSSIER_MENU"
    echo "Base des entrées de menu mise à jour."
else
    echo "update-desktop-database absent : étape ignorée."
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    # Échoue sans gravité si le thème n'a pas d'index (cache facultatif) : on le dit.
    if gtk-update-icon-cache -f -t "$DONNEES/icons/hicolor" >/dev/null 2>&1; then
        echo "Cache d'icônes mis à jour."
    else
        echo "Cache d'icônes non mis à jour (facultatif) : l'icône s'affichera peut-être après une reconnexion."
    fi
else
    echo "gtk-update-icon-cache absent : étape ignorée."
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    rafraichir=kbuildsycoca6
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
    rafraichir=kbuildsycoca5
else
    rafraichir=
fi
if [ -n "$rafraichir" ]; then
    if "$rafraichir" >/dev/null 2>&1; then
        echo "Menu des applications rafraîchi."
    else
        echo "Le menu des applications n'a pas pu être rafraîchi : déconnectez-vous puis reconnectez-vous pour voir la nouvelle icône."
    fi
else
    echo "Menu des applications non rafraîchi (outil absent) : étape ignorée."
fi
echo "EbonHux est installé. Cherchez « EbonHux » dans le menu des applications."
