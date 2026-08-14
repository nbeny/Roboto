#!/usr/bin/env bash
# Prepare un WSL2 Ubuntu 24.04 pour faire tourner Roboto nativement.
#
#   bash scripts/setup-wsl2.sh
#
# Installe ce qui manque et rien d'autre : le script est idempotent, on peut le relancer.
#
# Suppose ROS 2 Jazzy deja installe. Sinon :
#   https://docs.ros.org/en/jazzy/Installation/Ubuntu-Install-Debs.html
set -euo pipefail

readonly ROS_DISTRO_NAME="${ROS_DISTRO:-jazzy}"
readonly RUST_WORKSPACE="${HOME}/ros2_rust_ws"

info() { printf '\n\033[1;34m==\033[0m %s\n' "$*"; }
skip() { printf '   \033[2mdeja present : %s\033[0m\n' "$*"; }

if [[ ! -d "/opt/ros/${ROS_DISTRO_NAME}" ]]; then
    echo "ROS 2 ${ROS_DISTRO_NAME} est introuvable dans /opt/ros." >&2
    echo "Installe-le d'abord, puis relance ce script." >&2
    exit 1
fi

# --- 1. Paquets systeme ----------------------------------------------------------------

info "Paquets APT"

apt_packages=(
    git
    curl
    libclang-dev
    python3-pip
    python3-vcstool
    "ros-${ROS_DISTRO_NAME}-slam-toolbox"
    "ros-${ROS_DISTRO_NAME}-navigation2"
    "ros-${ROS_DISTRO_NAME}-nav2-bringup"
    "ros-${ROS_DISTRO_NAME}-ros-gz"
    "ros-${ROS_DISTRO_NAME}-xacro"
    "ros-${ROS_DISTRO_NAME}-robot-state-publisher"
    "ros-${ROS_DISTRO_NAME}-teleop-twist-keyboard"
    "ros-${ROS_DISTRO_NAME}-example-interfaces"
    "ros-${ROS_DISTRO_NAME}-test-msgs"
)

missing=()
for package in "${apt_packages[@]}"; do
    dpkg -s "${package}" >/dev/null 2>&1 || missing+=("${package}")
done

if (( ${#missing[@]} > 0 )); then
    echo "   a installer : ${missing[*]}"
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends "${missing[@]}"
else
    skip "tous les paquets APT"
fi

# --- 2. Rust ---------------------------------------------------------------------------

info "Chaine d'outils Rust"

if command -v cargo >/dev/null 2>&1; then
    skip "cargo $(cargo --version | cut -d' ' -f2)"
else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --component rustfmt --component clippy
fi

# shellcheck disable=SC1091
source "${HOME}/.cargo/env" 2>/dev/null || export PATH="${HOME}/.cargo/bin:${PATH}"

# --- 3. Greffons colcon ----------------------------------------------------------------

info "Greffons colcon pour Rust"

if python3 -c "import colcon_cargo, colcon_ros_cargo" >/dev/null 2>&1; then
    skip "colcon-cargo et colcon-ros-cargo"
else
    pip install --break-system-packages colcon-cargo colcon-ros-cargo
fi

if command -v cargo-ament-build >/dev/null 2>&1; then
    skip "cargo-ament-build"
else
    # colcon-ros-cargo delegue a cet outil pour installer les artefacts Rust.
    cargo install cargo-ament-build
fi

# --- 4. Espace ros2_rust ---------------------------------------------------------------

info "Espace ros2_rust (rclrs et crates de messages)"

# Les crates Rust des messages ROS 2 ne sont pas distribuables : la seule version publiee
# de `geometry_msgs` a ete yankee en avril 2024. Elles sont generees par `rosidl_rust` au
# moment d'un `colcon build`. Voir docs/architecture/0005-adaptateur-ros2-hors-workspace.md.
if [[ -f "${RUST_WORKSPACE}/install/setup.bash" ]]; then
    skip "${RUST_WORKSPACE}"
else
    mkdir -p "${RUST_WORKSPACE}/src"
    cd "${RUST_WORKSPACE}"

    if [[ ! -d src/ros2_rust ]]; then
        git clone --depth 1 https://github.com/ros2-rust/ros2_rust.git src/ros2_rust
        vcs import src < "src/ros2_rust/ros2_rust_${ROS_DISTRO_NAME}.repos"
    fi

    echo "   construction (environ 4 minutes)..."
    # shellcheck disable=SC1090
    source "/opt/ros/${ROS_DISTRO_NAME}/setup.bash"

    # Les paquets d'exemples sont ignores : `examples_rclrs_message_demo` echoue a
    # resoudre son paquet de messages et faisait avorter `nav_msgs` et `sensor_msgs`.
    colcon build \
        --packages-ignore-regex '^examples_.*' '^rust_pubsub$' \
        --cmake-args -DCMAKE_BUILD_TYPE=Release
fi

# --- 5. Recapitulatif ------------------------------------------------------------------

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cat <<EOF

$(info "Termine")

Pour construire l'adaptateur ROS 2 et lancer la simulation :

    source /opt/ros/${ROS_DISTRO_NAME}/setup.bash
    source ${RUST_WORKSPACE}/install/setup.bash

    cd ${repository}/ros2
    colcon build
    source install/setup.bash

    ros2 launch ../simulation/launch/autonomy.launch.py

EOF

if [[ "${repository}" == /mnt/* ]]; then
    cat <<'EOF'
ATTENTION — le depot est sur le disque Windows, monte via 9p.

Cargo y ecrit des dizaines de milliers de petits fichiers, et ce systeme de fichiers
les rend dix a vingt fois plus lents. Redirige le repertoire de compilation cote Linux :

    echo 'export CARGO_TARGET_DIR=$HOME/.cache/roboto-target' >> ~/.bashrc
    source ~/.bashrc

EOF
fi

cat <<'EOF'
Si Gazebo se plaint de ne pas trouver de peripherique de rendu, force le rendu logiciel :

    export LIBGL_ALWAYS_SOFTWARE=1

EOF
