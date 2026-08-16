#!/usr/bin/env bash
# Essai du nœud de perception visuelle.
#
# Sans Gazebo : l'image est fabriquée par le script, donc on sait exactement ce qui doit
# être détecté. Cela exerce le vrai chemin ROS 2 — `sensor_msgs/Image`, conversion
# maison sans `cv_bridge`, détection, publication sur `/vision/scene` — sans dépendre du
# rendu logiciel d'un simulateur.
#
#   docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/vision-smoke-test.sh
#
# Pas de `pipefail` : les outils `ros2` meurent sur SIGPIPE quand leur consommateur
# s'arrête.
set -e

set +u
source /workspace/ros2/install/setup.bash
set -u

export ROBOTO_VISION_PATH=/workspace/vision
export PYTHONPATH="${ROBOTO_VISION_PATH}:${PYTHONPATH:-}"

node_pid=""
publisher_pid=""

cleanup() {
    [[ -n "${publisher_pid}" ]] && kill -9 "${publisher_pid}" 2>/dev/null || true
    [[ -n "${node_pid}" ]] && kill -9 "${node_pid}" 2>/dev/null || true
    true
}
trap cleanup EXIT

fail() {
    echo "ECHEC : $*" >&2
    echo "--- nœud de vision ---" >&2
    tail -20 /tmp/vision_node.log 2>/dev/null >&2 || true
    echo "--- éditeur d'images ---" >&2
    tail -10 /tmp/publisher.log 2>/dev/null >&2 || true
    exit 1
}

ok() { echo "  OK  $*"; }

echo "== Essais unitaires de la bibliothèque de vision =="
# La même bibliothèque tourne ici sous OpenCV 4.6 et sur la machine de développement
# sous 5.x : les deux APIs ArUco diffèrent, et c'est `compat.py` qui les réconcilie.
(cd /workspace/vision && python3 -m pytest -q) || fail "essais unitaires de vision"
ok "la bibliothèque passe sous OpenCV $(python3 -c 'import cv2; print(cv2.__version__)')"

echo "== Construction du paquet ROS 2 =="
(cd /workspace/ros2 && colcon build --packages-select robot_vision > /tmp/colcon.log 2>&1) \
    || { tail -25 /tmp/colcon.log >&2; fail "colcon build robot_vision"; }
set +u
source /workspace/ros2/install/setup.bash
set -u
ok "robot_vision construit"

echo "== Démarrage du nœud de vision =="
ros2 run robot_vision vision_node --ros-args -p analysis_rate:=5.0 \
    > /tmp/vision_node.log 2>&1 &
node_pid=$!

deadline=$((SECONDS + 40))
while (( SECONDS < deadline )); do
    ros2 topic list 2>/dev/null | grep -q '^/vision/scene$' && break
    sleep 1
done
ros2 topic list 2>/dev/null | grep -q '^/vision/scene$' || fail "/vision/scene n'existe pas"
ok "le nœud publie sur /vision/scene"

echo "== Sans image, la scène reste muette mais le nœud tient =="
sleep 2
kill -0 "${node_pid}" 2>/dev/null || fail "le nœud est mort sans image"
ok "le nœud survit à l'absence de caméra"

echo "== Un marqueur à gauche est reconnu, et du bon côté =="
python3 /workspace/scripts/publish_test_frame.py --marker 7 --side left \
    > /tmp/publisher.log 2>&1 &
publisher_pid=$!

# Le lecteur analyse le JSON et s'exprime en valeurs : `marqueur 7|0.2626|gauche`.
# Chercher un mot dans la sortie YAML de `ros2 topic echo` vérifierait la mise en forme
# d'un afficheur, pas la donnée.
scene="$(python3 /workspace/scripts/read_scene.py --expect 'marqueur 7' --timeout 40 || true)"

[[ "${scene}" == *"marqueur 7"* ]] || fail "marqueur jamais reconnu. Reçu : ${scene}"
ok "marqueur 7 reconnu"

[[ "${scene}" == *"|gauche" ]] \
    || fail "le marqueur n'est pas situé à gauche : ${scene}"
ok "situé à gauche — la convention de signe est respectée (${scene})"

kill -9 "${publisher_pid}" 2>/dev/null || true
publisher_pid=""

echo "== Un marqueur à droite est annoncé à droite =="
python3 /workspace/scripts/publish_test_frame.py --marker 12 --side right \
    > /tmp/publisher.log 2>&1 &
publisher_pid=$!

scene="$(python3 /workspace/scripts/read_scene.py --expect 'marqueur 12' --timeout 40 || true)"

[[ "${scene}" == *"marqueur 12"* ]] || fail "marqueur 12 jamais reconnu"
[[ "${scene}" == *"|droite" ]] || fail "le marqueur n'est pas situé à droite : ${scene}"
ok "marqueur 12 reconnu sur la droite (${scene})"

echo "== Le nœud ne commande rien =="
# La propriété structurelle : la vision décrit, elle ne pilote pas. Un éditeur de
# vitesse dans ce nœud serait un chemin vers les roues qui contourne le cœur.
publishers="$(ros2 topic info /cmd_vel 2>/dev/null | grep -i 'publisher count' || echo 'Publisher count: 0')"
echo "    ${publishers}"
grep -rn "cmd_vel\|Twist" /workspace/ros2/src/robot_vision/ >/dev/null 2>&1 \
    && fail "le nœud de vision mentionne une consigne de vitesse"
ok "aucune consigne de vitesse dans le paquet robot_vision"

echo
echo "Tous les essais de vision sont passés."
