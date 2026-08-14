"""Ce que l'exécuteur transmet — et ce qu'il arrête avant l'API.

Le double d'API enregistre chaque appel : c'est ainsi qu'on vérifie qu'un but hors zone
n'atteint jamais le réseau, au lieu de vérifier seulement qu'un message d'erreur en
revient.
"""

import json

import pytest

from roboto_agent.safety import AuditLog, OperatingArea
from roboto_agent.tools import ToolExecutor


class FakeApi:
    """API du robot, réduite à ce que ces essais observent."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, tuple]] = []
        self.fail_with: Exception | None = None

    def _record(self, name: str, *args):
        self.calls.append((name, args))
        if self.fail_with is not None:
            raise self.fail_with
        return {"accepted": True, "message": f"{name} ok"}

    def status(self):
        return self._record("status")

    def pose(self):
        return self._record("pose")

    def sensors(self):
        return self._record("sensors")

    def navigate(self, x, y):
        return self._record("navigate", x, y)

    def request_state(self, state):
        return self._record("request_state", state)

    def stop(self):
        return self._record("stop")

    def names(self) -> list[str]:
        return [name for name, _ in self.calls]


@pytest.fixture
def setup():
    api = FakeApi()
    audit = AuditLog()
    area = OperatingArea(min_x=-5.0, max_x=5.0, min_y=-4.0, max_y=4.0)
    return api, audit, ToolExecutor(api, area, audit)


# --- La barrière ----------------------------------------------------------------------


def test_a_goal_outside_the_area_never_reaches_the_api(setup):
    api, audit, executor = setup

    answer = executor.navigate_to(500.0, 0.0)

    assert api.calls == [], "le but hors zone est parti sur le réseau"
    assert "refus" in answer.lower()
    assert "refusé" in audit.entries[0].outcome


def test_a_goal_inside_the_area_is_transmitted(setup):
    api, _, executor = setup

    executor.navigate_to(1.5, -2.0)

    assert api.calls == [("navigate", (1.5, -2.0))]


def test_a_goal_given_as_text_never_reaches_the_api(setup):
    # Un modèle peut rendre "1.5" au lieu de 1.5. La frontière le refuse plutôt que de
    # convertir en silence : deviner l'intention d'un modèle est précisément ce qu'il ne
    # faut pas faire à une frontière de sécurité.
    api, _, executor = setup

    answer = executor.navigate_to("1.5", "-2.0")

    assert api.calls == []
    assert "nombre" in answer.lower()


def test_the_refusal_explains_itself_to_the_model(setup):
    # Un refus qui ne dit pas pourquoi fait réessayer la même chose indéfiniment.
    _, _, executor = setup

    answer = executor.navigate_to(500.0, 0.0)

    assert "zone" in answer.lower()
    assert "5.0" in answer, "la zone autorisée doit être rappelée"


# --- Arrêt ----------------------------------------------------------------------------


def test_stop_is_transmitted(setup):
    api, _, executor = setup

    executor.stop()

    assert api.names() == ["stop"]


def test_a_failed_stop_is_stated_loudly_not_swallowed(setup):
    # C'est la pire nouvelle que l'agent puisse recevoir. Elle doit ressortir en clair
    # dans ce que lit le modèle, pas se fondre dans un message d'erreur générique.
    api, audit, executor = setup
    api.fail_with = ConnectionError("API injoignable")

    answer = executor.stop()

    assert "ARRÊT NON CONFIRMÉ" in answer
    assert "urgence" in answer.lower()
    assert "ÉCHEC" in audit.entries[0].outcome


# --- Lecture --------------------------------------------------------------------------


def test_reads_are_returned_as_json(setup):
    _, _, executor = setup

    assert json.loads(executor.get_robot_state())["accepted"] is True


def test_a_failed_read_is_reported_not_faked(setup):
    api, _, executor = setup
    api.fail_with = TimeoutError("délai dépassé")

    answer = executor.inspect()

    assert "impossible" in answer.lower()
    assert "délai" in answer


# --- Répartition ----------------------------------------------------------------------


def test_dispatch_routes_each_tool(setup):
    api, _, executor = setup

    for name in ("get_robot_state", "get_pose", "inspect", "allow_navigation", "stop"):
        executor.dispatch(name, {})
    executor.dispatch("navigate_to", {"x": 1.0, "y": 1.0})

    assert api.names() == [
        "status",
        "pose",
        "sensors",
        "request_state",
        "stop",
        "navigate",
    ]


def test_an_unknown_tool_is_refused_never_guessed(setup):
    api, audit, executor = setup

    answer = executor.dispatch("set_wheel_speed", {"left": 9.0})

    assert api.calls == []
    assert "inconnu" in answer.lower()
    assert "inconnu" in audit.entries[0].outcome


def test_navigate_to_dispatched_without_arguments_is_refused(setup):
    api, _, executor = setup

    answer = executor.dispatch("navigate_to", {})

    assert api.calls == []
    assert "nombre" in answer.lower()


def test_every_invocation_leaves_a_trace(setup):
    _, audit, executor = setup

    executor.dispatch("stop", {})
    executor.dispatch("navigate_to", {"x": 900.0, "y": 0.0})
    executor.dispatch("inconnu", {})

    assert [entry.name for entry in audit.entries] == ["stop", "navigate_to", "inconnu"]
