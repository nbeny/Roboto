"""Ce que le robot voit, sous une forme qu'on peut transmettre et relire.

Une description de scène n'est pas une image : c'est une poignée de faits — quoi, où,
avec quelle confiance. Elle traverse un topic ROS, une route HTTP puis le contexte d'un
modèle de langage, donc elle doit rester petite, sérialisable et sans ambiguïté.

**Rien ici ne commande quoi que ce soit.** La vision décrit ; c'est la navigation qui
décide, et `robot-safety` qui borne.
"""

from __future__ import annotations

import math
from dataclasses import asdict, dataclass
from datetime import datetime, timezone

__all__ = ["Detection", "SceneSummary", "bearing_from_pixel", "now_iso"]


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def bearing_from_pixel(
    x_centre: float, image_width: int, horizontal_fov: float
) -> float | None:
    """Gisement d'un point de l'image, en radians.

    Convention ROS : positif vers la gauche, dans le sens trigonométrique. Se tromper de
    signe fait tourner le robot du mauvais côté, et c'est le genre d'erreur qui se
    diagnostique difficilement une fois le robot en mouvement.

    Rend `None` plutôt qu'un angle fabriqué quand l'image n'a pas de largeur exploitable
    ou que le point tombe hors cadre : une division par zéro déguisée en gisement
    ressemble à une mesure.
    """
    if image_width <= 0 or not math.isfinite(x_centre):
        return None
    if not 0 <= x_centre <= image_width:
        return None

    # Décalage relatif au centre, dans [-0.5, +0.5], puis inversé pour que la gauche de
    # l'image (x petit) donne un angle positif.
    offset = (x_centre - image_width / 2.0) / image_width
    return -offset * horizontal_fov


@dataclass(frozen=True)
class Detection:
    """Une chose vue, et où elle se trouve dans l'image."""

    label: str
    confidence: float
    x: int
    y: int
    width: int
    height: int
    #: Gisement estimé en radians, ou `None` si l'image ne permet pas de le calculer.
    bearing: float | None

    def __post_init__(self) -> None:
        if not 0.0 <= self.confidence <= 1.0:
            raise ValueError(f"confiance hors de [0, 1] : {self.confidence}")
        if self.bearing is not None and not math.isfinite(self.bearing):
            raise ValueError("un gisement non fini n'est pas un gisement")
        if self.width < 0 or self.height < 0:
            raise ValueError("une boîte englobante ne peut pas avoir de côté négatif")

    def side(self) -> str:
        """Où c'est, en français, pour être lu par un humain ou un modèle."""
        if self.bearing is None:
            return "position indéterminée"
        if self.bearing > 0.09:
            return "sur la gauche"
        if self.bearing < -0.09:
            return "sur la droite"
        return "droit devant, en face"


@dataclass(frozen=True)
class SceneSummary:
    """Tout ce que la caméra a reconnu sur une image."""

    detections: tuple[Detection, ...]
    width: int
    height: int
    at: str

    def by_confidence(self) -> tuple[Detection, ...]:
        """Du plus sûr au moins sûr.

        L'agent lit la description en entier mais retient le début : ce dont on est
        certain doit venir en premier.
        """
        return tuple(sorted(self.detections, key=lambda d: d.confidence, reverse=True))

    def describe(self) -> str:
        """Une phrase, destinée à un humain ou à un modèle de langage."""
        if not self.detections:
            return "La caméra ne reconnaît rien en ce moment."

        pieces = [
            f"{found.label} {found.side()}" for found in self.by_confidence()
        ]
        return "La caméra voit : " + " ; ".join(pieces) + "."

    def to_dict(self) -> dict:
        return {
            "detections": [asdict(found) for found in self.by_confidence()],
            "width": self.width,
            "height": self.height,
            "at": self.at,
            "description": self.describe(),
        }
