#!/usr/bin/env bash
# Ouvre un shell dans l'environnement ROS 2 Jazzy + Rust, avec le depot monte.
#
# Usage :
#   ./scripts/ros2-shell.sh                # shell interactif
#   ./scripts/ros2-shell.sh colcon build   # commande unique
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="roboto-ros2:jazzy"

if ! docker image inspect "${image}" >/dev/null 2>&1; then
    echo "Image ${image} absente, construction..." >&2
    docker build -t "${image}" "${repository}/docker/ros2"
fi

# --network host pour que DDS decouvre les noeuds lances hors du conteneur.
exec docker run --rm -it \
    --network host \
    -v "${repository}:/workspace" \
    -w /workspace/ros2 \
    "${image}" "${@:-bash}"
