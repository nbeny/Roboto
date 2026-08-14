# 0004 — ROS 2 Jazzy Jalisco et Gazebo Harmonic

**Statut** : acceptée — 2026-08-14

## Contexte

Trois distributions ROS 2 étaient envisageables en août 2026 :

| Distribution | Sortie | Fin de support |
|---|---|---|
| Jazzy Jalisco (LTS) | mai 2024 | mai 2029 |
| Kilted Kaiju | mai 2025 | novembre 2026 |
| Lyrical Luth (LTS) | mai 2026 | mai 2031 |

Kilted meurt dans trois mois : écartée d'office.

Restait l'arbitrage entre Jazzy et Lyrical. Lyrical offre deux ans de support
supplémentaires et s'appaire à Gazebo Jetty (fin de vie mai 2031). Mais il a trois mois
d'âge : les binaires Nav2 et slam_toolbox arrivent, tandis que la masse des tutoriels, des
drivers tiers et des réponses Discourse reste sur Jazzy.

## Décision

**ROS 2 Jazzy Jalisco**, sur Ubuntu 24.04, appairé à **Gazebo Harmonic** via `ros_gz`.

## Justification

Pour un premier robot, le temps perdu en débogage d'intégration coûte bien plus cher que
deux ans de support en fin de vie. Jazzy conserve trois ans de support — largement au-delà
des Phases 0 à 5 du projet — et offre l'écosystème le plus dense : Nav2, slam_toolbox,
`ros2_control`, `ros_gz`, drivers LiDAR. `rclrs` le supporte au même titre que Lyrical.

Gazebo Harmonic découle du choix précédent : c'est l'appairage officiel de Jazzy, il est
supporté jusqu'en septembre 2028, et ses greffons diff-drive, LiDAR, IMU et caméra sont
natifs. Isaac Sim a été écarté : il exige un GPU NVIDIA et apporte une complexité sans
contrepartie pour un robot d'intérieur à entraînement différentiel.

## Conséquences

**Ce qu'on paie.** Une migration vers Lyrical — ou son successeur — sera nécessaire d'ici
2029. Le coût en est délibérément contenu par
[la décision 0001](0001-coeur-rust-sans-ros2.md) : seule la crate `robot-ros2` est
concernée, le cœur métier ne bouge pas.

**Contrainte de développement.** ROS 2 et Gazebo tournent dans WSL2 / Ubuntu 24.04, pas
nativement sous Windows. Le cœur Rust, lui, reste testable en natif.

## À revoir

Cette décision sera réexaminée avant la Milestone 6 (SLAM + Nav2). Si l'écosystème
Lyrical a mûri d'ici là et que la migration reste peu coûteuse, elle deviendra préférable
au moment où le projet s'installe dans la durée.
