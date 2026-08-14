# Décisions d'architecture

Chaque fichier consigne une décision structurante : le contexte qui l'a rendue nécessaire,
l'arbitrage retenu, et ce qu'on paie en échange. Une décision qui n'a rien coûté n'en était
probablement pas une.

| # | Décision | Statut |
|---|---|---|
| [0001](0001-coeur-rust-sans-ros2.md) | Le cœur Rust ne dépend pas de ROS 2 | Acceptée |
| [0002](0002-pas-d-horloge-ambiante.md) | Aucune horloge ambiante dans le cœur | Acceptée |
| [0003](0003-pas-d-async-dans-le-coeur.md) | Aucun runtime asynchrone dans le cœur | Acceptée |
| [0004](0004-ros2-jazzy-et-gazebo-harmonic.md) | ROS 2 Jazzy Jalisco et Gazebo Harmonic | Acceptée |
| [0005](0005-adaptateur-ros2-hors-workspace.md) | L'adaptateur ROS 2 vit hors du workspace Cargo | Acceptée |
| [0006](0006-horloge-monotone-et-non-murale.md) | La boucle de contrôle se cadence sur une horloge monotone | Acceptée |
| [0007](0007-marge-entre-nav2-et-la-securite.md) | Les limites de Nav2 restent à l'intérieur de l'enveloppe de sécurité | Acceptée |
| [0008](0008-watchdog-de-commande-et-robot-immobile.md) | Le watchdog de commande n'escalade que si le robot roule | Acceptée |
| [0009](0009-api-par-rosbridge.md) | L'API parle à ROS 2 par rosbridge, pas par un binding natif | Acceptée |

Le design complet — architecture, nomenclature matérielle, roadmap — vit dans
[`../superpowers/specs/2026-08-14-robot-ai-core-design.md`](../superpowers/specs/2026-08-14-robot-ai-core-design.md).
