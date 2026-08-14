#!/usr/bin/env bash
# Essai de bout en bout de l'agent IA contre la pile réelle.
#
# Vérifie la propriété qui porte le milestone 10 : l'agent conduit le robot **sans
# jamais pouvoir émettre une consigne de vitesse**, en traversant à chaque fois
# API -> rosbridge -> ROS 2 -> robot-core -> robot-safety -> Nav2 -> Gazebo.
#
#   docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy bash scripts/agent-smoke-test.sh
#
# Aucune clé Anthropic n'est nécessaire : les outils sont exercés directement. Ce qui
# est vérifié ici est la plomberie et la barrière, pas le jugement d'un modèle — ce
# dernier ne se teste pas de façon déterministe.
#
# Pas de `pipefail` : les outils `ros2` meurent sur SIGPIPE quand leur consommateur
# s'arrête, ce qui transformerait une vérification réussie en échec.
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
    echo "--- agent ---" >&2
    tail -30 /tmp/agent.log 2>/dev/null >&2 || true
    echo "--- API ---" >&2
    tail -15 "${API_LOG}" >&2 || true
    echo "--- lancement ---" >&2
    tail -25 "${LOG}" >&2 || true
    exit 1
}

ok() { echo "  OK  $*"; }

wait_for() {
    local what="$1" check="$2" deadline=$((SECONDS + ${3:-90}))
    while (( SECONDS < deadline )); do
        eval "${check}" >/dev/null 2>&1 && return 0
        sleep 2
    done
    fail "${what} n'est jamais arrivé"
}

echo "== Essais unitaires de l'agent, sans robot =="
# La couche de sécurité se vérifie sans réseau : c'est tout l'intérêt de l'avoir écrite
# sans dépendance.
(cd /workspace/ai && python3 -m pytest -q) || fail "les essais unitaires de l'agent échouent"
ok "la barrière et le catalogue sont vérifiés hors ligne"

echo "== Compilation de l'API =="
node api/node_modules/typescript/bin/tsc -p api || fail "la compilation de l'API a échoué"
ok "api/dist reconstruit"

echo "== Démarrage de la pile d'autonomie et de la passerelle =="
ros2 launch simulation/launch/autonomy.launch.py > "${LOG}" 2>&1 &
launch_pid=$!

wait_for "rosbridge" "curl -sS --max-time 3 http://127.0.0.1:9090 -o /dev/null" 150
ok "rosbridge écoute sur 9090"

echo "== Démarrage de l'API =="
ROSBRIDGE_URL=ws://127.0.0.1:9090 API_HOST=127.0.0.1 API_PORT=8080 \
    node api/dist/index.js > "${API_LOG}" 2>&1 &
api_pid=$!

wait_for "l'API" "[[ \$(curl -sS --max-time 5 -o /dev/null -w '%{http_code}' ${API}/api/health) == 200 ]]" 60
ok "l'API répond"

# La carte doit exister avant de naviguer : sans elle Nav2 refuse tout but.
wait_for "la carte" "[[ \$(curl -sS --max-time 8 -o /dev/null -w '%{http_code}' ${API}/api/robot/map) == 200 ]]" 180
ok "la cartographie a publié"

# Nav2 doit être actif, sinon le but part dans le vide.
wait_for "bt_navigator" "ros2 lifecycle get /bt_navigator 2>/dev/null | grep -q active" 150
sleep 8
ok "Nav2 est actif"

echo "== Vérification des outils de l'agent =="
cd /workspace/ai
ROBOTO_API="${API}" ROBOTO_AREA="-5,5,-4,4" \
    python3 -m roboto_agent --self-test --drive-test 2>&1 | tee /tmp/agent.log
grep -q "traversant toute la chaîne" /tmp/agent.log \
    || fail "l'agent n'a pas conduit puis arrêté le robot"

echo
echo "Tous les essais de l'agent sont passés."
