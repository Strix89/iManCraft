#!/usr/bin/env bash
# Script di build e packaging di IManCraft per Linux (x86_64)
set -euo pipefail

echo "=============================================================="
echo " CREAZIONE PACCHETTO DI DISTRIBUZIONE IMANCRAFT PER LINUX"
echo "=============================================================="

DIST_DIR="dist/IManCraft-v0.2.0-linux-x86_64"
TAR_PATH="dist/IManCraft-v0.2.0-linux-x86_64.tar.gz"

echo ""
echo "[1/4] Compilazione in Release mode (GUI + CLI)..."
cargo build --release -p imancraft -p imancraft-cli

if [ ! -f "target/release/imancraft" ]; then
    echo "Errore: target/release/imancraft non trovato!" >&2
    exit 1
fi

if [ ! -f "target/release/imancraft-cli" ]; then
    echo "Errore: target/release/imancraft-cli non trovato!" >&2
    exit 1
fi

echo "[2/4] Preparazione cartella di distribuzione: ${DIST_DIR}..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

cp target/release/imancraft "${DIST_DIR}/"
cp target/release/imancraft-cli "${DIST_DIR}/"
chmod +x "${DIST_DIR}/imancraft" "${DIST_DIR}/imancraft-cli"

if [ -f "imancraft.desktop" ]; then
    cp imancraft.desktop "${DIST_DIR}/"
fi

if [ -f "assets/imancraft.ico" ]; then
    cp assets/imancraft.ico "${DIST_DIR}/"
fi

if [ -f "LEGGIMI.txt" ]; then
    cp LEGGIMI.txt "${DIST_DIR}/"
fi

if [ -d "tools" ]; then
    cp -r tools "${DIST_DIR}/"
fi

echo "[3/4] Creazione archivio compresso ${TAR_PATH}..."
mkdir -p dist
tar -czf "${TAR_PATH}" -C dist "IManCraft-v0.2.0-linux-x86_64"

echo "[4/4] Verifica e calcolo checksum SHA-256..."
echo ""
echo "Contenuto pacchetto:"
ls -lh "${DIST_DIR}"
echo ""
echo "Checksum archivio finale:"
sha256sum "${TAR_PATH}"
echo ""
echo "=============================================================="
echo " Pacchetto Linux generato con successo in ${TAR_PATH}"
echo "=============================================================="
