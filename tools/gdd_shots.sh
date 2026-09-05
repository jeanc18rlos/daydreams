#!/usr/bin/env bash
# Toma las figuras del capítulo 3 del GDD (Geometría imposible) y las deja listas
# en docs/img/. Sin argumentos las toma todas; con uno, solo esa.
#
#   ./tools/gdd_shots.sh                       # las quince
#   ./tools/gdd_shots.sh ne-casa-seis-cuartos  # una sola
#
# Reglas de la casa, ya aplicadas en cada comando: --windowed (una corrida a pantalla
# completa roba el foco y cualquier tecla suelta contamina la toma) y --mute.
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=./target/release/daydreams
[ -x "$BIN" ] || { echo "falta $BIN -- corre: cargo build --release"; exit 1; }
mkdir -p docs/img
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
only="${1:-}"
shot() {
  name="$1"; shift
  if [ -n "$only" ] && [ "$only" != "$name" ]; then return 0; fi
  printf "%-32s" "$name"
  "$BIN" "$@" --shot "$TMP/$name.bmp" >/dev/null 2>&1
  sips -s format jpeg -s formatOptions 78 -Z 1280 "$TMP/$name.bmp" --out "docs/img/$name.jpg" >/dev/null
  echo "-> docs/img/$name.jpg"
}

shot ne-portal-desde-fuera --windowed --mute --no-gamepad --no-vsync --scene 0 --pos 2.4,1.5,4.5 --yaw 0 --frames 120
shot ne-portal-a-mitad-de-cruce --windowed --mute --no-gamepad --no-vsync --scene 0 --pos 2.4,1.5,2.2 --yaw 0 --forward --frames 600
shot ne-tunel-por-fuera --windowed --mute --no-gamepad --no-vsync --scene 0 --pos 0,1.5,8 --yaw 0 --pitch 0 --frames 120
shot ne-tunel-por-dentro --windowed --mute --no-gamepad --no-vsync --scene 0 --pos 2.4,1.5,1.2 --yaw 0 --pitch 0 --frames 120
shot ne-casa-seis-cuartos --windowed --mute --no-gamepad --no-vsync --scene 2 --pos 6.7,1.5,-0.7 --yaw=-45 --pitch 0 --frames 120
shot ne-tunel-escala-antes --windowed --mute --no-gamepad --no-vsync --scene 5 --pos=-1.2,1.5,6.0 --yaw 0 --pitch 0 --forward --frames 240
shot ne-tunel-escala-despues --windowed --mute --no-gamepad --no-vsync --scene 5 --pos=-1.2,1.5,6.0 --yaw 0 --pitch 0 --forward --frames 3700
shot ne-planta-infinita --windowed --mute --no-gamepad --no-vsync --scene 6 --pos 5.0292,1.5,1.0 --yaw 180 --pitch 0 --frames 120
shot ne-penrose-ciclo-corredor --windowed --mute --no-gamepad --no-vsync --scene 8 --pos 0,0.4,2.0 --yaw 180 --frames 60
shot ne-relativity-escultura --windowed --mute --no-gamepad --no-vsync --scene 13 --yaw 180 --frames 60
shot ne-compound-objeto-en-mano --windowed --mute --no-gamepad --no-vsync --scene 9 --pos=-1,1.5,6 --yaw 0 --pitch=-20 --e-at 30 --forward --frames 900
shot ne-anillo-desde-el-punto --windowed --mute --no-gamepad --no-vsync --scene 11 --pos 0,1.5,10 --yaw 0 --pitch 0 --frames 600
shot ne-anillo-fuera-del-punto --windowed --mute --no-gamepad --no-vsync --scene 11 --yaw 27.4 --pitch 0 --frames 60
shot ne-cubo-pintado-desde-su-punto --windowed --mute --no-gamepad --no-vsync --scene 12 --pos 0,1.5,6 --yaw 0 --pitch 7.6 --frames 120
shot ne-estatuas-sin-mirar --windowed --mute --no-gamepad --no-vsync --scene 10 --pos 0,1.5,-4 --yaw 0 --pitch=-7 --frames 120

echo
echo "Listo. Si tomaste todas, avísame y republico el GDD con las figuras dentro."
