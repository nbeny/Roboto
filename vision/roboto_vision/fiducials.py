"""Détection de marqueurs ArUco.

Pourquoi des marqueurs plutôt qu'un détecteur d'objets généraliste : ils fonctionnent
**hors ligne**, sans poids de modèle à télécharger, tiennent sur un Raspberry Pi sans
accélérateur, et donnent une identité **exacte** — le marqueur 7 est le marqueur 7, il
n'y a pas de « chien à 62 % ». Pour un robot qui doit reconnaître une station de charge
ou un point de passage, c'est exactement ce qu'il faut.

Un détecteur d'objets se branche à côté, via le protocole `Detector` : voir
`detector.py`.
"""

from __future__ import annotations

from typing import Any

import cv2
import numpy as np

from .compat import make_marker_detector
from .scene import Detection, SceneSummary, bearing_from_pixel, now_iso

__all__ = ["FiducialDetector"]


class FiducialDetector:
    """Reconnaît les marqueurs ArUco présents dans une image BGR."""

    #: Champ horizontal de la caméra du modèle, en radians (60°).
    DEFAULT_FOV = 1.047

    def __init__(
        self,
        horizontal_fov: float = DEFAULT_FOV,
        dictionary: int = cv2.aruco.DICT_4X4_50,
    ) -> None:
        self._fov = horizontal_fov
        # Passe par `compat` : l'image ROS livre OpenCV 4.6, une machine récente 5.x, et
        # l'API ArUco a changé entre les deux.
        self._detect_markers = make_marker_detector(dictionary)

    def detect(self, image: Any) -> tuple[Detection, ...]:
        """Marqueurs trouvés, ou un tuple vide.

        Une image absente, vide ou illisible ne lève pas : une caméra qui décroche est
        un incident banal, et une exception remontant jusqu'à la boucle de contrôle
        serait une bien plus mauvaise nouvelle qu'une image manquante.
        """
        if image is None or not isinstance(image, np.ndarray) or image.size == 0:
            return ()

        grey = image if image.ndim == 2 else cv2.cvtColor(image, cv2.COLOR_BGR2GRAY)

        try:
            corners, ids, _ = self._detect_markers(grey)
        except cv2.error:
            return ()

        if ids is None or len(ids) == 0:
            return ()

        width = image.shape[1]
        found: list[Detection] = []

        for quad, marker_id in zip(corners, ids.flatten()):
            points = quad.reshape(-1, 2)
            x_min, y_min = points.min(axis=0)
            x_max, y_max = points.max(axis=0)

            found.append(
                Detection(
                    label=f"marqueur {int(marker_id)}",
                    # Un ArUco est lu ou ne l'est pas : le code correcteur du
                    # dictionnaire ne laisse pas de place au doute. D'où 1.0, plutôt
                    # qu'un score inventé pour faire comme un réseau de neurones.
                    confidence=1.0,
                    x=int(x_min),
                    y=int(y_min),
                    width=int(x_max - x_min),
                    height=int(y_max - y_min),
                    bearing=bearing_from_pixel((x_min + x_max) / 2.0, width, self._fov),
                )
            )

        return tuple(found)

    def analyse(self, image: Any) -> SceneSummary:
        """Description complète de l'image."""
        height, width = (image.shape[0], image.shape[1]) if _usable(image) else (0, 0)

        return SceneSummary(
            detections=self.detect(image),
            width=width,
            height=height,
            at=now_iso(),
        )


def _usable(image: Any) -> bool:
    return isinstance(image, np.ndarray) and image.ndim >= 2 and image.size > 0
