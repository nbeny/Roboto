# Roboto

Plateforme robotique mobile autonome open-source, **Rust-first**, bâtie autour de ROS 2.

Budget matériel cible : 300 à 500 €.

> **État : Milestone 1 — cœur Rust minimal.**
> Pas encore de matériel, pas encore de ROS 2. Le cœur métier est complet et testé.

---

## Principe

Le cœur du robot — état, contrôle, sécurité — est écrit en Rust et **ne connaît ni ROS 2,
ni le matériel, ni aucun runtime asynchrone**. C'est la décision structurante du projet :
elle confine le risque du binding ROS 2 à une seule crate remplaçable, et rend le cœur
testable partout.

```
Next.js dashboard
      │ HTTP / WebSocket
API Node.js / TypeScript
      │ ROS 2
      ▼
┌──────────────────┐        ┌──────────────────────────┐
│   robot-ros2     │◄───────│  Couche IA (Python)      │
│  (adaptateur)    │ ROS 2  │  vision · VLM · STT/TTS  │
└────────┬─────────┘        └──────────────────────────┘
         ▼
┌──────────────────┐
│   robot-core     │  machine à états, orchestration
└────────┬─────────┘
         ▼
┌──────────────────┐
│  robot-safety    │  limiteurs, watchdog, arrêt d'urgence
└────────┬─────────┘
         ▼
   robot-hal → robot-mcu → Pico 2 → driver moteur → moteurs
```

**L'IA ne pilote jamais les moteurs.** Elle parle ROS 2 comme n'importe quel client et
n'a aucun chemin d'appel vers l'abstraction matérielle : le seul producteur de vitesse
moteur est `robot-core`, et il ne produit rien qui n'ait traversé `robot-safety`. Ce
n'est pas une convention documentaire, c'est le graphe de dépendances Cargo.

---

## Démarrage

Le cœur se compile et se teste en Rust pur, sans ROS 2, sans Docker, sans WSL — y compris
sous Windows.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

L'adaptateur ROS 2 vit dans un espace colcon séparé et se construit dans un conteneur —
voir [`ros2/README.md`](ros2/README.md) :

```bash
docker build -t roboto-ros2:jazzy docker/ros2
docker run --rm -v "$PWD":/workspace -w /workspace/ros2 roboto-ros2:jazzy colcon build
docker run --rm -v "$PWD":/workspace -w /workspace roboto-ros2:jazzy bash scripts/ros2-smoke-test.sh
```

Et la simulation Gazebo — voir [`simulation/README.md`](simulation/README.md) :

```bash
docker build -t roboto-sim:jazzy docker/simulation
docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/sim-smoke-test.sh
```

---

## Crates

| Crate | Responsabilité | Milestone |
|---|---|---|
| `robot-types` | Temps monotone, vitesses, poses, commandes. Zéro dépendance. | M1 ✅ |
| `robot-safety` | Saturation vitesse/accélération, watchdog, arrêt d'urgence. | M1 ✅ |
| `robot-core` | Machine à états, autorité des commandes, boucle de contrôle. | M1 ✅ |
| `robot-telemetry` | Compteurs, instantané, logs structurés. | M1 ✅ |
| `robot_ros2` | Adaptateur ROS 2 (`rclrs`), dans [`ros2/`](ros2/). | M2 ✅ |
| `robot-hal` | Traits d'abstraction matérielle + implémentations mock. | M4 |
| `robot-mcu` | Protocole série versionné Rust ↔ Pico 2. | M4 |

---

## Deux décisions qui expliquent le reste

**Aucune horloge ambiante.** Le cœur n'appelle jamais `Instant::now()` : le temps entre
par la porte.

```rust
robot.submit_command(command, now);
let output = robot.tick(now);
```

Les tests de watchdog s'exécutent en microsecondes plutôt qu'en dormant 500 ms, ils sont
déterministes, et le même code accepte indifféremment le temps simulé de Gazebo, le temps
monotone du robot réel et le temps fabriqué des tests.

**Aucun runtime async dans le cœur.** Une boucle de contrôle temps-réel souple a besoin
d'un pas de temps borné et prévisible, pas d'un ordonnanceur work-stealing. L'async vit
aux bords — série, ROS 2, API — où Tokio est le choix par défaut de l'écosystème.

---

## Machine à états

```
BOOTING → IDLE ⇄ TELEOPERATION ⇄ PAUSED
                ⇄ NAVIGATING   ⇄ PAUSED
                ⇄ CHARGING

tout état → SAFE_STOP   (sortie toujours explicite)
tout état → ERROR
```

Le mouvement n'est autorisé qu'en `TELEOPERATION` et `NAVIGATING`. La table
d'habilitation des commandes est exhaustive sur les sources : ajouter une source casse
la compilation plutôt que d'hériter d'une autorisation par défaut.

---

## Choix techniques

| | |
|---|---|
| ROS 2 | Jazzy Jalisco (LTS → mai 2029) |
| Simulateur | Gazebo Harmonic |
| Rust | stable, edition 2024, MSRV 1.85 |
| SBC | Raspberry Pi 4B 4 Go |
| MCU | Raspberry Pi Pico 2 (RP2350, encodeurs en PIO) |
| LiDAR | RPLIDAR C1 |
| IMU | BNO085 |

Justifications détaillées et nomenclature matérielle complète :
[`docs/superpowers/specs/2026-08-14-robot-ai-core-design.md`](docs/superpowers/specs/2026-08-14-robot-ai-core-design.md).

Décisions d'architecture : [`docs/architecture/`](docs/architecture/).

---

## Roadmap

| # | Milestone | État |
|---|---|---|
| M1 | Cœur Rust minimal | ✅ |
| M2 | Adaptateur ROS 2 + nœud | ✅ |
| M3 | Simulation Gazebo | ✅ |
| M4 | Firmware Pico 2 + protocole série | |
| M5 | Bring-up robot réel | |
| M6 | SLAM + Nav2 | |
| M7 | API TypeScript + dashboard | |
| M8–M11 | Vision, voix, agent IA, produit | |

M1 à M3 ne nécessitent aucun achat. **Le matériel n'est commandé qu'après validation de
M3** — on saura alors précisément quoi commander et pourquoi.

---

## Licence

[Apache-2.0](LICENSE).
