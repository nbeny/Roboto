# Roboto — Design du robot mobile autonome

**Date** : 2026-08-14
**Statut** : validé
**Portée** : architecture globale, choix techniques, BOM, roadmap, spécification de la Milestone 1

---

## 1. Objectif

Construire une plateforme robotique mobile autonome open-source, avec un budget matériel
de 300 à 500 €, dont le cœur métier est écrit en Rust.

Le projet est **Rust-first, pas Rust-exclusif** : Rust pour tout ce qui touche à la sécurité,
au contrôle et à l'état ; Python pour l'IA/ML là où son écosystème est supérieur ;
TypeScript/Next.js pour l'API et le dashboard ; C/C++ uniquement lorsqu'un composant
ROS 2 ou hardware existant le justifie.

Ordre de priorité, non négociable :

```
Sécurité → Fiabilité → Robot fonctionnel → Autonomie → Perception → IA → Fonctionnalités avancées
```

---

## 2. Contexte de départ

- Dépôt `github.com/nbeny/Roboto` vide au 2026-08-14 : aucun commit, aucun fichier.
- Machine de développement : **Windows 11**, `rustc 1.97.1` stable, hôte `x86_64-pc-windows-msvc`.
- Calculateur embarqué déjà possédé : **Raspberry Pi 4 Model B 4 Go** (0 € dans la BOM).

La contrainte Windows est structurante : ni ROS 2 ni Gazebo n'y tournent nativement de
façon crédible. Elle motive directement la décision d'architecture centrale (§4.1).

---

## 3. Architecture globale

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

### Propriété de sécurité fondamentale

L'IA parle ROS 2 comme n'importe quel autre client. Elle n'a **aucun chemin d'appel**
vers `robot-hal` : le seul producteur de vélocité moteur est `robot-core`, et
`robot-core` ne produit une vélocité qu'en sortie de `robot-safety`.

Ce n'est pas une convention documentaire, c'est une contrainte du graphe de dépendances
Cargo : `robot-hal` ne dépend pas de la couche IA, et rien dans le workspace n'expose
de chemin permettant de la contourner.

---

## 4. Décisions d'architecture

### 4.1 Le cœur ne connaît pas ROS 2

`robot-types`, `robot-safety`, `robot-core` et `robot-telemetry` n'ont **aucune
dépendance ROS 2**. La traduction vit dans une crate adaptateur isolée, `robot-ros2`.

Motivations, par ordre d'importance :

1. **Le binding Rust↔ROS 2 est le point de risque du projet.** `rclrs` annonce
   explicitement l'absence de garantie de stabilité d'API et impose `colcon` +
   `colcon-ros-cargo`. Confiner ce risque à une crate rend le remplacement par `r2r`
   une réécriture d'un fichier, pas du robot.
2. **Testabilité.** `cargo test --workspace` tourne sous Windows, sans ROS installé,
   sans Docker, sans WSL, sans matériel.
3. **CI triviale.** Le job Rust s'exécute sur `ubuntu-latest` nu, en quelques secondes.
4. **Un seul cœur pour trois environnements.** Simulation, robot réel et tests unitaires
   exécutent exactement le même code métier.

### 4.2 Aucune horloge ambiante dans le cœur

Le cœur n'appelle **jamais** `Instant::now()`. Le temps est un paramètre d'entrée :

```rust
robot.submit_command(cmd, now);
let output = robot.tick(now);
```

Conséquences : les tests de watchdog s'exécutent en microsecondes plutôt qu'en dormant
500 ms, ils sont déterministes et non-flaky, et le même code accepte indifféremment le
temps simulé de Gazebo, le temps monotone du robot réel et le temps fabriqué des tests.

Le type `Monotonic` est un newtype sur `Duration` depuis le démarrage. Les soustractions
sont saturantes : un temps qui recule ne provoque jamais de panique.

### 4.3 Aucun runtime async dans le cœur

`robot-types`, `robot-safety`, `robot-core` et `robot-telemetry` sont **synchrones**.

Une boucle de contrôle temps-réel souple n'a rien à gagner d'un exécuteur : elle a besoin
d'un pas de temps borné et prévisible, pas d'un ordonnanceur work-stealing. Imposer un
runtime au cœur reviendrait à l'imposer à tous ses consommateurs.

L'async vit uniquement aux bords — I/O série, ROS 2, client API — où **Tokio** est le
choix par défaut de l'écosystème. Bénéfice secondaire : le cœur reste compatible
`no_std` à terme, donc partageable avec le firmware Pico si l'on passe à Rust embarqué.

### 4.4 Découpage en crates

```
crates/
├── robot-types/       types fondamentaux, zéro dépendance     │ M1
├── robot-safety/      limiteurs, watchdog, arrêt d'urgence    │ M1
├── robot-core/        machine à états + orchestration         │ M1
├── robot-telemetry/   snapshot + pont événements→logs         │ M1
├── robot-hal/         traits hardware + implémentations mock  │ M4
├── robot-mcu/         protocole série versionné Rust↔Pico     │ M4
├── robot-ros2/        adaptateur rclrs                        │ M2
└── robot-node/        binaire d'assemblage                    │ M2
```

Graphe strictement acyclique et descendant :
`types ← safety ← core ← telemetry`, puis `hal` / `mcu` / `ros2` se branchent sur `core`
sans que `core` ne les connaisse.

`robot-state`, `robot-control` et `robot-navigation` de la proposition initiale sont
**fusionnés dans `robot-core`**. Séparer l'état des transitions qui le modifient produit
trois crates anémiques et force à exposer publiquement des invariants qui devraient
rester privés. La navigation, à ce stade, c'est Nav2 qui publie un `/cmd_vel` — pas une
crate Rust.

---

## 5. Choix techniques

| Décision | Choix | Justification |
|---|---|---|
| ROS 2 | **Jazzy Jalisco** | LTS jusqu'à mai 2029, Ubuntu 24.04. Écosystème le plus dense (Nav2, slam_toolbox, ros2_control, ros_gz). Kilted meurt en nov. 2026 ; Lyrical Luth, sorti en mai 2026, n'a pas encore l'écosystème tiers. |
| Rust | **stable** (1.97.1 au moment de la rédaction), edition 2024, MSRV 1.85 | `rust-toolchain.toml` fixe le canal `stable` ; le plancher de compatibilité est porté par `rust-version` et la reproductibilité des dépendances par `Cargo.lock`. Épingler une version exacte pénaliserait les contributeurs sans bénéfice réel. |
| Runtime async | **Aucun dans le cœur**, Tokio aux bords | Cf. §4.3. |
| Binding ROS 2 | **`rclrs`** derrière l'adaptateur | Direction officielle du projet, actions/services/paramètres présents. Instabilité confinée à une crate. |
| Simulateur | **Gazebo Harmonic** | Appairage officiel avec Jazzy via `ros_gz`, LTS jusqu'à sept. 2028, plugins natifs diff-drive/LiDAR/IMU/caméra. Isaac Sim écarté : GPU NVIDIA requis, complexité sans contrepartie ici. |
| SBC | **Raspberry Pi 4B 4 Go** | Possédé. Suffisant pour LiDAR 2D + slam_toolbox + Nav2 + cœur Rust. Builds Rust en cross-compilation depuis Windows, jamais sur la cible. |
| MCU | **Raspberry Pi Pico 2 (RP2350)** | Les 12 machines à états **PIO** décodent la quadrature des encodeurs en matériel : pas de coût CPU, pas d'interruption ratée à haute vitesse. Aucun ESP32 ni STM32 de ce prix n'offre l'équivalent. Plus : watchdog matériel, double Cortex-M33 150 MHz, USB CDC natif. |
| LiDAR | **RPLIDAR C1** | Driver ROS 2 officiel et maintenu (`sllidar_ros2`), DTOF 12 m, zone aveugle 5 cm. Le LD19 économise 30 € contre un driver communautaire : mauvais arbitrage pour un premier robot. |
| IMU | **BNO085** | Fusion de capteurs sur puce, sortie quaternion directe. Un MPU6050 à 3 € impose d'écrire et de tuner un filtre de Madgwick. |
| Caméra | **Webcam USB UVC** | `v4l2_camera` fonctionne immédiatement. Le CSI Pi Camera 3 impose libcamera sur Ubuntu arm64, friction réelle pour un gain nul en Phase 5. |
| Driver moteur | **Cytron MDD3A** | Disponible en Europe, borniers à vis, documentation sérieuse. Le L298N est obsolète et dissipe ~2 V par pont. |
| Licence | **Apache-2.0** | Concession de brevets explicite, standard de fait en robotique open-source. |

### Environnement de développement

- **Cœur Rust** : natif Windows, `cargo test` / `cargo clippy` directement.
- **ROS 2, Gazebo, Nav2** : WSL2 + Ubuntu 24.04, ou Docker.
- **Robot** : Raspberry Pi OS 64-bit ou Ubuntu Server 24.04 arm64, binaires cross-compilés.

---

## 6. Nomenclature matérielle (BOM)

Ordres de grandeur TTC constatés en Europe en août 2026. **À revalider au moment de la
commande** : le marché des composants est volatil (Raspberry Pi a relevé ses tarifs en
février 2026 en raison de la pénurie de mémoire liée à l'IA).

| Poste | Composant | € |
|---|---|---|
| Châssis | Plaques alu/PMMA, entretoises, visserie M3 | 45 |
| Motorisation | 2× moteur 12 V à réducteur + encodeur Hall (JGB37-520 ou Pololu 37D) | 45 |
| | 2× roues 85 mm | 12 |
| | 2× roulettes folles | 8 |
| | Cytron MDD3A (double pont 3 A) | 17 |
| Compute | Raspberry Pi 4B 4 Go — **possédé** | 0 |
| | microSD 64 Go A2 + dissipateur | 20 |
| Contrôle | 2× Raspberry Pi Pico 2 (dont 1 de rechange) | 12 |
| Capteurs | RPLIDAR C1 | 100 |
| | IMU BNO085 | 32 |
| | Webcam USB UVC | 25 |
| Énergie | Pack 3S Li-ion 11,1 V ~5 Ah avec BMS + XT60 | 42 |
| | Chargeur équilibreur 3S | 18 |
| | Convertisseur 12 V→5 V 5 A | 15 |
| Sécurité | Arrêt d'urgence champignon NC + relais de coupure puissance | 18 |
| | Fusible, porte-fusible, interrupteur général | 8 |
| Divers | Câblage, connecteurs, borniers, gaines | 25 |
| | Supports LiDAR/caméra imprimés 3D | 15 |
| | **Total** | **457** |

**Variante économique ~374 €** : LD19 au lieu du C1 (−30), ICM-20948 au lieu du BNO085
(−17), caméra reportée en Phase 5 (−25), chargeur simple (−11).

### Points d'attention

- L'arrêt d'urgence **coupe physiquement la puissance moteur par relais**. Il ne passe
  pas par le logiciel. Le `SAFE_STOP` logiciel du cœur Rust en est le complément, jamais
  le substitut.
- Le Pi 4 réclame 5 V / 3 A stables. C'est la qualité du convertisseur qui décide si le
  robot redémarre en pleine navigation.

Hors BOM, assumé pour la Phase 8 : station de recharge, deuxième batterie, plateau
supérieur.

---

## 7. Roadmap

| # | Milestone | Critère de succès vérifiable | Phase |
|---|---|---|---|
| **M1** | Rust Robot Core minimal | `cargo test --workspace` et `cargo clippy -- -D warnings` verts, sous Windows, sans ROS ni matériel | 0-1 |
| M2 | Adaptateur ROS 2 + nœud | `/cmd_vel` → cœur → `/robot/state` publié ; téléop clavier pilote la machine à états | 1 |
| M3 | Simulation Gazebo | URDF diff-drive + LiDAR + IMU ; odométrie et TF cohérentes ; le robot simulé obéit au cœur Rust | 2 |
| M4 | Firmware Pico 2 + protocole | Encodeurs en PIO, PID vitesse, watchdog ; protocole série versionné ; tests de boucle Rust↔MCU | 3 |
| M5 | Bring-up robot réel | Le robot roule en téléop, odométrie fermée, E-stop coupe la puissance | 3 |
| M6 | SLAM + Nav2 | Carte construite, localisation stable, navigation vers un but ; Nav2 passe par la couche safety | 4 |
| M7 | API TS + dashboard | Statut/pose/batterie temps réel, bouton STOP fonctionnel | — |
| M8 | Vision | Détection d'objets publiée en ROS 2 | 5 |
| M9 | Voix | STT/TTS, commandes vocales simples | 6 |
| M10 | Agent IA | Tool calling → `navigate_to`, `inspect`, `stop`, `get_robot_state` | 7 |
| M11 | Produit | Docking, recharge, OTA, diagnostics | 8 |

M1 à M3 ne coûtent rien et ne nécessitent aucun achat. **Le matériel n'est commandé
qu'après validation de M3.**

---

## 8. Spécification de la Milestone 1

### 8.1 Périmètre

Le cœur Rust minimal doit :

- représenter l'état du robot ;
- recevoir une commande de mouvement ;
- appliquer les limites de sécurité ;
- gérer un timeout de commande ;
- passer en `SAFE_STOP` ;
- exposer des événements et des logs structurés ;
- être entièrement testable sans matériel.

**Hors périmètre** : ROS 2, hardware, navigation, perception, API, frontend, simulation.

### 8.2 `robot-types`

Zéro dépendance externe (hormis `serde` en feature optionnelle).

| Type | Rôle |
|---|---|
| `Monotonic` | Instant monotone depuis le démarrage. Newtype sur `Duration`, soustraction saturante. |
| `Velocity2d` | `linear` (m/s, positif = avant), `angular` (rad/s, positif = trigonométrique). |
| `Pose2d` | `x`, `y` (m), `theta` (rad). |
| `CommandSource` | `Teleoperation`, `Navigation`, `Ai`, `Internal`. |
| `MotionCommand` | `velocity` + `source` + `issued_at`. |

### 8.3 `robot-safety`

**`SafetyLimits`** — vitesses et accélérations maximales, timeout de commande.
Valeurs par défaut délibérément conservatrices : 0,5 m/s, 1,0 rad/s, 0,5 m/s²,
1,5 rad/s², timeout 500 ms. Validées à la construction : toutes finies et strictement
positives.

**`Watchdog`** — timeout générique, réutilisable pour le heartbeat de communication en
M4. Un watchdog jamais nourri n'est **pas** considéré comme expiré ; c'est le cœur qui
établit la ligne de base en le nourrissant à l'entrée d'un état de mouvement.

**`SafetyLayer`** — compose limiteurs, watchdog et arrêt d'urgence.

```rust
pub enum MotionAuthority {
    Allowed(Velocity2d),  // l'état autorise le mouvement
    Denied,               // l'état l'interdit → zéro immédiat
}

pub fn evaluate(&mut self, authority: MotionAuthority, now: Monotonic) -> SafetyDecision
```

Ordre d'évaluation, strictement :

1. Arrêt d'urgence engagé → `EmergencyStopEngaged`, `SAFE_STOP` requis. **Priorité
   absolue** : ce contrôle précède celui de l'autorité, de sorte qu'un arrêt d'urgence
   déclenché à l'arrêt (`Idle`, `Charging`) force malgré tout le passage en `SafeStop`.
2. `Denied` → vélocité nulle immédiate, aucune violation (c'est nominal).
3. Vélocité non finie (NaN/∞) → `NonFiniteCommand`, `SAFE_STOP` requis.
4. Watchdog de commande expiré → `CommandTimeout`, `SAFE_STOP` requis.
5. Si `SAFE_STOP` requis → vélocité nulle immédiate, court-circuitant le limiteur
   d'accélération.
6. Saturation des vitesses.
7. Saturation des accélérations sur `dt` écoulé depuis la dernière évaluation.

Un arrêt de sécurité produit une vélocité **nulle immédiate**, sans rampe. La
décélération physique est bornée par la rampe côté MCU et l'inertie des moteurs. Des
profils d'arrêt contrôlé pourront être ajoutés en M4 si la mécanique l'exige.

### 8.4 `robot-core`

**États** : `Booting`, `Idle`, `Teleoperation`, `Navigating`, `Paused`, `Error`,
`SafeStop`, `Charging`.

**Transitions autorisées** :

| Depuis | Vers |
|---|---|
| `Booting` | `Idle` |
| `Idle` | `Teleoperation`, `Navigating`, `Charging` |
| `Teleoperation` | `Idle`, `Paused` |
| `Navigating` | `Idle`, `Paused` |
| `Paused` | `Teleoperation`, `Navigating`, `Idle` |
| `Charging` | `Idle` |
| `Error` | `Idle` |
| `SafeStop` | `Idle` |
| *tout état* | `SafeStop`, `Error` |

Toute autre transition renvoie `TransitionError::Forbidden`.

Le mouvement n'est autorisé que dans `Teleoperation` et `Navigating`.

**Autorité des commandes** — encode « l'IA ne pilote jamais les moteurs » :

| État | Sources acceptées |
|---|---|
| `Teleoperation` | `Teleoperation`, `Internal` |
| `Navigating` | `Navigation`, `Internal` |
| tous les autres | aucune |

`CommandSource::Ai` n'est acceptée dans **aucun** état. L'IA agit par objectifs de haut
niveau, jamais par vélocités brutes.

**Validation des commandes**, avant même l'examen de l'état :

- vélocité non finie → `NonFiniteVelocity` ;
- horodatage postérieur à `now` → `FutureTimestamp` ;
- commande plus vieille que `command_timeout` → `StaleCommand`.

**Événements** : `StateChanged`, `CommandAccepted`, `CommandRejected`,
`SafetyViolation`, `SafeStopEngaged`, `SafeStopCleared`, `EmergencyStopEngaged`,
`EmergencyStopCleared`. Consommés via `drain_events()`.

**Boucle `tick(now)`** :

1. Premier tick : enregistrer l'instant de démarrage.
2. `Booting` et durée de boot écoulée → transition vers `Idle`.
3. Déterminer l'autorité : `Allowed(v)` si l'état autorise le mouvement et qu'une
   commande est en attente, sinon `Denied`.
4. `safety.evaluate(authority, now)`.
5. Émettre un événement par violation.
6. Si `SAFE_STOP` requis → transition vers `SafeStop`.
7. Retourner `ControlOutput { state, velocity, at }`.

**Sortie de `SafeStop`** : `clear_safe_stop(now)` échoue tant que l'arrêt d'urgence est
engagé. Le retour à `Idle` est toujours explicite, jamais automatique.

### 8.5 `robot-telemetry`

`TelemetryCollector` consomme les `RobotEvent`, met à jour des compteurs
(`commands_accepted`, `commands_rejected`, `safety_violations`, `safe_stops`,
`state_transitions`) et émet des logs `tracing` structurés. `TelemetrySnapshot` fournit
l'instantané destiné à l'API en M7.

### 8.6 Livrables annexes

`Cargo.toml` workspace, `rust-toolchain.toml`, `.github/workflows/rust.yml`
(fmt + clippy `-D warnings` + test + build), `README.md`, `LICENSE` (Apache-2.0),
`CONTRIBUTING.md`, `SECURITY.md`, `.gitignore`, ADR dans `docs/architecture/`.

### 8.7 Critères d'acceptation

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

Les trois commandes doivent passer sous Windows, sans ROS 2, sans Docker, sans matériel.

Propriétés couvertes par les tests, au minimum :

- toute transition interdite est rejetée sans changer d'état ;
- une commande dépassant les limites est saturée, pas rejetée ;
- l'absence de commande pendant `command_timeout` en état de mouvement déclenche
  `SAFE_STOP` ;
- l'arrêt d'urgence produit une vélocité nulle immédiate depuis n'importe quel état ;
- `SafeStop` ne se quitte pas tant que l'arrêt d'urgence est engagé ;
- une commande `CommandSource::Ai` est rejetée dans tous les états ;
- une vélocité NaN ou infinie ne se propage jamais jusqu'à la sortie.

---

## 9. Risques identifiés

| Risque | Impact | Mitigation |
|---|---|---|
| API `rclrs` instable | Casse à chaque montée de version | Confiné à `robot-ros2` ; bascule `r2r` possible |
| Windows sans ROS 2 natif | Friction de développement | Cœur testable en natif ; ROS 2 en WSL2 |
| Prix composants volatils | BOM obsolète | Achat différé après M3 ; variante économique chiffrée |
| Pi 4 saturé en Phase 5 | Vision impraticable | LiDAR 2D suffit jusqu'à M6 ; mini-PC N100 en upgrade |
| Dérive d'odométrie | Localisation dégradée | Encodeurs en PIO + fusion IMU BNO085 dès M4 |

---

## 10. Méthode

Une milestone à la fois. Pour chacune : inspecter l'existant, proposer le design,
identifier les risques, implémenter, tester, valider, documenter, **attendre la
validation avant la suivante**.

Pas de dépendance sans justification. Pas de microservice. Pas de complexité anticipant
un problème qui n'existe pas encore.
