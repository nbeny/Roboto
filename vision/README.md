# Vision

Perception visuelle : une image en entrée, une description de scène en sortie.

```
caméra ──► vision_node ──► /vision/scene ──► API ──► agent
```

---

## La vision décrit, elle ne commande pas

Le nœud n'a **aucun éditeur de commande**. Pas de `Twist`, pas de client de service de
mouvement. C'est vérifié par l'essai de bout en bout, qui échoue si le mot `cmd_vel`
apparaît quelque part dans le paquet.

Si la vision devait freiner, elle le ferait sans les garanties de `robot-safety` : sans
limite d'accélération, sans watchdog, sans arrêt verrouillé. On aurait deux autorités de
freinage aux règles différentes, et un jour elles ne diraient pas la même chose — au pire
moment. Voir [ADR 0011](../docs/architecture/0011-la-vision-decrit-elle-ne-commande-pas.md).

---

## Pourquoi des marqueurs ArUco

Trois raisons, dans cet ordre.

**Ils se vérifient hors ligne.** OpenCV sait produire les marqueurs qu'il sait lire : les
26 essais fabriquent leurs propres images. Aucune photo, aucun poids de modèle, aucune
caméra. Un détecteur généraliste aurait imposé un téléchargement de dizaines de
mégaoctets — ou, plus vraisemblablement, l'absence d'essai.

**Ils tiennent sur un Raspberry Pi 4**, sans accélérateur, à côté de Nav2 et de SLAM.

**Ils ne se trompent pas à moitié.** Le marqueur 7 est le marqueur 7 ; il n'y a pas de
« chien à 62 % ». D'où une confiance de 1.0, plutôt qu'un score inventé pour ressembler à
un réseau de neurones.

Le LiDAR sait qu'il y a quelque chose à 1,2 m ; il ne sait pas que c'est la station de
charge. C'est cette identité que la vision apporte.

Un détecteur d'objets se branche à côté par le protocole `Detector`, sans rien changer au
reste.

---

## Utilisation

```bash
pip install -e ".[dev]"
python -m pytest                              # 26 essais, sans caméra

# Fabriquer un marqueur à imprimer
python -m roboto_vision --make-marker 7 --out marqueur7.png

# Analyser une photo
python -m roboto_vision photo.jpg
python -m roboto_vision photo.jpg --json
```

Dans la simulation, le nœud démarre avec `autonomy.launch.py`. Pour s'en passer :

```bash
ros2 launch simulation/launch/autonomy.launch.py vision:=false
```

---

## Le piège qui a failli coûter cher

L'API ArUco d'OpenCV a été **refondue en 4.7**. L'image ROS 2 Jazzy — et donc le
Raspberry Pi sous Ubuntu 24.04 — livre OpenCV **4.6** ; une machine de développement
récente installe **5.x**.

Écrire pour l'une casse l'autre, et découvrir cela sur le robot aurait coûté cher.
[`compat.py`](roboto_vision/compat.py) réconcilie les deux, et les 26 essais passent sous
les deux versions — c'est vérifié dans les deux environnements, pas supposé.

---

## Convention de gisement

Positive vers la **gauche**, comme partout en ROS. Se tromper de signe fait tourner le
robot du mauvais côté, et cela se diagnostique mal une fois en mouvement — d'où quatre
essais dédiés, et une vérification de bout en bout qui affiche la valeur :

```
marqueur 7|0.26256796875|gauche
marqueur 12|-0.26093203125|droite
```
