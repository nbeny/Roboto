"""Contrat de la description de scène.

Rien ici n'a besoin de ROS 2, d'une caméra ni d'un robot : la vision est une fonction
d'une image vers une description, et c'est ce qui la rend vérifiable.
"""

import math

import pytest

from roboto_vision.scene import Detection, SceneSummary, bearing_from_pixel

FOV = 1.047  # 60°, celui de la caméra du modèle


def detection(label="marqueur 7", bearing=0.0, **kwargs) -> Detection:
    base = dict(label=label, confidence=1.0, x=0, y=0, width=10, height=10, bearing=bearing)
    base.update(kwargs)
    return Detection(**base)


# --- Gisement -------------------------------------------------------------------------


def test_the_centre_of_the_image_is_straight_ahead():
    assert bearing_from_pixel(320, 640, FOV) == pytest.approx(0.0, abs=1e-9)


def test_the_left_of_the_image_is_a_positive_bearing():
    # Convention ROS : les angles croissent dans le sens trigonométrique, donc la gauche
    # est positive. Se tromper de signe fait tourner le robot du mauvais côté.
    assert bearing_from_pixel(0, 640, FOV) > 0


def test_the_right_of_the_image_is_a_negative_bearing():
    assert bearing_from_pixel(639, 640, FOV) < 0


def test_the_edges_sit_at_half_the_field_of_view():
    assert bearing_from_pixel(0, 640, FOV) == pytest.approx(FOV / 2, abs=1e-3)
    assert bearing_from_pixel(640, 640, FOV) == pytest.approx(-FOV / 2, abs=1e-3)


def test_the_bearing_is_proportional_to_the_offset():
    quarter = bearing_from_pixel(160, 640, FOV)
    assert quarter == pytest.approx(FOV / 4, abs=1e-3)


@pytest.mark.parametrize("width", [0, -640])
def test_an_image_without_width_has_no_bearing(width):
    # Une division par zéro déguisée en angle produirait un gisement d'apparence
    # normale. Mieux vaut ne rien rendre.
    assert bearing_from_pixel(10, width, FOV) is None


def test_a_pixel_outside_the_image_has_no_bearing():
    assert bearing_from_pixel(-5, 640, FOV) is None
    assert bearing_from_pixel(700, 640, FOV) is None


# --- Description ----------------------------------------------------------------------


def test_an_empty_scene_says_so_plainly():
    summary = SceneSummary(detections=(), width=640, height=480, at="2026-08-16T10:00:00Z")

    assert "rien" in summary.describe().lower()


def test_a_described_scene_names_what_it_sees():
    summary = SceneSummary(
        detections=(detection(label="marqueur 7", bearing=0.5),),
        width=640,
        height=480,
        at="2026-08-16T10:00:00Z",
    )

    described = summary.describe().lower()
    assert "marqueur 7" in described
    assert "gauche" in described


def test_the_description_says_which_side():
    right = SceneSummary(
        detections=(detection(bearing=-0.5),), width=640, height=480, at="x"
    )
    ahead = SceneSummary(
        detections=(detection(bearing=0.01),), width=640, height=480, at="x"
    )

    assert "droite" in right.describe().lower()
    assert "face" in ahead.describe().lower()


def test_a_detection_without_bearing_is_still_described():
    summary = SceneSummary(
        detections=(detection(bearing=None),), width=640, height=480, at="x"
    )

    # Ne pas connaître le gisement n'empêche pas de signaler la présence.
    assert "marqueur 7" in summary.describe().lower()


def test_the_summary_serialises_to_plain_data():
    summary = SceneSummary(
        detections=(detection(),), width=640, height=480, at="2026-08-16T10:00:00Z"
    )

    payload = summary.to_dict()

    assert payload["width"] == 640
    assert payload["detections"][0]["label"] == "marqueur 7"

    import json

    json.dumps(payload)  # part tel quel sur un topic ROS puis en HTTP


def test_detections_are_ordered_by_confidence():
    # L'agent lit la description en entier mais retient le début : ce qui est le plus
    # sûr doit venir en premier.
    summary = SceneSummary(
        detections=(
            detection(label="incertain", confidence=0.3),
            detection(label="certain", confidence=0.9),
        ),
        width=640,
        height=480,
        at="x",
    )

    assert summary.by_confidence()[0].label == "certain"


def test_a_negative_confidence_is_rejected():
    with pytest.raises(ValueError):
        detection(confidence=-0.1)


def test_a_non_finite_bearing_is_rejected():
    with pytest.raises(ValueError):
        detection(bearing=math.nan)
