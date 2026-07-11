#!/usr/bin/env bash
# Installation de Engram_hive dans l'espace utilisateur.
# Requis : cargo, Rust toolchain stable.

set -euo pipefail

BINAIRE="engram_hive"
DEST_BIN="${HOME}/.local/bin"
DEST_APPS="${HOME}/.local/share/applications"
DEST_ICONE="${HOME}/.local/share/engram_hive"

echo "on monte d'un etage vu qu'on m'a releguer dans un placard qui pue la vieille chaussette"

# 1. Le script est dans "install/", on remonte d'un cran vers la racine
cd "$(dirname "$0")/.." || exit 1

# Vérification de sécurité pour l'utilisateur
if [ ! -f "Cargo.toml" ]; then
    echo "Erreur : Cargo.toml introuvable. Structure du projet incorrecte. fallait pas toucher à ça . Bon retourne sur une page officiel et retelecharge je vais me faire un thé " >&2
    exit 1
fi

echo "=== Build release de engram_hive ==="
# Point 2 : Tentative de build parfait. Si échec, fallback sur un build standard.
if ! RUSTFLAGS="-C target-cpu=native" cargo build --release; then
    echo "Avertissement : Le build optimisé (native CPU) a foiré ,enfin vu ta machine de merde cherche pas plus loin." >&2
    echo "Tentative de repli façon 'les Français a Hazincourt'  avec un build release standard..." >&2
    cargo build --release
fi

echo "=== Installation du binaire dans ${DEST_BIN}/ ==="
mkdir -p "${DEST_BIN}"
#|--------------#|--------------|#|#
# Message d'erreur ajusté logiquement
#|--------------|##|--------------|##|--------------|#
if [ ! -f "target/release/${BINAIRE}" ]; then
    echo "Erreur critique : Tu vas rire , ca a build mais je sais pas ou on a foutu le binaire " >&2
    exit 1
fi

cp "target/release/${BINAIRE}" "${DEST_BIN}/${BINAIRE}"
chmod +x "${DEST_BIN}/${BINAIRE}"

echo "=== Installation de l'icône, une faute de gout si tu veux mon avis  ==="
if [ -f "assets/icon.png" ]; then
    mkdir -p "${DEST_ICONE}"
    cp "assets/icon.png" "${DEST_ICONE}/icon.png"
fi

echo "=== Installation du fichier .desktop pour que ton binaire finnise pas au chiotte==="
mkdir -p "${DEST_APPS}"
#|---------------------------------------------------------|#
#| Recherche du fichier .desktop dans le dossier "install/" |
#|---------------------------------------------------------|#
if [ -f "install/engram_hive.desktop" ]; then
    cp "install/engram_hive.desktop" "${DEST_APPS}/engram_hive.desktop"
    # Icône : chemin ABSOLU vers le fichier installé (le lanceur ne résout pas
    # un nom thémé puisqu'on n'installe pas dans un thème d'icônes).
    sed -i "s|^Icon=.*|Icon=${DEST_ICONE}/icon.png|" "${DEST_APPS}/engram_hive.desktop"
else
    echo "Attention : Fichier install/engram_hive.desktop introuvable, étape ignorée. Enfin comme quand ta meuf/ton mec/ta mère/dieu te demande de sortir les poubelles  " >&2
fi

echo " Mise a jour de la DB des apps . Promis je dirais rien sur 'PornHub_Premium.desktop'."
#|------------------------------------------------|#
#| Mise à jour du cache des applications .desktop |#
#|------------------------------------------------|#
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "${DEST_APPS}" 2>/dev/null || true
fi
#|--------------|#
#| LE NETTOYAGE |#
#|--------------|#
echo "=== je sors le Lance-Flammes pour un petit nettoyage de la target ==="
cargo clean
echo "  [OK] Dossier target incinérer... j'aime l'odeur du Napalm au petit matin ."
# ================================================================
echo -e "\nInstallation terminée.En principe, on sait jamais , peut etre que je fou une backdoor pour profiter de ton abonnement premium sur pornhub. Ben oui on lit un .sh avant de le lancer ,hygienne numérique"
echo "  Binaire : ${DEST_BIN}/${BINAIRE}"
echo "  Lanceur : ${DEST_APPS}/engram_hive.desktop"

# Vérification dynamique du PATH
if [[ ":$PATH:" != *":${DEST_BIN}:"* ]]; then
    echo -e "\n[IMPORTANT] ${DEST_BIN} n'est pas dans votre PATH ni dans vos pattes ni la pathpatrouille. Ajoutez ceci à votre ~/.bashrc ou ~/.zshrc :"
    echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi

echo -e "\nPour démarrer avec le système : ajoutez engram_hive aux applications au démarrage de votre environnement (KDE, GNOME (vous n'avez aucun gout), etc.)."
