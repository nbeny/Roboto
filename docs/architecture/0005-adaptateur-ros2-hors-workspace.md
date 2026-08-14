# 0005 — L'adaptateur ROS 2 vit hors du workspace Cargo

**Statut** : acceptée — 2026-08-14

## Contexte

La [décision 0001](0001-coeur-rust-sans-ros2.md) plaçait l'adaptateur ROS 2 dans une
crate isolée. Restait à savoir où : membre du workspace Cargo racine, ou espace séparé ?

L'enquête menée avant d'écrire la moindre ligne a tranché à notre place.

**Les crates Rust des messages ROS 2 ne sont pas distribuables.** `geometry_msgs` existe
bien sur crates.io, mais sa seule version réelle — 4.2.3, publiée le 18 avril 2024 — a
été yankée le jour même. Ce qui subsiste est un placeholder en 0.0.0. Même situation pour
`std_msgs` et les autres.

Ces crates sont **générées** par `rosidl_rust` à partir des fichiers `.idl`, au moment
d'un `colcon build`, et déposées dans l'espace d'installation colcon. Elles dépendent donc
de la distribution ROS 2 installée, et n'ont pas d'existence hors de cet espace.

Un membre du workspace racine déclarant `geometry_msgs = "*"` ferait échouer
`cargo test --workspace` sur la machine de développement, où aucun espace colcon n'existe.
Ce serait la perte exacte de la propriété que la décision 0001 cherchait à obtenir.

## Décision

`ros2/src/robot_ros2` est un **espace de travail Cargo autonome**, déclaré par un
`[workspace]` vide dans son propre `Cargo.toml`, et le workspace racine l'exclut
explicitement via `exclude = ["ros2"]`.

Il dépend du cœur par des dépendances `path`, ce qui fonctionne sans difficulté : une
crate externe peut référencer par chemin un membre d'un autre workspace.

Sa construction passe par `colcon build` dans l'image `docker/ros2/`.

## Conséquences

**Ce qu'on préserve.** `cargo test --workspace` continue de tourner sous Windows, sans
ROS 2, sans Docker, sans WSL. C'était l'objectif, il tient.

**Ce qu'on paie.** Deux chaînes de construction au lieu d'une, et deux commandes de test.
La CI y répond par deux jobs distincts. Le `Cargo.lock` de l'adaptateur n'est pas versionné :
il référencerait des crates générées dont les versions dépendent de la distribution ROS 2
locale, ce qui le rendrait faux pour tout le monde sauf son auteur.

**Un piège à connaître.** Les paquets d'exemples de `ros2-rust/examples` ne se construisent
pas en l'état : `examples_rclrs_message_demo` échoue à résoudre son propre paquet de
messages, avec `no matching package named rclrs_example_msgs found, location searched:
crates.io index`. Comme colcon interrompt la construction sur échec, il entraînait avec lui
`nav_msgs` et `sensor_msgs` — dont la Milestone 3 aura besoin. L'image les ignore
explicitement, sans supprimer les sources : elles servent de banc d'essai pour diagnostiquer
la résolution des crates générées.
