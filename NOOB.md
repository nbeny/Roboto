# NOOB.md — tout lancer et tout tester, pas à pas

Ce document part du principe que tu n'as **rien** installé et que tu ne connais **ni ROS 2,
ni Rust, ni Docker**. Chaque commande est donnée en entier, avec ce que tu dois voir
s'afficher.

Suis les niveaux dans l'ordre. Chacun fonctionne sans le suivant.

| Niveau | Ce que tu obtiens | Durée | Docker ? |
|---|---|---|---|
| [1](#niveau-1--tester-sans-rien-de-lourd) | tous les essais unitaires | 5 min | non |
| [2](#niveau-2--construire-les-images-docker) | les images ROS 2 et Gazebo | 30–45 min, **une fois** | oui |
| [3](#niveau-3--construire-lespace-ros-2) | l'adaptateur ROS 2 compilé | 5–10 min, **une fois** | oui |
| [4](#niveau-4--les-essais-automatiques) | la preuve que tout marche | 40 min | oui |
| [5](#niveau-5--tout-lancer-et-jouer-avec) | le robot en simulation, le tableau de bord, la voix | — | oui |

> **Il n'y a pas de robot physique à ce stade.** Tout se passe en simulation. Le matériel
> n'a pas encore été commandé.

---

## Avant tout : les deux règles de sécurité

Elles ne servent à rien aujourd'hui, en simulation. Elles serviront le jour où un moteur
sera branché, et il vaut mieux les avoir lues avant.

1. **L'arrêt d'urgence qui compte est un bouton physique** qui coupe le courant. Aucun
   bouton à l'écran, aucune phrase prononcée, aucune IA ne le remplace.
2. **La reprise après un arrêt est toujours explicite**, en deux gestes : lever l'arrêt
   d'urgence, puis quitter l'arrêt de sécurité. C'est volontairement pénible.

---

## Ce qu'il faut installer

Sous **Windows**, ouvre PowerShell (touche Windows, tape `powershell`).

| Outil | Pour quoi | Où |
|---|---|---|
| Git | récupérer le projet | https://git-scm.com/downloads |
| Rust | le cœur du robot | https://rustup.rs |
| Node.js 22+ | l'API et le tableau de bord | https://nodejs.org |
| Python 3.11+ | l'agent, la vision, la voix | https://python.org |
| Docker Desktop | ROS 2 et le simulateur | https://docker.com/products/docker-desktop |

Après installation, **ferme et rouvre PowerShell**, puis vérifie :

```powershell
git --version
cargo --version
node --version
python --version
docker --version
```

Chaque ligne doit afficher un numéro de version. Si l'une dit *« n'est pas reconnu »*,
l'outil n'est pas installé ou le terminal n'a pas été rouvert.

> **Docker Desktop doit être démarré** (icône baleine dans la barre des tâches) avant tout
> ce qui touche à Docker. S'il ne l'est pas, tu verras
> `failed to connect to the docker API`.

---

## Récupérer le projet

```powershell
cd $HOME\Documents
git clone https://github.com/<ton-compte>/Roboto.git
cd Roboto
```

Toutes les commandes qui suivent se lancent **depuis ce dossier**, sauf mention contraire.

---

## Niveau 1 — tester sans rien de lourd

Aucun Docker, aucun ROS 2. C'est délibéré : la partie critique du robot a été écrite pour
être vérifiable sans le middleware, sans le simulateur et sans le matériel.

### 1.1 Le cœur du robot (Rust)

```powershell
cargo test --workspace --all-features
```

La première fois, la compilation prend 2 à 5 minutes. Tu dois voir, en dernier :

```
test result: ok. 38 passed; 0 failed
...
```

**252 essais** au total, répartis sur plusieurs lignes. Aucun `failed`.

Le style et la qualité :

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

Les deux ne doivent **rien afficher**. Pas de nouvelle : bonne nouvelle.

### 1.2 L'API (TypeScript)

```powershell
cd api
npm install
npm test
```

Attendu :

```
 Test Files  5 passed (5)
      Tests  62 passed (62)
```

Ces essais font tourner un **faux serveur ROS en mémoire** : c'est pourquoi ils
fonctionnent sous Windows sans avoir installé ROS 2.

```powershell
npm run typecheck
npm run build
cd ..
```

### 1.3 Le tableau de bord (Next.js)

```powershell
cd web
npm install
npm run build
cd ..
```

Attendu : `✓ Compiled successfully`.

### 1.4 L'agent IA (Python)

```powershell
cd ai
python -m pytest -q
cd ..
```

Attendu : `49 passed`. **Sans clé API, sans réseau, sans robot** — la couche de sécurité
de l'agent n'a aucune dépendance, c'est ce qui la rend vérifiable ainsi.

### 1.5 La vision (Python)

```powershell
cd vision
pip install -e ".[dev]"
python -m pytest -q
cd ..
```

Attendu : `26 passed`. Les essais fabriquent eux-mêmes leurs images de marqueurs.

Pour t'amuser, fabrique un marqueur à imprimer :

```powershell
cd vision
python -m roboto_vision --make-marker 7 --out marqueur7.png
cd ..
```

### 1.6 La voix (Python)

```powershell
cd voice
python -m pytest -q
python -m roboto_voice --self-test
cd ..
```

Le second affiche ce qui arrête le robot et ce qui ne l'arrête pas :

```
  OK     ARRÊT  « stop »
  OK     ARRÊT  « ne t'arrête pas »
  OK     agent  « va dans la cuisine »
```

> **« ne t'arrête pas » arrête aussi, et c'est voulu.** Un arrêt inutile coûte quelques
> secondes ; un arrêt manqué coûte potentiellement quelqu'un.

### Récapitulatif du niveau 1

| Brique | Commande | Attendu |
|---|---|---|
| Cœur Rust | `cargo test --workspace --all-features` | 252 essais |
| API | `cd api && npm test` | 62 essais |
| Tableau de bord | `cd web && npm run build` | compilé |
| Agent | `cd ai && python -m pytest` | 49 essais |
| Vision | `cd vision && python -m pytest` | 26 essais |
| Voix | `cd voice && python -m pytest` | 40 essais |

Si tout est vert, **la logique du robot est saine**. La suite ne concerne que ROS 2 et le
simulateur.

---

## Niveau 2 — construire les images Docker

À faire **une seule fois**. Compte 30 à 45 minutes et une bonne connexion : la seconde
image pèse environ 8 Go.

```powershell
docker build -t roboto-ros2:jazzy docker/ros2
docker build -t roboto-sim:jazzy docker/simulation
```

Vérifie :

```powershell
docker images | Select-String roboto
```

Tu dois voir `roboto-ros2` et `roboto-sim`.

> Si la construction échoue sur un téléchargement, relance la même commande : Docker
> reprend là où il s'était arrêté.

---

## Niveau 3 — construire l'espace ROS 2

Aussi **une seule fois** (à refaire si tu modifies le code de `ros2/`).

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace/ros2 roboto-ros2:jazzy colcon build
```

> Sous Linux ou WSL2, remplace `"${PWD}"` par `"$PWD"` dans toutes les commandes Docker.

Cela construit deux paquets : `robot_ros2` (l'adaptateur, en Rust) et `robot_vision` (la
perception, en Python). Attendu, à la fin :

```
Summary: 2 packages finished
```

> **Piège à connaître.** Ne lance **jamais** `colcon build` depuis la racine du dépôt : il
> y déposerait un fichier `.cargo/config.toml` qui casse la compilation Rust normale. Si
> `cargo test` se met soudain à échouer avec une erreur qui parle de chemins introuvables,
> supprime les dossiers `.cargo/` et `log/` à la racine.

---

## Niveau 4 — les essais automatiques

Chaque essai démarre la pile, vérifie quelque chose de précis, puis nettoie. Tous suivent
le même modèle :

```powershell
docker run --rm -v "${PWD}:/workspace" -w /workspace roboto-sim:jazzy bash scripts/<nom>.sh
```

Lance-les **dans cet ordre** — du plus rapide au plus long :

| # | Script | Durée | Ce que ça prouve |
|---|---|---|---|
| 1 | `ros2-smoke-test.sh` | ~2 min | le cœur Rust parle bien ROS 2 |
| 2 | `vision-smoke-test.sh` | ~3 min | la caméra est comprise, gauche et droite ne sont pas inversées |
| 3 | `sim-smoke-test.sh` | ~5 min | Gazebo tourne, le LiDAR voit les murs |
| 4 | `nav-smoke-test.sh` | ~10 min | le robot cartographie et navigue tout seul |
| 5 | `api-smoke-test.sh` | ~10 min | le bouton d'arrêt du tableau de bord arrête vraiment le robot |
| 6 | `agent-smoke-test.sh` | ~12 min | l'IA conduit le robot, et la voix l'arrête sans modèle |

Chacun se termine par une ligne claire. Par exemple, pour le dernier :

```
  OK  « arrête-toi » a arrêté le robot, sans aucun modèle de langage

Tous les essais de l'agent sont passés.
```

En cas d'échec, le script affiche les 20 à 30 dernières lignes des journaux concernés.

> Ces essais démarrent un simulateur complet. Sur une machine chargée, ils peuvent être
> plus lents que les durées indiquées. Laisse-les finir.

---

## Niveau 5 — tout lancer et jouer avec

Ici, tu pilotes toi-même. Il te faut **trois terminaux PowerShell** ouverts sur le dossier
du projet.

### Terminal 1 — le robot et le simulateur

```powershell
docker run --rm -it --name roboto -p 9090:9090 -v "${PWD}:/workspace" -w /workspace `
  roboto-sim:jazzy ros2 launch simulation/launch/autonomy.launch.py
```

Beaucoup de texte défile. Attends d'y voir apparaître `Creating bond timer` et des lignes
de `bt_navigator` : la pile est prête au bout d'une à deux minutes.

Ce lancement démarre : Gazebo, le cœur Rust, la cartographie, la navigation, la
passerelle web et la vision.

Pour désactiver une partie :

```powershell
# sans la vision
... ros2 launch simulation/launch/autonomy.launch.py vision:=false
# sans la passerelle web
... ros2 launch simulation/launch/autonomy.launch.py rosbridge:=false
```

### Terminal 2 — l'API

```powershell
cd api
npm run build
$env:ROSBRIDGE_URL="ws://127.0.0.1:9090"
npm start
```

Attendu :

```
[api] REST et WebSocket sur http://0.0.0.0:8080
[api] rosbridge connecté sur ws://127.0.0.1:9090
```

Teste-la dans un navigateur ou avec `curl` :

```powershell
curl http://127.0.0.1:8080/api/robot/status
```

### Terminal 3 — le tableau de bord

```powershell
cd web
npm run dev
```

Ouvre **http://localhost:3000**.

Tu dois voir :

- la **carte** construite par le robot, qui se remplit à mesure qu'il explore ;
- une **flèche** qui est le robot, orientée dans son sens de marche ;
- l'**état** en gros, dont la couleur change : bleu au repos, vert en navigation, rouge à
  l'arrêt de sécurité ;
- un gros bouton **ARRÊT**.

**Clique sur la carte** : le robot y va.
**Clique sur ARRÊT** : il s'arrête net, et l'état passe au rouge.

Pour repartir, il faut **deux clics** : « lever l'arrêt d'urgence », puis « lever l'arrêt
de sécurité ». C'est volontaire — un arrêt d'urgence ne se défait pas d'un clic.

> **La batterie affiche « non instrumentée », et ce n'est pas un bug.** Le robot n'a aucun
> capteur de tension. Afficher un pourcentage inventé sur un écran de supervision est
> exactement le genre de mensonge qui vide une batterie en pleine mission.

### Terminal 4 (facultatif) — la voix

```powershell
cd voice
$env:ROBOTO_API="http://127.0.0.1:8080"
python -m roboto_voice --text
```

Tape des phrases. `stop` arrête le robot immédiatement. Les autres phrases vont à l'agent
IA — s'il est configuré.

### Terminal 5 (facultatif) — l'agent IA

Il faut une clé Anthropic :

```powershell
cd ai
$env:ANTHROPIC_API_KEY="sk-ant-..."
$env:ROBOTO_API="http://127.0.0.1:8080"
pip install -e ".[dev]"
python -m roboto_agent
```

Puis parle-lui normalement :

```
> où es-tu ?
> qu'est-ce que tu vois ?
> va au point 1.5, 0
> arrête-toi
```

**Sans clé API**, tu peux quand même vérifier toute la plomberie :

```powershell
python -m roboto_agent --self-test
```

### Tout arrêter

Ctrl-C dans chaque terminal, puis :

```powershell
docker rm -f roboto
```

---

## Dépannage

### « failed to connect to the docker API »
Docker Desktop n'est pas démarré. Lance-le et attends que la baleine cesse de s'animer.

### « no such file or directory: /workspace/ros2/install/setup.bash »
Tu as sauté le [niveau 3](#niveau-3--construire-lespace-ros-2).

### `cargo test` échoue soudainement avec des chemins introuvables
Tu as lancé `colcon build` depuis la racine. Supprime `.cargo/` et `log/` à la racine du
dépôt, puis relance.

### Le tableau de bord affiche « liaison rompue »
L'API ne tourne pas, ou pas sur le port 8080. Vérifie le terminal 2.

### La carte reste vide
La cartographie met 30 à 60 secondes avant sa première publication. Si rien n'arrive
après deux minutes, vérifie dans le terminal 1 que `slam_toolbox` est passé à l'état
`active`.

### Le robot ne bouge pas quand je clique sur la carte
Regarde son état. S'il est en `SAFE_STOP`, il faut lever les deux verrous. S'il est en
`IDLE`, clique sur « autoriser la navigation » : le cœur n'accepte les consignes de
navigation qu'en état `NAVIGATING`, et c'est voulu.

### « ModuleNotFoundError: No module named 'roboto_vision' »
Le paquet de vision n'est pas installé. Soit `pip install -e vision/`, soit laisse le
lancement s'en charger — il transmet le chemin automatiquement.

### Tout est lent
La simulation calcule le LiDAR et la caméra **sans carte graphique**, en rendu logiciel.
C'est normal. Ferme les autres applications lourdes.

---

## Glossaire

| Terme | Ce que c'est |
|---|---|
| **ROS 2** | la « plomberie » standard de la robotique : des programmes qui s'échangent des messages |
| **topic** | un canal de messages, par exemple `/scan` pour le télémètre laser |
| **nœud** | un programme branché sur ROS 2 |
| **Gazebo** | le simulateur : il fait tourner un robot virtuel avec de la physique |
| **SLAM** | cartographier et se localiser en même temps |
| **Nav2** | la pile de navigation : elle calcule les trajets et les suit |
| **colcon** | l'outil qui compile les paquets ROS 2 |
| **LiDAR** | le télémètre laser qui tourne et mesure les distances |
| **ArUco** | un motif noir et blanc que la caméra reconnaît de façon certaine |
| **rosbridge** | une passerelle qui expose ROS 2 en JSON sur WebSocket, pour l'API |
| **SAFE_STOP** | l'état d'arrêt de sécurité : le robot ne bouge plus tant qu'on n'a pas explicitement repris |

---

## Et quand j'aurai le matériel ?

Rien de ce document ne change, sauf le terminal 1 : au lieu de lancer Gazebo, tu lanceras
le vrai robot. Le reste — API, tableau de bord, agent, voix — parle à la même interface.

Ce qui manque encore côté matériel :

- le **binaire du microcontrôleur** (décodage des encodeurs, USB, pilotage des moteurs) ;
  la logique est écrite et testée, le programme embarqué attend une carte ;
- la **mise en service** : câblage, calibration des roues, essais à vitesse réduite.

Commence par lire [SECURITY.md](SECURITY.md) avant de brancher quoi que ce soit.

---

## Pour aller plus loin

| Document | Contenu |
|---|---|
| [README.md](README.md) | vue d'ensemble et choix techniques |
| [docs/architecture/](docs/architecture/) | les décisions, et pourquoi elles ont été prises |
| [SECURITY.md](SECURITY.md) | ce qui protège les gens autour du robot |
| [api/README.md](api/README.md) | les routes de l'API |
| [vision/README.md](vision/README.md) | la perception visuelle |
| [voice/README.md](voice/README.md) | la commande vocale |
| [ai/README.md](ai/README.md) | l'agent et ses garde-fous |
