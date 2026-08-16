"""Détection de marqueurs ArUco, exercée sur des images fabriquées ici même.

OpenCV sait produire les marqueurs qu'il sait lire : ces essais n'ont donc besoin
d'aucune photo, d'aucun poids de modèle et d'aucune caméra. Ils vérifient une vraie
détection, pas un simulacre.
"""

import cv2
import numpy as np
import pytest

from roboto_vision.compat import generate_marker
from roboto_vision.fiducials import FiducialDetector

MARKER_ID = 7
IMAGE_WIDTH = 640
IMAGE_HEIGHT = 480


def scene_with_marker(centre_x: int, size: int = 120) -> np.ndarray:
    """Image blanche portant un marqueur ArUco centré sur `centre_x`."""
    image = np.full((IMAGE_HEIGHT, IMAGE_WIDTH), 255, dtype=np.uint8)

    marker = generate_marker(cv2.aruco.DICT_4X4_50, MARKER_ID, size)

    left = centre_x - size // 2
    top = IMAGE_HEIGHT // 2 - size // 2
    image[top : top + size, left : left + size] = marker

    return cv2.cvtColor(image, cv2.COLOR_GRAY2BGR)


@pytest.fixture
def detector() -> FiducialDetector:
    return FiducialDetector(horizontal_fov=1.047)


def test_a_marker_is_found(detector):
    detections = detector.detect(scene_with_marker(IMAGE_WIDTH // 2))

    assert len(detections) == 1
    assert str(MARKER_ID) in detections[0].label


def test_a_marker_on_the_left_has_a_positive_bearing(detector):
    # Le quart gauche de l'image : la marge est large, la détection ne doit pas être
    # à la limite du champ.
    detections = detector.detect(scene_with_marker(IMAGE_WIDTH // 4))

    assert detections[0].bearing is not None
    assert detections[0].bearing > 0.1


def test_a_marker_on_the_right_has_a_negative_bearing(detector):
    detections = detector.detect(scene_with_marker(3 * IMAGE_WIDTH // 4))

    assert detections[0].bearing < -0.1


def test_a_centred_marker_is_roughly_straight_ahead(detector):
    detections = detector.detect(scene_with_marker(IMAGE_WIDTH // 2))

    assert detections[0].bearing == pytest.approx(0.0, abs=0.05)


def test_a_blank_image_yields_nothing(detector):
    blank = np.full((IMAGE_HEIGHT, IMAGE_WIDTH, 3), 255, dtype=np.uint8)

    assert detector.detect(blank) == ()


def test_noise_yields_nothing_and_does_not_crash(detector):
    rng = np.random.default_rng(seed=1)
    noise = rng.integers(0, 256, (IMAGE_HEIGHT, IMAGE_WIDTH, 3), dtype=np.uint8)

    # Une caméra voit du bruit dès qu'il fait sombre. Inventer un marqueur dans du bruit
    # serait pire que de ne rien voir.
    assert detector.detect(noise) == ()


def test_the_bounding_box_covers_the_marker(detector):
    detections = detector.detect(scene_with_marker(IMAGE_WIDTH // 2, size=120))
    found = detections[0]

    assert found.width == pytest.approx(120, abs=8)
    assert found.height == pytest.approx(120, abs=8)


def test_several_markers_are_all_reported(detector):
    image = scene_with_marker(IMAGE_WIDTH // 4)
    second = generate_marker(cv2.aruco.DICT_4X4_50, 12, 120)
    image[180:300, 460:580] = cv2.cvtColor(second, cv2.COLOR_GRAY2BGR)

    labels = {d.label for d in detector.detect(image)}

    assert len(labels) == 2


def test_an_empty_frame_is_handled(detector):
    assert detector.detect(None) == ()
    assert detector.detect(np.zeros((0, 0, 3), dtype=np.uint8)) == ()


def test_analysing_produces_a_summary(detector):
    summary = detector.analyse(scene_with_marker(IMAGE_WIDTH // 2))

    assert summary.width == IMAGE_WIDTH
    assert summary.height == IMAGE_HEIGHT
    assert str(MARKER_ID) in summary.describe()
