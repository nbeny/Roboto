# 0001 — Le cœur Rust ne dépend pas de ROS 2

**Statut** : acceptée — 2026-08-14

## Contexte

Le projet est Rust-first et ROS 2 est le middleware robotique retenu. Il faut donc un
binding Rust ↔ ROS 2. Deux candidats en août 2026 :

- **`rclrs`** (ros2-rust) — direction officielle, version 0.7 sur crates.io, support
  Humble/Jazzy/Kilted/Lyrical, services, actions, timers et paramètres présents. Mais le
  projet annonce explicitement *« no stability guarantees for the moment »*, et impose
  `colcon` avec les greffons `colcon-cargo` et `colcon-ros-cargo` : plus de `cargo build`
  nu.
- **`r2r`** — 0.9.5, async-natif, `cargo build` suffit, API plus stable en pratique, mais
  projet plus confidentiel et moins aligné sur la direction officielle.

Choisir l'un ou l'autre revient à parier le cœur du robot sur un pari qu'on n'a pas les
moyens d'arbitrer aujourd'hui.

Contrainte aggravante : la machine de développement tourne sous Windows 11, où ni ROS 2 ni
Gazebo ne s'installent nativement de façon crédible.

## Décision

`robot-types`, `robot-safety`, `robot-core` et `robot-telemetry` n'ont **aucune dépendance
ROS 2**. La traduction vit dans une crate adaptateur isolée, `robot-ros2`, introduite en
Milestone 2 et bâtie sur `rclrs`.

## Conséquences

**Ce qu'on gagne.** Le risque du binding est confiné : basculer vers `r2r` devient la
réécriture d'un fichier, pas du robot. `cargo test --workspace` tourne sous Windows sans
ROS, sans Docker, sans WSL — ce qui rend la Milestone 1 livrable immédiatement. La CI Rust
s'exécute sur un `ubuntu-latest` nu en quelques secondes. Et le même code métier tourne à
l'identique en simulation, sur le robot réel et en test unitaire.

**Ce qu'on paie.** Une couche de traduction à écrire et à maintenir : `geometry_msgs/Twist`
vers `MotionCommand`, `RobotState` vers un message de diagnostic, etc. Coût réel mais
borné, et qui documente explicitement la frontière plutôt que de la laisser se dissoudre.

**Ce qui est interdit.** Ajouter une dépendance ROS 2 à l'une des quatre crates du cœur.
La CI le vérifie indirectement en compilant et testant sur Windows.
