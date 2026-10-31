#!/bin/sh
# Construit l'AppImage d'EbonHux sur l'hôte (WebKitGTK n'est que là), de façon sobre :
# un seul cargo à la fois, priorité minimale, 4 tâches au plus.
set -eu

ICI=$(cd "$(dirname "$0")" && pwd -P)
COQUE=$(cd "$ICI/../src-tauri" && pwd -P)
VERROU=/mnt/JeuxInstalles/WebDev/AddOns/EbonHold/_copies/.verrou-cargo
SORTIE="$COQUE/target/release/bundle/appimage"

if pgrep -af 'cargo|rustc' | grep -v 'pgrep' | grep -v "construire-appimage" >/dev/null 2>&1; then
  echo "Un cargo ou rustc tourne déjà : attendre sa fin avant de construire." >&2
  exit 1
fi

echo "Construction de l'AppImage (environ 2 à 5 minutes)..."
cd "$COQUE"
NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1 \
  nice -n 19 ionice -c3 flock "$VERROU" env CARGO_BUILD_JOBS=4 \
  npm exec --yes --package=@tauri-apps/cli@2 -- tauri build --bundles appimage

APPIMAGE=$(ls -t "$SORTIE"/*.AppImage 2>/dev/null | head -n 1 || true)
if [ -z "$APPIMAGE" ]; then
  echo "Aucune AppImage trouvée dans $SORTIE" >&2
  exit 1
fi
echo "AppImage produite : $APPIMAGE"
du -h "$APPIMAGE" | cut -f1 | sed 's/^/Taille : /'
