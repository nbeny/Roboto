"""La boucle de l'agent, exercée sans réseau ni clé API.

Un faux client Claude rend des réponses préparées. Ce qui est vérifié ici n'est pas
l'intelligence du modèle — elle ne se teste pas — mais ce que la boucle fait de ses
sorties : ce qu'elle exécute, ce qu'elle refuse, et ce qu'elle rapporte quand ça se
passe mal.
"""

from types import SimpleNamespace

import pytest

from roboto_agent.agent import MAX_TOOL_ROUNDS, RobotAgent
from roboto_agent.safety import AuditLog, OperatingArea
from roboto_agent.tools import ToolExecutor

from test_executor import FakeApi


def text(value: str):
    return SimpleNamespace(type="text", text=value)


def tool_use(name: str, arguments: dict, identifier: str = "toolu_1"):
    return SimpleNamespace(type="tool_use", id=identifier, name=name, input=arguments)


def reply(*blocks, stop_reason="end_turn", stop_details=None):
    return SimpleNamespace(
        content=list(blocks), stop_reason=stop_reason, stop_details=stop_details
    )


class FakeClaude:
    """Rend les réponses préparées, dans l'ordre, et retient les requêtes reçues."""

    def __init__(self, responses):
        self._responses = list(responses)
        self.requests: list[dict] = []
        self.messages = SimpleNamespace(create=self._create)

    def _create(self, **kwargs):
        self.requests.append(kwargs)
        if not self._responses:
            raise AssertionError("la boucle a demandé plus de réponses que prévu")
        return self._responses.pop(0)


@pytest.fixture
def parts():
    api = FakeApi()
    audit = AuditLog()
    executor = ToolExecutor(
        api, OperatingArea(min_x=-5.0, max_x=5.0, min_y=-4.0, max_y=4.0), audit
    )
    return api, audit, executor


# --- Boucle ---------------------------------------------------------------------------


def test_a_plain_answer_is_returned_without_touching_the_robot(parts):
    api, _, executor = parts
    agent = RobotAgent(FakeClaude([reply(text("Bonjour."))]), executor)

    assert agent.send("bonjour") == "Bonjour."
    assert api.calls == []


def test_a_tool_call_is_executed_and_the_result_fed_back(parts):
    api, _, executor = parts
    claude = FakeClaude(
        [
            reply(tool_use("get_pose", {}), stop_reason="tool_use"),
            reply(text("Le robot est à l'origine.")),
        ]
    )
    agent = RobotAgent(claude, executor)

    answer = agent.send("où es-tu ?")

    assert api.names() == ["pose"]
    assert answer == "Le robot est à l'origine."

    # Le résultat doit repartir vers le modèle, sinon il répond dans le vide.
    last = claude.requests[-1]["messages"][-1]
    assert last["role"] == "user"
    assert last["content"][0]["type"] == "tool_result"
    assert last["content"][0]["tool_use_id"] == "toolu_1"


def test_several_tool_calls_in_one_turn_are_all_answered(parts):
    # Le protocole impose un `tool_result` par `tool_use`, tous dans un seul message.
    api, _, executor = parts
    claude = FakeClaude(
        [
            reply(
                tool_use("get_pose", {}, "a"),
                tool_use("inspect", {}, "b"),
                stop_reason="tool_use",
            ),
            reply(text("fini")),
        ]
    )
    agent = RobotAgent(claude, executor)

    agent.send("fais le point")

    results = claude.requests[-1]["messages"][-1]["content"]
    assert [block["tool_use_id"] for block in results] == ["a", "b"]
    assert api.names() == ["pose", "sensors"]


def test_a_refused_goal_is_reported_to_the_model_not_hidden(parts):
    api, _, executor = parts
    claude = FakeClaude(
        [
            reply(tool_use("navigate_to", {"x": 900.0, "y": 0.0}), stop_reason="tool_use"),
            reply(text("Ce point est hors de la zone.")),
        ]
    )
    agent = RobotAgent(claude, executor)

    agent.send("va au point 900, 0")

    assert api.calls == []
    result = claude.requests[-1]["messages"][-1]["content"][0]
    assert "refus" in result["content"].lower()


# --- Arrêts de la boucle --------------------------------------------------------------


def test_a_refusal_is_surfaced_plainly(parts):
    _, _, executor = parts
    claude = FakeClaude(
        [
            reply(
                stop_reason="refusal",
                stop_details=SimpleNamespace(category="cyber", explanation=None),
            )
        ]
    )
    agent = RobotAgent(claude, executor)

    answer = agent.send("...")

    assert "refus" in answer.lower()


def test_the_loop_stops_rather_than_spinning_forever(parts):
    # Un modèle peut boucler sur un outil. Un agent qui pilote un robot ne doit pas
    # pouvoir tourner indéfiniment sans que personne ne le sache.
    _, _, executor = parts
    claude = FakeClaude(
        [reply(tool_use("get_pose", {}), stop_reason="tool_use")] * (MAX_TOOL_ROUNDS + 1)
    )
    agent = RobotAgent(claude, executor)

    answer = agent.send("tourne en rond")

    assert "interrompue" in answer.lower()
    assert len(claude.requests) <= MAX_TOOL_ROUNDS


def test_a_paused_turn_is_resumed(parts):
    _, _, executor = parts
    claude = FakeClaude(
        [
            reply(text("je réfléchis"), stop_reason="pause_turn"),
            reply(text("voilà.")),
        ]
    )
    agent = RobotAgent(claude, executor)

    assert agent.send("...") == "voilà."


# --- Ce que la boucle envoie ----------------------------------------------------------


def test_the_catalogue_and_the_system_prompt_are_sent(parts):
    from roboto_agent.tools import TOOL_SCHEMAS

    _, _, executor = parts
    claude = FakeClaude([reply(text("ok"))])
    agent = RobotAgent(claude, executor)

    agent.send("bonjour")

    request = claude.requests[0]
    assert request["tools"] is TOOL_SCHEMAS
    assert "sécurité" in request["system"].lower()


def test_history_is_kept_across_turns(parts):
    _, _, executor = parts
    claude = FakeClaude([reply(text("un")), reply(text("deux"))])
    agent = RobotAgent(claude, executor)

    agent.send("premier")
    agent.send("second")

    assert [m["role"] for m in claude.requests[-1]["messages"]] == [
        "user",
        "assistant",
        "user",
    ]
