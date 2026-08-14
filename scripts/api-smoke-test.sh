#!/usr/bin/env bash
# Essai de bout en bout de l'API et du chemin de commande du tableau de bord.
#
# Verifie la seule chose qui compte vraiment ici : que le bouton d'arret du tableau de
# bord arrete reellement le robot, en traversant toute la chaine —
# HTTP, API, rosbridge, ROS 2, robot-core, robot-safety, Gazebo.
#
#   docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/api-smoke-test.sh
#
# Pas de `pipefail` : les outils `ros2` meurent sur SIGPIPE quand leur consommateur
# s'arrete, ce qui transformerait une verification reussie en echec.
set -e

set +u
source /workspace/ros2/install/setup.bash
set -u

readonly LOG=/tmp/autonomy.log
readonly API_LOG=/tmp/api.log
readonly API=http://127.0.0.1:8080

launch_pid=""
api_pid=""

cleanup() {
    [[ -n "${api_pid}" ]] && kill -9 "${api_pid}" 2>/dev/null || true
    [[ -n "${launch_pid}" ]] && kill -INT "${launch_pid}" 2>/dev/null || true
    sleep 3
    pkill -9 -f 'gz sim' 2>/dev/null || true
    pkill -9 -f robot_node 2>/dev/null || true
    pkill -9 -f 'rosbridge\|nav2\|slam_toolbox\|parameter_bridge\|robot_state_publisher' 2>/dev/null || true
    [[ -n "${launch_pid}" ]] && kill -9 "${launch_pid}" 2>/dev/null || true
    true
}
trap cleanup EXIT

fail() {
    echo "ECHEC : $*" >&2
    echo "--- API ---" >&2
    tail -20 "${API_LOG}" >&2 || true
    echo "--- lancement ---" >&2
    tail -30 "${LOG}" >&2 || true
    exit 1
}

ok() { echo "  OK  $*"; }

get() { curl -sS --max-time 10 "${API}$1"; }

post() {
    if [[ $# -ge 2 ]]; then
        curl -sS --max-time 15 -X POST -H 'content-type: application/json' -d "$2" "${API}$1"
    else
        curl -sS --max-time 15 -X POST "${API}$1"
    fi
}

status_code() { curl -sS --max-time 10 -o /dev/null -w '%{http_code}' "${API}$1"; }

json_field() { node -e "
  let raw = '';
  process.stdin.on('data', (c) => (raw += c));
  process.stdin.on('end', () => {
    try {
      const value = process.argv[1].split('.').reduce((o, k) => (o == null ? o : o[k]), JSON.parse(raw));
      console.log(value === undefined || value === null ? '' : value);
    } catch { console.log(''); }
  });
" "$1"; }

wait_for() {
    local what="$1" check="$2" deadline=$((SECONDS + ${3:-90}))
    while (( SECONDS < deadline )); do
        eval "${check}" >/dev/null 2>&1 && return 0
        sleep 2
    done
    fail "${what} n'est jamais arrive"
}

echo "== Compilation de l'API =="
# Reconstruite systematiquement plutot que supposee fraiche. Un `dist` perime se comporte
# comme un bug du systeme : ici, une version anterieure de l'API ne s'abonnait pas encore
# a `/map`, et le symptome — carte absente — pointait vers rosbridge.
# `typescript` est du JavaScript pur : les modules installes sous Windows fonctionnent tels quels.
node api/node_modules/typescript/bin/tsc -p api || fail "la compilation de l'API a echoue"
ok "api/dist reconstruit"

echo "== Demarrage de la pile d'autonomie et de la passerelle =="
ros2 launch simulation/launch/autonomy.launch.py > "${LOG}" 2>&1 &
launch_pid=$!

wait_for "rosbridge" "curl -sS --max-time 3 http://127.0.0.1:9090 -o /dev/null || nc -z 127.0.0.1 9090" 150
ok "rosbridge ecoute sur 9090"

echo "== Demarrage de l'API =="
# `ws` est du JavaScript pur : les modules installes sous Windows fonctionnent tels quels.
ROSBRIDGE_URL=ws://127.0.0.1:9090 API_HOST=127.0.0.1 API_PORT=8080 \
    node api/dist/index.js > "${API_LOG}" 2>&1 &
api_pid=$!

wait_for "l'API" "[[ \$(status_code /api/health) == 200 ]]" 60
ok "l'API repond sur /api/health"

echo "== 1. L'API voit l'etat reel du robot =="
deadline=$((SECONDS + 120))
state=""
while (( SECONDS < deadline )); do
    state="$(get /api/robot/status | json_field state)"
    [[ -n "${state}" && "${state}" != "null" ]] && break
    sleep 2
done
[[ -n "${state}" ]] || fail "l'API n'a jamais recu d'etat"
ok "etat lu par l'API : ${state}"

echo "== 2. La batterie est annoncee comme non instrumentee =="
# Le robot n'a aucune mesure de tension. Un tableau de bord qui afficherait un
# pourcentage l'inventerait.
instrumented="$(get /api/robot/status | json_field battery.instrumented)"
[[ "${instrumented}" == "false" ]] \
    || fail "la batterie se declare instrumentee (${instrumented}) alors qu'aucun capteur n'existe"
ok "batterie declaree non instrumentee"

echo "== 3. La carte est servie =="
wait_for "la carte" "[[ \$(status_code /api/robot/map) == 200 ]]" 150
width="$(get /api/robot/map | json_field width)"
[[ -n "${width}" && "${width}" -gt 0 ]] || fail "carte sans largeur exploitable"
ok "carte servie, ${width} cellules de large"

echo "== 4. La pose suit le robot =="
wait_for "la pose" "[[ -n \$(get /api/robot/pose | json_field pose.x) ]]" 60
ok "pose disponible : x = $(get /api/robot/pose | json_field pose.x)"

echo "== 5. Un but envoye par l'API met le robot en mouvement =="
accepted="$(post /api/robot/navigation '{"x": 1.2, "y": 0.0}' | json_field accepted)"
[[ "${accepted}" == "true" ]] || fail "but refuse par l'API"

moved=false
deadline=$((SECONDS + 90))
while (( SECONDS < deadline )); do
    speed="$(get /api/robot/status | json_field velocity.linear)"
    if [[ -n "${speed}" ]] && awk -v v="${speed}" 'BEGIN { exit ((v > 0.05) || (v < -0.05)) ? 0 : 1 }'; then
        moved=true
        break
    fi
    sleep 2
done
[[ "${moved}" == true ]] || fail "le robot n'a jamais bouge apres le but envoye par l'API"
ok "le robot avance, vitesse lue par l'API : ${speed} m/s"

echo "== 6. Le bouton d'arret arrete reellement le robot =="
# La verification decisive : HTTP -> API -> rosbridge -> ROS 2 -> robot-core ->
# robot-safety -> Gazebo. Si un maillon manque, le robot continue de rouler.
post /api/robot/stop > /tmp/stop.json
[[ "$(json_field accepted < /tmp/stop.json)" == "true" ]] \
    || fail "l'arret a ete refuse : $(cat /tmp/stop.json)"

stopped=false
deadline=$((SECONDS + 30))
while (( SECONDS < deadline )); do
    state="$(get /api/robot/status | json_field state)"
    speed="$(get /api/robot/status | json_field velocity.linear)"
    if [[ "${state}" == "SAFE_STOP" ]] \
        && awk -v v="${speed:-1}" 'BEGIN { exit (v < 0.02 && v > -0.02) ? 0 : 1 }'; then
        stopped=true
        break
    fi
    sleep 1
done
[[ "${stopped}" == true ]] \
    || fail "etat ${state}, vitesse ${speed} : le robot ne s'est pas arrete"
ok "robot en SAFE_STOP, vitesse ${speed} m/s"

echo "== 7. La reprise exige de lever les deux verrous, dans l'ordre =="
# Un arret d'urgence ne se defait pas d'un clic. Tant qu'il tient, le coeur refuse de
# quitter SAFE_STOP, et l'API rend 409.
refused="$(curl -sS --max-time 15 -o /dev/null -w '%{http_code}' -X POST "${API}/api/robot/resume")"
[[ "${refused}" == "409" ]] \
    || fail "la reprise a ete accordee (HTTP ${refused}) alors que l'arret d'urgence tient"
ok "reprise refusee tant que l'arret d'urgence est engage"

cleared="$(post /api/robot/clear-emergency-stop | json_field accepted)"
[[ "${cleared}" == "true" ]] || fail "impossible de lever l'arret d'urgence"

sleep 1
state="$(get /api/robot/status | json_field state)"
[[ "${state}" == "SAFE_STOP" ]] \
    || fail "etat ${state} : lever l'arret d'urgence ne doit pas remettre le robot en marche"
ok "arret d'urgence leve, le robot reste en SAFE_STOP"

resumed="$(post /api/robot/resume | json_field accepted)"
[[ "${resumed}" == "true" ]] || fail "la reprise a echoue une fois les verrous levés"
sleep 2
state="$(get /api/robot/status | json_field state)"
[[ "${state}" == "IDLE" ]] || fail "etat ${state} apres reprise, attendu IDLE"
ok "retour en IDLE apres les deux demandes explicites"

echo "== 8. L'API refuse ce qu'elle ne comprend pas =="
[[ "$(status_code /api/robot/teapot)" == "404" ]] || fail "route inconnue non refusee"
code="$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' -X POST \
    -H 'content-type: application/json' -d '{"x": "gauche"}' "${API}/api/robot/navigation")"
[[ "${code}" == "400" ]] || fail "but invalide accepte (HTTP ${code})"
ok "routes inconnues et buts invalides refuses"

echo
echo "Tous les essais de l'API sont passes."
