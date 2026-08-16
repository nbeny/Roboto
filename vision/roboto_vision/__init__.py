"""Perception visuelle de Roboto.

La vision **décrit**, elle ne commande pas. Elle produit une `SceneSummary` que d'autres
couches consomment ; aucun chemin ne mène d'ici aux moteurs.
"""

from .scene import Detection, SceneSummary, bearing_from_pixel

__all__ = ["Detection", "SceneSummary", "bearing_from_pixel", "__version__"]

__version__ = "0.1.0"
