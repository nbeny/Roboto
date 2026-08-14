#!/usr/bin/env bash
# Source les deux espaces avant toute commande : celui de ROS 2 Jazzy, puis celui de
# ros2_rust qui fournit `rclrs` et les crates de messages generees.
set -eo pipefail

# Les scripts `setup.bash` de ROS 2 lisent des variables non definies
# (AMENT_TRACE_SETUP_FILES, COLCON_TRACE...). `set -u` les ferait echouer, on le
# desactive donc pendant le sourcing uniquement.
set +u
source "/opt/ros/${ROS_DISTRO:-jazzy}/setup.bash"
source /opt/ros2_rust_ws/install/setup.bash
set -u

exec "$@"
