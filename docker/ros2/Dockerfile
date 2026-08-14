# Environnement de developpement ROS 2 Jazzy + Rust.
#
# Pourquoi une image dediee plutot qu'un simple `cargo build` : les crates Rust des
# messages ROS 2 (`geometry_msgs`, `std_msgs`...) ne sont pas distribuables. La seule
# version publiee sur crates.io a ete yankee en avril 2024. Elles sont **generees** par
# `rosidl_rust` au moment d'un `colcon build`, a partir des fichiers `.idl` des paquets
# d'interfaces. Il faut donc reconstruire ces paquets depuis les sources avec le
# generateur Rust dans la boucle.
#
# C'est le cout d'integration annonce par docs/architecture/0001-coeur-rust-sans-ros2.md,
# et la raison pour laquelle il reste confine a `ros2/src/robot_ros2` : le coeur du robot,
# lui, se compile avec un `cargo test` nu.

FROM ros:jazzy

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG ROS_DISTRO=jazzy
ARG RUST_TOOLCHAIN=stable

# libclang-dev est requis par bindgen, qui genere les liaisons vers `rcl`.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        curl \
        git \
        libclang-dev \
        python3-pip \
        python3-vcstool \
        "ros-${ROS_DISTRO}-example-interfaces" \
        "ros-${ROS_DISTRO}-test-msgs" \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- --default-toolchain "${RUST_TOOLCHAIN}" -y --profile minimal \
        --component rustfmt --component clippy
ENV PATH=/root/.cargo/bin:$PATH

# colcon-ros-cargo delegue a cargo-ament-build pour installer les artefacts Rust
# dans l'espace d'installation colcon.
RUN pip install --break-system-packages colcon-cargo colcon-ros-cargo \
    && cargo install cargo-ament-build

# Espace colcon dedie a ros2_rust. Separe de l'espace applicatif pour qu'une
# recompilation du noeud ne redeclenche pas celle des generateurs.
WORKDIR /opt/ros2_rust_ws
RUN mkdir -p src \
    && git clone --depth 1 https://github.com/ros2-rust/ros2_rust.git src/ros2_rust \
    && vcs import src < src/ros2_rust/ros2_rust_${ROS_DISTRO}.repos

# Les paquets d'exemples sont ignores, pas supprimes : `examples_rclrs_message_demo`
# echoue a resoudre son paquet de messages et faisait avorter `nav_msgs` et
# `sensor_msgs`, dont la Milestone 3 aura besoin. Les sources restent disponibles dans
# l'image comme banc d'essai pour diagnostiquer la resolution des crates generees.
RUN source "/opt/ros/${ROS_DISTRO}/setup.bash" \
    && colcon build \
        --event-handlers console_direct+ \
        --packages-ignore-regex '^examples_.*' '^rust_pubsub$' \
        --cmake-args -DCMAKE_BUILD_TYPE=Release

# Sourcer les deux espaces a chaque shell interactif ou non interactif.
RUN echo "source /opt/ros/${ROS_DISTRO}/setup.bash" >> /etc/bash.bashrc \
    && echo "source /opt/ros2_rust_ws/install/setup.bash" >> /etc/bash.bashrc

WORKDIR /workspace
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["bash"]
