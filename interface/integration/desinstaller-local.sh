#!/bin/sh
# Retire exactement les éléments (binaire, quatre tailles de l'icône, entrée de menu) posés par installer-local.sh, et rien d'autre.
# Aucune élévation de droits. Relancer le script est sans danger.
set -eu

DONNEES=${XDG_DATA_HOME:-"$HOME/.local/share"}
BINAIRE="$DONNEES/ebonhux/ebonhux-launcher"
ENTREE="$DONNEES/applications/ebonhux-launcher.desktop"

if [ "$(id -u)" -eq 0 ]; then
    echo "Erreur : ne pas lancer ce script avec les droits administrateur." >&2
    exit 1
fi

# Retire un fichier ordinaire dont le chemin est exactement celui attendu (jamais un lien, jamais un dossier).
retirer() {
    cible=$1
    attendu=$2
    if [ "$cible" != "$attendu" ]; then
        echo "Erreur : chemin inattendu, rien n'est retiré : $cible" >&2
        exit 1
    fi
    if [ -L "$cible" ]; then
        echo "Erreur : $cible est un lien, il n'est pas retiré." >&2
        exit 1
    fi
    if [ -f "$cible" ]; then
        rm -f -- "$cible"
        echo "Retiré : $cible"
    else
        echo "Déjà absent : $cible"
    fi
}

retirer "$BINAIRE" "$DONNEES/ebonhux/ebonhux-launcher"
for taille in 32x32 128x128 256x256 512x512; do
    dossier="$DONNEES/icons/hicolor/$taille/apps"
    # Le nom actuel de l'icône dépend de son contenu : on retire l'ancien nom et chaque nom de la forme ebonhux-launcher-<12 caractères>.
    for fichier in "$dossier"/ebonhux-launcher.png "$dossier"/ebonhux-launcher-????????????.png; do
        [ -e "$fichier" ] || [ -L "$fichier" ] || continue
        nom=${fichier##*/}
        if [ "$nom" != "ebonhux-launcher.png" ]; then
            code=${nom#ebonhux-launcher-}
            code=${code%.png}
            case "$code" in
                *[!0-9a-f]*) continue ;;
            esac
        fi
        retirer "$fichier" "$dossier/$nom"
    done
done
retirer "$ENTREE" "$DONNEES/applications/ebonhux-launcher.desktop"

# Le dossier ebonhux/ est laissé en place (vide) : seuls les éléments posés sont retirés.

if command -v update-desktop-database >/dev/null 2>&1 && [ -d "$DONNEES/applications" ]; then
    update-desktop-database "$DONNEES/applications"
    echo "Base des entrées de menu mise à jour."
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$DONNEES/icons/hicolor/icon-theme.cache" ]; then
    if gtk-update-icon-cache -f -t "$DONNEES/icons/hicolor" >/dev/null 2>&1; then
        echo "Cache d'icônes mis à jour."
    fi
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
        echo "Le menu des applications n'a pas pu être rafraîchi : déconnectez-vous puis reconnectez-vous pour faire disparaître l'icône."
    fi
else
    echo "Menu des applications non rafraîchi (outil absent) : étape ignorée."
fi
echo "EbonHux est désinstallé."
