"""Contrat de la frontière entre l'agent et le robot.

Aucune de ces vérifications n'a besoin du SDK Anthropic, du réseau, de ROS 2 ni d'un
robot : c'est délibéré. Ce qui borne un modèle de langage doit pouvoir être vérifié
sans lui.
"""

import math

import pytest

from roboto_agent.safety import (
    AuditLog,
    OperatingArea,
    check_goal,
)


def area() -> OperatingArea:
    return OperatingArea(min_x=-5.0, max_x=5.0, min_y=-4.0, max_y=4.0)


# --- Zone d'évolution -----------------------------------------------------------------


def test_a_goal_inside_the_area_is_accepted():
    assert check_goal(1.5, -2.0, area()) is None


def test_a_goal_outside_the_area_is_refused():
    # Un modèle de langage peut halluciner une coordonnée. La frontière la borne avant
    # qu'elle n'atteigne Nav2 : sans cela, « va au point 500, 500 » enverrait le robot
    # traverser un mur pendant une heure.
    refusal = check_goal(500.0, 0.0, area())

    assert refusal is not None
    assert "zone" in refusal.reason.lower()


@pytest.mark.parametrize(
    ("x", "y"),
    [(-5.0, 0.0), (5.0, 0.0), (0.0, -4.0), (0.0, 4.0)],
)
def test_the_boundary_itself_is_inside(x, y):
    assert check_goal(x, y, area()) is None


@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_a_non_finite_coordinate_is_refused(value):
    assert check_goal(value, 0.0, area()) is not None
    assert check_goal(0.0, value, area()) is not None


@pytest.mark.parametrize("value", ["gauche", None, [1.0], {"x": 1.0}])
def test_a_coordinate_that_is_not_a_number_is_refused(value):
    assert check_goal(value, 0.0, area()) is not None


def test_a_boolean_is_not_a_coordinate():
    # En Python `bool` hérite de `int`, donc une vérification naïve laisserait passer
    # `True` comme abscisse valant 1 mètre.
    assert check_goal(True, 0.0, area()) is not None


def test_an_area_with_inverted_bounds_is_rejected_at_construction():
    # Une zone vide accepterait tout ou rien selon l'ordre des comparaisons : mieux vaut
    # échouer au démarrage qu'à la première commande.
    with pytest.raises(ValueError):
        OperatingArea(min_x=5.0, max_x=-5.0, min_y=-4.0, max_y=4.0)


def test_an_area_with_a_non_finite_bound_is_rejected():
    with pytest.raises(ValueError):
        OperatingArea(min_x=-5.0, max_x=math.inf, min_y=-4.0, max_y=4.0)


# --- Journal d'audit ------------------------------------------------------------------


def test_every_invocation_is_recorded():
    log = AuditLog()

    log.record("navigate_to", {"x": 1.0, "y": 2.0}, "accepté")
    log.record("stop", {}, "accepté")

    assert [entry.name for entry in log.entries] == ["navigate_to", "stop"]


def test_a_refusal_is_recorded_as_faithfully_as_an_acceptance():
    # Un journal qui ne garderait que les succès rendrait invisible exactement ce qu'on
    # veut pouvoir relire : ce que l'agent a tenté et qui lui a été refusé.
    log = AuditLog()

    log.record("navigate_to", {"x": 500.0, "y": 0.0}, "refusé : hors zone")

    assert len(log.entries) == 1
    assert "refusé" in log.entries[0].outcome


def test_entries_cannot_be_altered_through_the_returned_view():
    log = AuditLog()
    log.record("stop", {}, "accepté")

    entries = log.entries
    with pytest.raises((AttributeError, TypeError)):
        entries.append("frelaté")  # type: ignore[attr-defined]

    assert len(log.entries) == 1
