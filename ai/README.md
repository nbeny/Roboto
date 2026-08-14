# Agent

Agent conversationnel qui pilote Roboto par ordres de haut niveau.

```
Utilisateur ──► Agent ──HTTP──► API ──► robot-core ──► robot-safety ──► Nav2 ──► robot
```

---

## Ce que l'agent ne peut pas faire

**Émettre une vitesse.** Aucun outil n'en prend une en entrée — c'est vérifié
structurellement, sur les schémas exacts qui partent dans la requête :

```python
def test_no_tool_lets_the_agent_set_a_velocity():
    for schema in TOOL_SCHEMAS:
        # nom + input_schema examinés ; "linear", "speed", "wheel", "pwm"… interdits
```

**Atteindre ROS 2.** L'agent n'a pas de client ROS, pas de `rclpy`, aucun accès au
middleware. Son seul chemin vers le robot est l'API HTTP. Voir
[ADR 0010](../docs/architecture/0010-agent-sans-acces-ros2.md).

**Sortir de la zone d'évolution.** Un modèle peut produire « va au point 500, 500 » avec
une confiance parfaite. La barrière le refuse **avant tout appel réseau**, avec un motif
que l'agent relit et corrige.

Ces trois protections se superposent, et chacune suffirait. `CommandSource::Ai` est
refusée dans les huit états du cœur depuis la Milestone 1 : même une commande fabriquée
serait rejetée — mais l'agent ne peut pas même en former une.

---

## Les outils

| Outil | Entrée | Rôle |
|---|---|---|
| `get_robot_state` | — | état, allure, mission |
| `get_pose` | — | position et cap dans le repère `map` |
| `inspect` | — | obstacle le plus proche, portée du LiDAR |
| `navigate_to` | `x`, `y` | envoie le robot à un point |
| `allow_navigation` | — | demande la transition vers `NAVIGATING` |
| `stop` | — | arrêt immédiat |

`stop` ne prend aucun argument : un arrêt qui pourrait être mal formé serait un arrêt
qui peut échouer.

Chaque description dit **quand** appeler l'outil, pas seulement ce qu'il fait — les
descriptions prescriptives donnent un gain mesurable sur les modèles récents, qui
sollicitent les outils avec parcimonie.

---

## Vérification sans modèle

Le jugement d'un modèle ne se teste pas de façon déterministe. La plomberie qui
l'entoure, si :

```bash
# 47 essais, sans réseau, sans robot, sans clé API — la couche de sécurité
# n'a aucune dépendance, c'est ce qui la rend vérifiable ainsi
python -m pytest

# les outils contre l'API réelle, toujours sans modèle
python -m roboto_agent --self-test

# FAIT BOUGER LE ROBOT : conduit puis arrête, par les outils de l'agent
python -m roboto_agent --drive-test
```

De bout en bout, contre la simulation :

```bash
docker run --rm -v "$PWD":/workspace -w /workspace roboto-sim:jazzy \
  bash scripts/agent-smoke-test.sh
```

---

## Conversation

```bash
export ANTHROPIC_API_KEY=...   # ou : ant auth login
export ROBOTO_API=http://127.0.0.1:8080
export ROBOTO_AREA=-5,5,-4,4   # min_x,max_x,min_y,max_y

pip install -e ".[dev]"
roboto-agent
```

Modèle : `claude-opus-5`, réflexion adaptative, effort `high`. Tout ce que l'agent
tente — accepté comme refusé — passe dans un journal d'audit affiché en sortie.

---

## Deux choix qui méritent une explication

**La boucle est écrite à la main**, pas confiée au *tool runner* du SDK. Deux raisons
précises : l'artefact testé doit être l'artefact expédié — la propriété « aucune vitesse
en entrée » est vérifiée sur `TOOL_SCHEMAS`, et c'est `TOOL_SCHEMAS` qui part dans la
requête ; et l'exécuteur porte un état (client, zone, journal) que le décorateur du SDK
obligerait à mettre dans une variable globale. La boucle tient en trente lignes, et
c'est une frontière de sécurité : elle mérite d'être lisible d'un seul tenant.

**Un arrêt qui échoue est dit en majuscules.** C'est la pire nouvelle que l'agent puisse
recevoir ; elle ressort en clair dans ce qu'il lit, avec la consigne de prévenir
l'utilisateur et de renvoyer vers l'arrêt d'urgence matériel — jamais fondue dans un
message d'erreur générique.
