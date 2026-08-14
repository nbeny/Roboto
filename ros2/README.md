# Espace ROS 2

Espace colcon contenant l'adaptateur entre ROS 2 et le cœur Rust.

> **Ce répertoire n'est pas un workspace Cargo.** Il est explicitement exclu de celui de
> la racine. Les crates de messages ROS 2 sont générées localement par `rosidl_rust` et
> n'existent pas sur crates.io — voir
> [ADR 0005](../docs/architecture/0005-adaptateur-ros2-hors-workspace.md).

---

## Construire et lancer

Tout passe par l'image `docker/ros2/`, qui embarque ROS 2 Jazzy, Rust, les greffons
colcon et l'espace `ros2_rust` déjà construit.

```bash
# Une fois : construire l'image (~5 min)
docker build -t roboto-ros2:jazzy docker/ros2

# Ouvrir un shell dans l'espace de travail
docker run --rm -it -v "$PWD":/workspace roboto-ros2:jazzy bash

# Dans le conteneur
cd /workspace/ros2
colcon build
source install/setup.bash
ros2 run robot_ros2 robot_node
```

Sous PowerShell, remplacer `"$PWD"` par `"${PWD}"`.

---

## Interface du nœud `robot_core`

| Direction | Nom | Type | Rôle |
|---|---|---|---|
| entrée | `/cmd_vel` | `geometry_msgs/Twist` | téléopération |
| entrée | `/nav/cmd_vel` | `geometry_msgs/Twist` | pile de navigation |
| entrée | `~/request_state` | `std_msgs/String` | transition d'état par nom |
| sortie | `~/state` | `std_msgs/String` | état opérationnel, 50 Hz |
| sortie | `~/cmd_vel_safe` | `geometry_msgs/Twist` | consigne après la couche de sécurité |
| service | `~/emergency_stop` | `std_srvs/Trigger` | engage l'arrêt d'urgence |
| service | `~/clear_emergency_stop` | `std_srvs/Trigger` | lève l'arrêt d'urgence |
| service | `~/clear_safe_stop` | `std_srvs/Trigger` | quitte `SAFE_STOP` |

**Deux topics de commande, pas un.** C'est le topic d'arrivée qui détermine la source,
donc l'autorité. Un `/cmd_vel` unique partagé entre téléopération et navigation rendrait
la table d'habilitation du cœur inopérante : le robot ne pourrait plus distinguer qui
commande.

---

## Essai en téléopération

Trois terminaux dans le conteneur (ou `docker exec`) :

```bash
# 1 — le nœud
ros2 run robot_ros2 robot_node

# 2 — passer en téléopération, puis observer
ros2 topic pub --once /robot_core/request_state std_msgs/String "{data: 'TELEOPERATION'}"
ros2 topic echo /robot_core/state

# 3 — piloter
ros2 run teleop_twist_keyboard teleop_twist_keyboard
```

Cesser d'appuyer sur une touche pendant plus de 500 ms fait basculer l'état en
`SAFE_STOP` : c'est le watchdog de commande qui fait son travail. Pour repartir :

```bash
ros2 service call /robot_core/clear_safe_stop std_srvs/srv/Trigger
ros2 topic pub --once /robot_core/request_state std_msgs/String "{data: 'TELEOPERATION'}"
```

---

## Limites connues

- La boucle de contrôle cadence sur l'horloge murale tout en horodatant avec l'horloge
  ROS 2. Sous simulation à facteur temps réel différent de 1, la cadence sera fausse.
  À reprendre en Milestone 3 avec un timer ROS 2.
- `~/request_state` est un topic, pas un service, pour éviter de générer un paquet
  d'interfaces sur mesure. À remplacer quand la Milestone 7 fixera le contrat d'API.
