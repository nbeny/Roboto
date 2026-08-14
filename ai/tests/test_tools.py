"""Ce que l'agent peut faire — et surtout ce qu'il ne peut pas.

Le catalogue d'outils est la surface d'attaque de l'agent. Ces essais le vérifient
structurellement, sans exécuter quoi que ce soit.
"""

import json

from roboto_agent.tools import TOOL_SCHEMAS, tool_names


def all_strings(value) -> set[str]:
    """Toutes les chaînes d'un schéma, clés comprises, en minuscules."""
    found: set[str] = set()
    if isinstance(value, dict):
        for key, item in value.items():
            found.add(str(key).lower())
            found |= all_strings(item)
    elif isinstance(value, list):
        for item in value:
            found |= all_strings(item)
    elif isinstance(value, str):
        found.add(value.lower())
    return found


def test_no_tool_lets_the_agent_set_a_velocity():
    """La propriété qui porte tout le milestone.

    L'agent dispose d'ordres de haut niveau — aller quelque part, s'arrêter, observer.
    Aucun ne prend une vitesse **en entrée**. La chaîne reste : agent -> API -> coeur ->
    sécurité -> moteurs, et jamais agent -> moteurs.

    L'examen porte sur le nom et le schéma d'entrée, pas sur la description : un outil
    de lecture qui *rapporte* une vitesse est légitime, c'en est même le but. Ce qui est
    interdit, c'est qu'une vitesse soit un paramètre que l'agent choisit.
    """
    forbidden = {
        "linear",
        "angular",
        "velocity",
        "vitesse",
        "speed",
        "cmd",
        "twist",
        "wheel",
        "roue",
        "motor",
        "moteur",
        "pwm",
        "duty",
        "throttle",
    }

    for schema in TOOL_SCHEMAS:
        surface = {"name": schema["name"], "input_schema": schema["input_schema"]}

        words = set()
        for token in all_strings(surface):
            words |= set(token.replace("_", " ").replace("/", " ").split())

        leaked = words & forbidden
        assert not leaked, f"l'outil {schema['name']} expose {sorted(leaked)} en entrée"


def test_the_stop_tool_takes_no_argument():
    # Un arrêt qui pourrait être mal formé serait un arrêt qui peut échouer.
    stop = next(s for s in TOOL_SCHEMAS if s["name"] == "stop")

    assert stop["input_schema"].get("properties", {}) == {}
    assert stop["input_schema"].get("required", []) == []


def test_every_tool_declares_a_description_and_a_schema():
    for schema in TOOL_SCHEMAS:
        assert schema["description"].strip(), f"{schema['name']} sans description"
        assert schema["input_schema"]["type"] == "object"


def test_every_tool_says_when_to_call_it():
    # Une description qui dit seulement ce que fait l'outil laisse le modèle deviner
    # quand l'appeler. Les recommandations Claude sont explicites là-dessus.
    for schema in TOOL_SCHEMAS:
        text = schema["description"].lower()
        assert any(
            trigger in text for trigger in ("quand", "lorsque", "appelle", "utilise")
        ), f"{schema['name']} ne dit pas quand l'appeler"


def test_tool_names_are_unique():
    names = tool_names()
    assert len(names) == len(set(names))


def test_the_catalogue_is_valid_json():
    # Il part tel quel dans une requête HTTP : tout ce qui ne se sérialise pas casserait
    # l'agent au premier appel, pas au démarrage.
    json.dumps(TOOL_SCHEMAS)


def test_navigation_declares_only_a_destination():
    navigate = next(s for s in TOOL_SCHEMAS if s["name"] == "navigate_to")
    properties = navigate["input_schema"]["properties"]

    assert set(properties) == {"x", "y"}
    assert set(navigate["input_schema"]["required"]) == {"x", "y"}
