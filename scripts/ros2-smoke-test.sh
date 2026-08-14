#!/usr/bin/env bash
# Essai de bout en bout de l'adaptateur ROS 2.
#
# Verifie le parcours complet de la Milestone 2 : demarrage, teleoperation, saturation
# par la couche de securite, arret automatique sur watchdog, et reprise explicite.
#
# A lancer dans l'image docker/ros2, depuis la racine du depot montee sur /workspace :
#   docker run --rm -v "$PWD":/workspace roboto-ros2:jazzy bash scripts/ros2-smoke-test.sh
set -eo pipefail

set +u
source /workspace/ros2/install/setup.bash
set -u

readonly NODE_NS=/robot_core
readonly TIMEOUT=10

node_pid=""
publisher_pid=""

cleanup() {
    [[ -n "${publisher_pid}" ]] && kill "${publisher_pid}" 2>/dev/null || true
    [[ -n "${node_pid}" ]] && kill "${node_pid}" 2>/dev/null || true
    wait 2>/dev/null || true
}
trap cleanup EXIT

fail() {
    echo "ECHEC : $*" >&2
    exit 1
}

current_state() {
    timeout "${TIMEOUT}" ros2 topic echo "${NODE_NS}/state" std_msgs/msg/String --once 2>/dev/null \
        | sed -n 's/^data: *//p' \
        | tr -d "'\"" \
        | tr -d '[:space:]' \
        | head -1
}

safe_linear_velocity() {
    timeout "${TIMEOUT}" ros2 topic echo "${NODE_NS}/cmd_vel_safe" geometry_msgs/msg/Twist --once 2>/dev/null \
        | awk '/^linear:/ { inside = 1; next } inside && /x:/ { print $2; exit }'
}

expect_state() {
    local expected="$1" description="$2" actual
    actual="$(current_state)"
    [[ "${actual}" == "${expected}" ]] || fail "${description} : etat ${actual:-<aucun>}, attendu ${expected}"
    echo "  OK  ${description} : ${expected}"
}

echo "== Demarrage du noeud =="
ros2 run robot_ros2 robot_node &
node_pid=$!
sleep 3

echo "== 1. Le demarrage s'acheve en IDLE =="
expect_state IDLE "apres la sequence de boot"

echo "== 2. IDLE refuse le mouvement =="
ros2 topic pub --once /cmd_vel geometry_msgs/msg/Twist '{linear: {x: 0.3}}' >/dev/null
sleep 0.5
velocity="$(safe_linear_velocity)"
awk -v v="${velocity}" 'BEGIN { exit (v < 0.000001 && v > -0.000001) ? 0 : 1 }' \
    || fail "IDLE a laisse passer une vitesse de ${velocity}"
echo "  OK  consigne ignoree en IDLE : ${velocity} m/s"

echo "== 3. Passage en teleoperation =="
ros2 topic pub --once "${NODE_NS}/request_state" std_msgs/msg/String "{data: 'TELEOPERATION'}" >/dev/null
sleep 0.5
expect_state TELEOPERATION "apres la demande de transition"

echo "== 4. Un flux de commandes fait avancer le robot =="
ros2 topic pub -r 20 /cmd_vel geometry_msgs/msg/Twist '{linear: {x: 5.0}}' >/dev/null &
publisher_pid=$!
sleep 3
velocity="$(safe_linear_velocity)"
awk -v v="${velocity}" 'BEGIN { exit (v > 0.1) ? 0 : 1 }' \
    || fail "le robot n'avance pas : ${velocity} m/s"
# La consigne demandee vaut 5,0 m/s : la couche de securite doit la borner a 0,5.
awk -v v="${velocity}" 'BEGIN { exit (v <= 0.5000001) ? 0 : 1 }' \
    || fail "vitesse ${velocity} m/s au-dela de la limite de 0,5 m/s"
echo "  OK  consigne de 5,0 m/s bornee a ${velocity} m/s"
expect_state TELEOPERATION "pendant le mouvement"

echo "== 5. Le tarissement des commandes declenche un arret de securite =="
kill "${publisher_pid}" 2>/dev/null || true
publisher_pid=""
sleep 2
expect_state SAFE_STOP "apres 2 s sans commande"
velocity="$(safe_linear_velocity)"
awk -v v="${velocity}" 'BEGIN { exit (v < 0.000001 && v > -0.000001) ? 0 : 1 }' \
    || fail "vitesse non nulle en SAFE_STOP : ${velocity}"
echo "  OK  vitesse nulle en SAFE_STOP"

echo "== 6. La reprise est explicite =="
ros2 topic pub --once "${NODE_NS}/request_state" std_msgs/msg/String "{data: 'TELEOPERATION'}" >/dev/null
sleep 0.5
expect_state SAFE_STOP "SAFE_STOP ne se quitte pas par une simple demande d'etat"

ros2 service call "${NODE_NS}/clear_safe_stop" std_srvs/srv/Trigger >/dev/null
sleep 0.5
expect_state IDLE "apres clear_safe_stop"

echo "== 7. L'arret d'urgence prime, meme a l'arret =="
ros2 service call "${NODE_NS}/emergency_stop" std_srvs/srv/Trigger >/dev/null
sleep 0.5
expect_state SAFE_STOP "arret d'urgence engage depuis IDLE"

ros2 service call "${NODE_NS}/clear_safe_stop" std_srvs/srv/Trigger >/dev/null
sleep 0.5
expect_state SAFE_STOP "clear_safe_stop refuse tant que l'arret d'urgence tient"

ros2 service call "${NODE_NS}/clear_emergency_stop" std_srvs/srv/Trigger >/dev/null
ros2 service call "${NODE_NS}/clear_safe_stop" std_srvs/srv/Trigger >/dev/null
sleep 0.5
expect_state IDLE "apres levee des deux verrous"

echo
echo "Tous les essais sont passes."
