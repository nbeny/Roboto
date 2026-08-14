# Simulation

Modèle Gazebo Harmonic du robot, piloté par le cœur Rust à travers ROS 2.

```
teleop ──► /cmd_vel ──► robot_core ──► robot-safety ──► ~/cmd_vel_safe
                                                              │
                                                        ros_gz_bridge
                                                              │
                                                  gz /roboto/cmd_vel ──► roues
```

**Le robot simulé n'écoute que `/roboto/cmd_vel`**, et le pont n'y recopie que la sortie
post-sécurité du cœur. Il n'existe aucun chemin permettant de piloter les roues en
contournant `robot-safety` — la contrainte architecturale du projet tient jusque dans le
simulateur.

---

## Lancer

Deux lancements, selon ce qu'on veut faire.

| Lancement | Contenu |
|---|---|
| `simulation.launch.py` | Gazebo, le robot, le pont, le cœur Rust — téléopération |
| `autonomy.launch.py` | tout ce qui précède, plus SLAM et Nav2 — navigation autonome |

### Dans Docker

```bash
docker build -t roboto-sim:jazzy docker/simulation      # une fois
docker run --rm -it -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash

# Dans le conteneur
colcon build --base-paths ros2          # si l'adaptateur n'est pas déjà construit
source ros2/install/setup.bash
ros2 launch simulation/launch/autonomy.launch.py
```

Sous PowerShell, remplacer `"$PWD"` par `"${PWD}"`.

### Nativement dans WSL2

```bash
bash scripts/setup-wsl2.sh              # installe Nav2, SLAM, Rust, ros2_rust
```

Le script est idempotent et n'installe que ce qui manque. Il signale aussi le piège de
performance : si le dépôt est sur le disque Windows, Cargo y écrit des dizaines de
milliers de petits fichiers à travers 9p, et il vaut mieux rediriger `CARGO_TARGET_DIR`
côté Linux.

### Essais automatiques

```bash
docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/sim-smoke-test.sh
docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/nav-smoke-test.sh
```

### Envoyer un but

```bash
ros2 action send_goal /navigate_to_pose nav2_msgs/action/NavigateToPose \
  "{pose: {header: {frame_id: 'map'}, pose: {position: {x: 1.5, y: -1.0}, orientation: {w: 1.0}}}}"
```

Si le robot ne bouge pas, la première chose à vérifier est son état : le cœur n'accepte
les consignes de Nav2 qu'en `NAVIGATING`.

```bash
ros2 topic echo /robot_core/state --once
ros2 topic pub --once /robot_core/request_state std_msgs/msg/String "{data: 'NAVIGATING'}"
```

---

## Le robot

| Élément | Valeur | Correspondance BOM |
|---|---|---|
| Roues | ⌀ 85 mm, empattement 25 cm | moteurs JGB37-520 + roues 85 mm |
| Châssis | 30 × 22 × 8 cm, 2,6 kg | plaques + calculateur + batterie |
| LiDAR | 360°, 12 m, zone aveugle 5 cm, 10 Hz | RPLIDAR C1 |
| IMU | 100 Hz | BNO085 |
| Caméra | 640×480, 15 Hz | webcam USB UVC |

Les dimensions suivent la nomenclature réelle. Un modèle qui ment sur son empattement
produit une odométrie qui ment aussi, et tout le réglage fait en simulation devient
inutilisable sur le robot réel.

---

## Topics

| ROS 2 | Gazebo | Sens |
|---|---|---|
| `/clock` | `/clock` | gz → ROS |
| `/robot_core/cmd_vel_safe` | `/roboto/cmd_vel` | **ROS → gz** |
| `/odom` | `/roboto/odom` | gz → ROS |
| `/tf` | `/roboto/tf` | gz → ROS |
| `/joint_states` | `/roboto/joint_states` | gz → ROS |
| `/scan` | `/roboto/scan` | gz → ROS |
| `/imu` | `/roboto/imu` | gz → ROS |
| `/camera/image_raw` | `/roboto/camera` | gz → ROS |

---

## Temps simulé

Le nœud tourne avec `use_sim_time:=true` : sa référence est `/clock`, publié par Gazebo.
`rclrs` s'en charge — son `TimeSource` interne s'abonne à `/clock` et pilote l'horloge du
nœud.

Hors simulation, le nœud utilise délibérément une horloge **monotone** et non celle du
nœud : le temps ROS est adossé à l'horloge murale, qui recule. Voir
[ADR 0006](../docs/architecture/0006-horloge-monotone-et-non-murale.md).

Conséquence pratique pour les outils en ligne de commande : tout ce qui lit des données
horodatées en temps simulé doit recevoir `--ros-args -p use_sim_time:=true`. Sans cela,
`tf2_echo` juge toutes les transformées hors délai.

---

## Rendu sans GPU

En Harmonic, il n'existe pas de LiDAR CPU : `gpu_lidar` passe par le pipeline de rendu.
L'image force donc le rendu logiciel (`LIBGL_ALWAYS_SOFTWARE=1`, llvmpipe). Cela
fonctionne, mais coûte : le LiDAR tourne autour de 8,5 Hz au lieu des 10 Hz demandés.

---

## Navigation autonome

```
Nav2 ──► velocity_smoother ──► /nav/cmd_vel ──► robot_core ──► robot-safety
                                                                    │
                                                             ~/cmd_vel_safe ──► roues
```

`slam_toolbox` tourne en mode `mapping` : la carte se construit pendant que le robot
navigue, ce qui évite d'avoir à cartographier puis relancer avec `amcl` et une carte
enregistrée.

**Nav2 publie sur `/nav/cmd_vel`, jamais sur `/cmd_vel`.** C'est le topic d'arrivée qui
détermine la source, donc l'autorité : une consigne arrivant là est traitée comme
`CommandSource::Navigation` et n'est acceptée qu'en état `NAVIGATING`. La téléopération
garde `/cmd_vel` et son autorité propre.

**Les limites de Nav2 sont strictement à l'intérieur de l'enveloppe de sécurité** —
0,35 m/s contre 0,50. Ce n'est pas de la prudence décorative : voir
[ADR 0007](../docs/architecture/0007-marge-entre-nav2-et-la-securite.md).

---

## Diagnostic

**Le robot ne bouge pas alors que Nav2 planifie.** Vérifier l'état du cœur en premier :
il refuse les consignes de navigation hors `NAVIGATING`, et ne le signale qu'en niveau
`debug`.

```bash
ros2 topic echo /robot_core/state --once
```

**`/map` n'est jamais publié.** `slam_toolbox` est un nœud à cycle de vie. Sans
gestionnaire pour le configurer puis l'activer, il démarre, reste en `unconfigured`, ne
s'abonne jamais à `/scan` — et n'émet aucun avertissement. `autonomy.launch.py` lui dédie
un `lifecycle_manager_slam` pour cette raison.

```bash
ros2 lifecycle get /slam_toolbox     # doit répondre « active »
```

**Les transformées semblent hors délai.** Tout outil en ligne de commande lisant des
données horodatées en temps simulé doit recevoir `--ros-args -p use_sim_time:=true`.

---

## Limites connues

- Les liens reliés par des articulations fixes sont fusionnés lors de la conversion
  URDF → SDF : Gazebo rattache les capteurs à `base_footprint`. Les poses restent
  correctes et `gz_frame_id` préserve le repère annoncé dans les messages, mais les
  avertissements au démarrage viennent de là.
- La boucle de contrôle se cadence sur l'horloge murale même sous temps simulé. La
  logique du cœur, elle, est entièrement pilotée par l'instant horodaté qu'on lui passe :
  seule la fréquence d'échantillonnage dériverait, et uniquement à un facteur temps réel
  différent de 1.
- Pas d'interface graphique : le serveur tourne seul. Pour l'affichage, lancer `gz sim -g`
  depuis un environnement disposant d'un serveur X.
- L'image pèse environ 8 Go. `npm`, installé depuis apt pour exécuter l'API dans le même
  environnement, y tire à lui seul plusieurs centaines de paquets `node-*` dont rien
  n'a besoin ici — l'API est compilée côté hôte. À remplacer par une installation de
  Node seul.
