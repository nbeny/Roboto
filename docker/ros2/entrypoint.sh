#!/usr/bin/env bash
# Source les deux espaces avant toute commande : celui de ROS 2 Jazzy, puis celui de
# ros2_rust qui fournit `rclrs` et les crates de messages generees.
set -euo pipefail

source "/opt/ros/${ROS_DISTRO:-jazzy}/setup.bash"
source /opt/ros2_rust_ws/install/setup.bash

exec "$@"
