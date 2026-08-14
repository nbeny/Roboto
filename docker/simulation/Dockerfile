# Environnement de simulation : ROS 2 Jazzy + Rust + Gazebo Harmonic.
#
# Construit par-dessus l'image ROS 2, pour que la couche colcon de ros2_rust — la plus
# longue a construire — reste en cache quand seule la simulation evolue.
#
# Gazebo Harmonic arrive par les paquets « vendor » de ROS 2 Jazzy plutot que par le
# depot apt d'OSRF : c'est l'appairage officiel de Jazzy, et cela evite d'ajouter une
# source de paquets supplementaire.
FROM roboto-ros2:jazzy

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG ROS_DISTRO=jazzy

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        "ros-${ROS_DISTRO}-ros-gz" \
        "ros-${ROS_DISTRO}-xacro" \
        "ros-${ROS_DISTRO}-robot-state-publisher" \
        "ros-${ROS_DISTRO}-joint-state-publisher" \
        "ros-${ROS_DISTRO}-tf2-ros" \
        "ros-${ROS_DISTRO}-teleop-twist-keyboard" \
        # Rendu logiciel : en Harmonic, le LiDAR est un capteur `gpu_lidar` et passe
        # donc par le pipeline de rendu. Sans GPU dans le conteneur, c'est llvmpipe qui
        # s'en charge — lent, mais suffisant pour un LiDAR 2D a 10 Hz.
        libgl1-mesa-dri \
        libglx-mesa0 \
        mesa-utils \
    && rm -rf /var/lib/apt/lists/*

# Force le rendu logiciel : sans cela, ogre2 cherche un GPU et echoue.
ENV LIBGL_ALWAYS_SOFTWARE=1
ENV GALLIUM_DRIVER=llvmpipe

# Ou Gazebo cherche les modeles et les mondes du depot.
ENV GZ_SIM_RESOURCE_PATH=/workspace/simulation/models:/workspace/simulation/worlds

WORKDIR /workspace
