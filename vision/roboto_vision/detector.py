"""Le contrat que doit remplir un détecteur, et la composition de plusieurs.

Un détecteur d'objets généraliste — MobileNet-SSD, YOLO — se branche ici sans rien
changer au reste. Il n'est pas fourni par défaut : ses poids se téléchargent, pèsent des
dizaines de mégaoctets, et ne peuvent donc pas être vérifiés hors ligne. Les marqueurs
ArUco, eux, le peuvent. Le socle vérifiable d'abord ; le reste s'ajoute.
"""

from __future__ import annotations

from typing import Any, Protocol, Sequence

from .scene import Detection, SceneSummary, now_iso

__all__ = ["Detector", "CompositeDetector"]


class Detector(Protocol):
    """Une image en entrée, des détections en sortie. Rien d'autre."""

    def detect(self, image: Any) -> tuple[Detection, ...]: ...


class CompositeDetector:
    """Fait tourner plusieurs détecteurs sur la même image et fusionne le résultat.

    Un détecteur qui échoue ne fait pas tomber les autres : perdre les marqueurs parce
    qu'un réseau de neurones a manqué de mémoire serait absurde.
    """

    def __init__(self, detectors: Sequence[Detector], horizontal_fov: float = 1.047) -> None:
        self._detectors = tuple(detectors)
        self._fov = horizontal_fov

    def detect(self, image: Any) -> tuple[Detection, ...]:
        found: list[Detection] = []
        for detector in self._detectors:
            try:
                found.extend(detector.detect(image))
            except Exception:  # noqa: BLE001 — un détecteur défaillant est isolé
                continue
        return tuple(found)

    def analyse(self, image: Any) -> SceneSummary:
        shape = getattr(image, "shape", None)
        height, width = (shape[0], shape[1]) if shape and len(shape) >= 2 else (0, 0)

        return SceneSummary(
            detections=self.detect(image), width=width, height=height, at=now_iso()
        )
