"""Compatibilité entre les deux âges de l'API ArUco d'OpenCV.

OpenCV a refondu ArUco en 4.7 : `Dictionary_get` et `DetectorParameters_create` ont
laissé la place à `getPredefinedDictionary` et à la classe `ArucoDetector`.

Les deux comptent ici. L'image ROS 2 Jazzy, et donc le Raspberry Pi sous Ubuntu 24.04,
livrent OpenCV **4.6** ; une machine de développement récente installe **5.x**. Écrire
pour l'une casse l'autre, et découvrir cela sur le robot serait cher payé.

Tout le reste de la bibliothèque passe par ici et ignore la question.
"""

from __future__ import annotations

from typing import Any, Callable

import cv2

__all__ = ["MODERN_API", "generate_marker", "make_marker_detector"]

#: Vrai à partir d'OpenCV 4.7, où `ArucoDetector` existe.
MODERN_API: bool = hasattr(cv2.aruco, "ArucoDetector")


def make_marker_detector(dictionary_id: int) -> Callable[[Any], tuple]:
    """Rend une fonction `image_en_niveaux_de_gris -> (coins, identifiants, rejetés)`."""
    if MODERN_API:
        detector = cv2.aruco.ArucoDetector(
            cv2.aruco.getPredefinedDictionary(dictionary_id),
            cv2.aruco.DetectorParameters(),
        )
        return detector.detectMarkers

    dictionary = cv2.aruco.Dictionary_get(dictionary_id)
    parameters = cv2.aruco.DetectorParameters_create()

    def detect(grey: Any) -> tuple:
        return cv2.aruco.detectMarkers(grey, dictionary, parameters=parameters)

    return detect


def generate_marker(dictionary_id: int, marker_id: int, size: int) -> Any:
    """Image en niveaux de gris d'un marqueur, à imprimer ou à donner aux essais."""
    if MODERN_API:
        dictionary = cv2.aruco.getPredefinedDictionary(dictionary_id)
        return cv2.aruco.generateImageMarker(dictionary, marker_id, size)

    dictionary = cv2.aruco.Dictionary_get(dictionary_id)
    return cv2.aruco.drawMarker(dictionary, marker_id, size)
