"""La frontière entre l'agent et le robot.

Ce module n'importe rien : ni le SDK Anthropic, ni le client HTTP, ni ROS 2. C'est
délibéré. Ce qui borne un modèle de langage doit pouvoir être lu, compris et vérifié
sans lui.

Il ne duplique aucune règle de `robot-safety`. Le cœur Rust reste seul à décider ce
qu'un robot peut faire ; ce module décide seulement ce qu'un agent peut *demander*.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Sequence

__all__ = ["AuditLog", "OperatingArea", "Refusal", "ToolInvocation", "check_goal"]


def _is_finite_number(value: Any) -> bool:
    """Vrai pour un nombre réel fini, et pour rien d'autre.

    `bool` est exclu explicitement : en Python il hérite de `int`, donc une
    vérification naïve accepterait `True` comme une abscisse valant un mètre.
    """
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return False
    return math.isfinite(value)


@dataclass(frozen=True)
class OperatingArea:
    """Rectangle du repère `map` hors duquel l'agent ne peut envoyer le robot.

    Le modèle peut se tromper de coordonnée — il produit du texte plausible, pas des
    mesures. Cette zone est la borne qui transforme une hallucination en refus plutôt
    qu'en trajet de cinq cents mètres.
    """

    min_x: float
    max_x: float
    min_y: float
    max_y: float

    def __post_init__(self) -> None:
        for name in ("min_x", "max_x", "min_y", "max_y"):
            if not _is_finite_number(getattr(self, name)):
                raise ValueError(f"{name} doit être un nombre fini")

        # Une zone vide accepterait tout ou rien selon l'ordre des comparaisons. Mieux
        # vaut échouer au démarrage qu'à la première commande.
        if self.min_x >= self.max_x or self.min_y >= self.max_y:
            raise ValueError("zone d'évolution vide : les bornes sont inversées")

    def contains(self, x: float, y: float) -> bool:
        """Bornes incluses : un but posé sur le bord reste dans la zone."""
        return self.min_x <= x <= self.max_x and self.min_y <= y <= self.max_y

    def describe(self) -> str:
        return (
            f"x de {self.min_x:.1f} à {self.max_x:.1f} m, "
            f"y de {self.min_y:.1f} à {self.max_y:.1f} m"
        )


@dataclass(frozen=True)
class Refusal:
    """Pourquoi une demande de l'agent n'a pas été transmise.

    Le motif est rendu à l'agent tel quel : c'est ainsi qu'il corrige son tir au lieu
    de réessayer la même chose.
    """

    reason: str


def check_goal(x: Any, y: Any, area: OperatingArea) -> Refusal | None:
    """Valide un but de navigation. Rend `None` s'il est acceptable.

    Deux refus seulement, et ils sont de nature différente : une coordonnée qui n'est
    pas un nombre fini est une erreur de forme, une coordonnée hors zone est une
    demande recevable mais bornée.
    """
    if not _is_finite_number(x) or not _is_finite_number(y):
        return Refusal("x et y doivent être des nombres finis, en mètres")

    if not area.contains(float(x), float(y)):
        return Refusal(
            f"le but ({x:.2f} ; {y:.2f}) sort de la zone d'évolution "
            f"({area.describe()})"
        )

    return None


@dataclass(frozen=True)
class ToolInvocation:
    """Une chose que l'agent a tentée, et ce qui en est advenu."""

    name: str
    arguments: dict[str, Any]
    outcome: str
    at: str


@dataclass
class AuditLog:
    """Trace de tout ce que l'agent a tenté.

    Les refus y figurent au même titre que les acceptations. Un journal qui ne
    garderait que les succès rendrait invisible exactement ce qu'on veut pouvoir
    relire : ce que l'agent a essayé de faire et qui lui a été refusé.
    """

    _entries: list[ToolInvocation] = field(default_factory=list)

    def record(self, name: str, arguments: dict[str, Any], outcome: str) -> None:
        self._entries.append(
            ToolInvocation(
                name=name,
                arguments=dict(arguments),
                outcome=outcome,
                at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
            )
        )

    @property
    def entries(self) -> Sequence[ToolInvocation]:
        """Vue en lecture seule : un journal d'audit modifiable n'en est pas un."""
        return tuple(self._entries)
