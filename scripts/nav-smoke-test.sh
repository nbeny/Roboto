#!/usr/bin/env bash
# Essai de bout en bout de la navigation autonome.
#
# Verifie ce que promet la Milestone 6 : le robot construit une carte, se localise,
# recoit un but, et l'atteint — en passant par la couche de securite du coeur Rust.
#
#   docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/nav-smoke-test.sh
#
# Pas de `pipefail` : les outils `ros2` sont interrompus par leurs consommateurs en aval
# et meurent sur SIGPIPE, ce qui transformerait une verification reussie en echec.
set -e

set +u
source /workspace/ros2/install/setup.bash
set -u

readonly LOG=/tmp/autonomy.log
readonly GOAL_X=1.5
readonly GOAL_Y=-1.0
#: Tolerance d'arrivee : celle de Nav2 est de 0,15 m, on laisse la marge de l'odometrie.
readonly ARRIVAL_TOLERANCE=0.45
#: Vitesse maximale de Nav2. Celle de robot-safety vaut 0,5 : l'ecart est la marge.
readonly NAV2_MAX_SPEED=0.35

launch_pid=""

cleanup() {
    [[ -n "${launch_pid}" ]] && kill -INT "${launch_pid}" 2>/dev/null || true
    sleep 3
    pkill -9 -f 'gz sim' 2>/dev/null || true
    pkill -9 -f robot_node 2>/dev/null || true
    pkill -9 -f 'nav2\|slam_toolbox\|parameter_bridge\|robot_state_publisher' 2>/dev/null || true
    [[ -n "${launch_pid}" ]] && kill -9 "${launch_pid}" 2>/dev/null || true
    true
}
trap cleanup EXIT

fail() {
    echo "ECHEC : $*" >&2
    echo "--- dernieres lignes du lancement ---" >&2
    tail -40 "${LOG}" >&2 || true
    exit 1
}

ok() { echo "  OK  $*"; }

state() {
    timeout 15 ros2 topic echo /robot_core/state --field data --once 2>/dev/null \
        | head -1 | tr -d "'\"[:space:]"
}

wait_for_topic() {
    local topic="$1" deadline=$((SECONDS + ${2:-60}))
    while (( SECONDS < deadline )); do
        timeout 5 ros2 topic echo "${topic}" --once >/dev/null 2>&1 && return 0
        sleep 1
    done
    fail "aucun message sur ${topic} apres ${2:-60} s"
}

# Position du robot dans le repere `map`, sous la forme "x,y,z".
pose_in_map() {
    timeout 15 ros2 run tf2_ros tf2_echo map base_footprint \
        --ros-args -p use_sim_time:=true > /tmp/pose.txt 2>&1 || true
    grep -m1 -- '- Translation:' /tmp/pose.txt | sed 's/.*\[\(.*\)\]/\1/' | tr -d ' '
}

echo "== Demarrage de la pile d'autonomie =="
ros2 launch simulation/launch/autonomy.launch.py > "${LOG}" 2>&1 &
launch_pid=$!

echo "== 1. La simulation et le coeur tournent =="
wait_for_topic /clock 120
wait_for_topic /robot_core/state 90
wait_for_topic /scan 90
ok "temps simule, coeur et LiDAR actifs"

echo "== 2. La cartographie produit une carte =="
wait_for_topic /map 120
ok "slam_toolbox publie /map"

echo "== 3. La chaine de reperes est complete jusqu'a la carte =="
start_pose="$(pose_in_map)"
[[ -n "${start_pose}" ]] || fail "chaine TF map -> base_footprint absente"
ok "map -> base_footprint disponible, depart a (${start_pose})"

echo "== 4. Le coeur autorise la navigation =="
deadline=$((SECONDS + 60))
current=""
while (( SECONDS < deadline )); do
    current="$(state)"
    [[ "${current}" == "NAVIGATING" ]] && break
    sleep 2
done
[[ "${current}" == "NAVIGATING" ]] || fail "etat ${current:-<aucun>}, attendu NAVIGATING"
ok "le coeur est en NAVIGATING, les consignes de Nav2 font autorite"

echo "== 5. Nav2 est actif =="
# L'apparition de l'action dans la liste ne suffit pas : le serveur existe des sa
# creation, avant que le noeud ne soit reellement pret. Un but envoye dans cet intervalle
# est accepte, mais la reponse n'arrive jamais au client — vu une fois en developpement,
# avec un « Failed to send goal response (timeout) » cote bt_navigator.
deadline=$((SECONDS + 120))
while (( SECONDS < deadline )); do
    [[ "$(timeout 5 ros2 lifecycle get /bt_navigator 2>/dev/null | cut -d' ' -f1)" == "active" ]] && break
    sleep 3
done
[[ "$(timeout 5 ros2 lifecycle get /bt_navigator 2>/dev/null | cut -d' ' -f1)" == "active" ]] \
    || fail "bt_navigator n'est pas passe a l'etat actif"

ros2 action list 2>/dev/null | grep -q navigate_to_pose \
    || fail "l'action navigate_to_pose n'est pas disponible"
sleep 3
ok "bt_navigator actif et action navigate_to_pose disponible"

echo "== 6. Envoi d'un but de navigation =="
# Observation de la vitesse pendant la navigation, en tache de fond.
timeout 60 ros2 topic echo /robot_core/cmd_vel_safe --field linear.x \
    > /tmp/speeds.txt 2>/dev/null &
speed_watcher=$!

timeout 180 ros2 action send_goal /navigate_to_pose nav2_msgs/action/NavigateToPose \
    "{pose: {header: {frame_id: 'map'}, pose: {position: {x: ${GOAL_X}, y: ${GOAL_Y}, z: 0.0}, orientation: {w: 1.0}}}}" \
    > /tmp/goal.txt 2>&1 || true

wait "${speed_watcher}" 2>/dev/null || true

grep -q "Goal finished with status: SUCCEEDED" /tmp/goal.txt \
    || fail "le but n'a pas ete atteint : $(tail -3 /tmp/goal.txt)"
ok "Nav2 rapporte le but atteint"

echo "== 7. Le robot est reellement arrive =="
final_pose="$(pose_in_map)"
[[ -n "${final_pose}" ]] || fail "position finale introuvable"

distance="$(awk -F, -v gx="${GOAL_X}" -v gy="${GOAL_Y}" \
    '{ dx = $1 - gx; dy = $2 - gy; print sqrt(dx*dx + dy*dy) }' <<<"${final_pose}")"

awk -v d="${distance}" -v t="${ARRIVAL_TOLERANCE}" 'BEGIN { exit (d <= t) ? 0 : 1 }' \
    || fail "arrive a (${final_pose}), soit ${distance} m du but, tolerance ${ARRIVAL_TOLERANCE} m"
ok "arrive a (${final_pose}), a ${distance} m du but"

echo "== 8. Nav2 borne la vitesse, pas la couche de securite =="
# Propriete de conception : les limites de Nav2 sont strictement a l'interieur de
# l'enveloppe de robot-safety. Voir l'en-tete de simulation/config/nav2_params.yaml.
#
# Si la vitesse observee atteignait le plafond de securite de 0,5 m/s, cela signifierait
# que la marge a disparu : Nav2 commanderait des trajectoires que le robot ne peut pas
# executer, et se mettrait a osciller.
max_speed="$(awk '{ v = ($1 < 0) ? -$1 : $1; if (v > m) m = v } END { print m + 0 }' /tmp/speeds.txt)"

[[ -n "${max_speed}" ]] || fail "aucune vitesse observee"
awk -v v="${max_speed}" 'BEGIN { exit (v > 0.05) ? 0 : 1 }' \
    || fail "le robot n'a jamais bouge : vitesse maximale ${max_speed} m/s"
awk -v v="${max_speed}" -v n="${NAV2_MAX_SPEED}" 'BEGIN { exit (v <= n + 0.02) ? 0 : 1 }' \
    || fail "vitesse maximale ${max_speed} m/s au-dela de la limite Nav2 de ${NAV2_MAX_SPEED} : la marge de securite a disparu"
ok "vitesse maximale ${max_speed} m/s, bornee par Nav2 et non par la securite"

echo "== 9. Une mission accomplie ne verrouille pas le robot =="
# Nav2 cesse de publier des qu'il atteint son but. Une regle de watchdog trop grossière
# verrouillerait alors le robot en SAFE_STOP, et il faudrait une intervention humaine
# avant chaque mission suivante. Voir SECURITY.md.
final_state="$(state)"
[[ "${final_state}" == "NAVIGATING" ]] \
    || fail "etat ${final_state:-<aucun>} apres la mission, attendu NAVIGATING : le robot s'est verrouille"
ok "le robot reste en NAVIGATING, pret pour le but suivant"

echo "== 10. Un second but s'enchaine sans intervention =="
timeout 180 ros2 action send_goal /navigate_to_pose nav2_msgs/action/NavigateToPose \
    "{pose: {header: {frame_id: 'map'}, pose: {position: {x: 0.0, y: 0.0, z: 0.0}, orientation: {w: 1.0}}}}" \
    > /tmp/goal2.txt 2>&1 || true

grep -q "Goal finished with status: SUCCEEDED" /tmp/goal2.txt \
    || fail "le second but a echoue : $(tail -3 /tmp/goal2.txt)"
ok "retour au point de depart sans avoir eu a lever quoi que ce soit"

echo
echo "Tous les essais de navigation sont passes."
