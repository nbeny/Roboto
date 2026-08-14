#!/usr/bin/env bash
# Essai de bout en bout de la simulation.
#
# Verifie ce que la Milestone 3 promet : le robot simule obeit au coeur Rust, l'odometrie
# et les TF sont coherentes, les capteurs produisent des donnees plausibles, et la couche
# de securite garde le dernier mot jusque dans le simulateur.
#
# A lancer dans l'image docker/simulation :
#   docker run --rm -v "$PWD":/workspace roboto-sim:jazzy bash scripts/sim-smoke-test.sh
# Pas de `pipefail` ici, deliberement. Les outils `ros2` sont interrompus par les
# consommateurs en aval (`head -1`, `grep -q`), meurent sur SIGPIPE, et `pipefail`
# transformerait alors une verification reussie en echec.
set -e

set +u
source /workspace/ros2/install/setup.bash
set -u

readonly LOG=/tmp/simulation.log
launch_pid=""
publisher_pid=""

cleanup() {
    [[ -n "${publisher_pid}" ]] && kill "${publisher_pid}" 2>/dev/null || true
    [[ -n "${launch_pid}" ]] && kill -INT "${launch_pid}" 2>/dev/null || true
    sleep 3
    # `ros2 launch` ne propage pas toujours l'arret a ses enfants. On acheve donc
    # explicitement, sans jamais attendre : un `wait` ici bloquerait indefiniment si un
    # processus survivait, et c'est exactement ce qui s'est produit la premiere fois.
    pkill -9 -f 'gz sim' 2>/dev/null || true
    pkill -9 -f robot_node 2>/dev/null || true
    pkill -9 -f parameter_bridge 2>/dev/null || true
    pkill -9 -f robot_state_publisher 2>/dev/null || true
    [[ -n "${launch_pid}" ]] && kill -9 "${launch_pid}" 2>/dev/null || true
    true
}
trap cleanup EXIT

fail() {
    echo "ECHEC : $*" >&2
    echo "--- dernieres lignes du lancement ---" >&2
    tail -30 "${LOG}" >&2 || true
    exit 1
}

ok() { echo "  OK  $*"; }

field() {
    # $1 = topic, $2 = chemin du champ
    timeout 15 ros2 topic echo "$1" --field "$2" --once 2>/dev/null | head -1 | tr -d '[:space:]'
}

state() {
    timeout 15 ros2 topic echo /robot_core/state --field data --once 2>/dev/null \
        | head -1 | tr -d "'\"[:space:]"
}

# Attend qu'un topic publie au moins un message.
wait_for_topic() {
    local topic="$1" deadline=$((SECONDS + ${2:-60}))
    while (( SECONDS < deadline )); do
        if timeout 5 ros2 topic echo "${topic}" --once >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    fail "aucun message sur ${topic} apres ${2:-60} s"
}

greater() { awk -v a="$1" -v b="$2" 'BEGIN { exit (a > b) ? 0 : 1 }'; }

echo "== Demarrage de la simulation =="
ros2 launch simulation/launch/simulation.launch.py > "${LOG}" 2>&1 &
launch_pid=$!

echo "== 1. Le temps simule circule =="
wait_for_topic /clock 90
ok "/clock publie"

echo "== 2. Le coeur tourne sur le temps simule =="
# Sans /clock, le noeud reste muet : atteindre IDLE prouve que le temps simule le pilote.
wait_for_topic /robot_core/state 60
deadline=$((SECONDS + 30))
current=""
while (( SECONDS < deadline )); do
    current="$(state)"
    [[ "${current}" == "IDLE" ]] && break
    sleep 1
done
[[ "${current}" == "IDLE" ]] || fail "etat ${current:-<aucun>}, attendu IDLE"
ok "le noeud a atteint IDLE en temps simule"

echo "== 3. Les capteurs produisent des donnees =="
wait_for_topic /scan 60
wait_for_topic /imu 30
wait_for_topic /odom 30
ok "/scan, /imu et /odom publient"

scan_min="$(field /scan range_min)"
[[ -n "${scan_min}" ]] || fail "le LiDAR ne publie pas de portee minimale"
ok "LiDAR actif, portee minimale ${scan_min} m"

echo "== 4. Les transformations sont coherentes =="
wait_for_topic /tf 30
ok "/tf publie"

# `use_sim_time` est indispensable ici : les transformees sont horodatees en temps
# simule, et un tf2_echo cale sur l'horloge murale les jugerait toutes hors delai.
#
# La sortie passe par un fichier plutot que par un tube : `tf2_echo` ne s'arrete jamais
# seul, c'est `timeout` qui le tue, et un `grep -q` en aval le tuerait avant sur SIGPIPE.
timeout 20 ros2 run tf2_ros tf2_echo odom base_footprint \
    --ros-args -p use_sim_time:=true > /tmp/tf_echo.txt 2>&1 || true

grep -q "Translation" /tmp/tf_echo.txt \
    || fail "la chaine TF odom -> base_footprint est absente ($(head -3 /tmp/tf_echo.txt))"
ok "chaine TF odom -> base_footprint disponible"

echo "== 5. Le robot est immobile au repos =="
start_x="$(field /odom pose.pose.position.x)"
[[ -n "${start_x}" ]] || fail "pas d'odometrie"
ok "position initiale x = ${start_x} m"

echo "== 6. La teleoperation fait avancer le robot =="
ros2 topic pub --once /robot_core/request_state std_msgs/msg/String "{data: 'TELEOPERATION'}" >/dev/null
sleep 1
[[ "$(state)" == "TELEOPERATION" ]] || fail "le passage en teleoperation a echoue"

# Consigne volontairement excessive : la couche de securite doit la borner a 0,5 m/s.
ros2 topic pub -r 20 /cmd_vel geometry_msgs/msg/Twist '{linear: {x: 3.0}}' >/dev/null &
publisher_pid=$!
sleep 8

moving_x="$(field /odom pose.pose.position.x)"
speed="$(field /odom twist.twist.linear.x)"
greater "${moving_x}" "$(awk -v s="${start_x}" 'BEGIN { print s + 0.3 }')" \
    || fail "le robot n'a pas avance : ${start_x} -> ${moving_x}"
ok "le robot a avance de ${start_x} a ${moving_x} m"

echo "== 7. La couche de securite borne la vitesse jusque dans le simulateur =="
# Marge de 20 % : la vitesse mesuree sur l'odometrie porte le bruit de la physique.
awk -v v="${speed}" 'BEGIN { exit (v <= 0.6) ? 0 : 1 }' \
    || fail "vitesse simulee ${speed} m/s, au-dela de la limite de 0,5 m/s"
greater "${speed}" "0.3" || fail "vitesse simulee ${speed} m/s, trop faible pour une consigne saturee"
ok "consigne de 3,0 m/s bornee a ${speed} m/s dans le simulateur"

echo "== 8. Le tarissement des commandes arrete le robot =="
kill "${publisher_pid}" 2>/dev/null || true
publisher_pid=""
sleep 4

[[ "$(state)" == "SAFE_STOP" ]] || fail "etat $(state), attendu SAFE_STOP"
stopped_speed="$(field /odom twist.twist.linear.x)"
awk -v v="${stopped_speed}" 'BEGIN { exit (v < 0.05 && v > -0.05) ? 0 : 1 }' \
    || fail "le robot roule encore a ${stopped_speed} m/s en SAFE_STOP"
ok "SAFE_STOP atteint, robot immobilise a ${stopped_speed} m/s"

echo
echo "Tous les essais de simulation sont passes."
