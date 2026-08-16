"""Reconnaissance d'un ordre d'arrêt, avant toute intelligence.

Ce module n'importe **rien** : ni réseau, ni modèle, ni client HTTP. C'est ce qui lui
permet d'être placé en tête du traitement vocal. S'il pouvait échouer, attendre ou
dépendre d'un service, ce ne serait pas un court-circuit.

Le parti pris est assumé : **on arrête au moindre doute.** « ne t'arrête pas » contient
« arrête » et déclenche donc l'arrêt. L'asymétrie est totale — un arrêt inutile coûte
quelques secondes et une reprise explicite, un arrêt manqué coûte potentiellement
quelqu'un.
"""

from __future__ import annotations

import re
import unicodedata

__all__ = ["EMERGENCY_WORDS", "is_emergency", "normalise"]

#: Racines de mots qui arrêtent le robot. Comparées après normalisation, sur des mots
#: entiers étendus à leur famille — « arrête », « arrêtes », « arrêtez », « arrêter ».
EMERGENCY_WORDS: tuple[str, ...] = (
    "stop",
    "stoppe",
    "stopper",
    "arrete",
    "arretes",
    "arretez",
    "arreter",
    "halte",
    "urgence",
    "danger",
    "attention",
    "secours",
)

_SEPARATORS = re.compile(r"[^a-z0-9]+")


def normalise(text: str | None) -> str:
    """Minuscules, sans accents, ponctuation remplacée par des espaces.

    Une transcription vocale arrive avec une casse, une ponctuation et des accents
    imprévisibles. Dépendre de l'un des trois rendrait l'arrêt aléatoire.
    """
    if not text:
        return ""

    decomposed = unicodedata.normalize("NFKD", text)
    without_accents = "".join(c for c in decomposed if not unicodedata.combining(c))

    return _SEPARATORS.sub(" ", without_accents.lower()).strip()


def is_emergency(text: str | None) -> bool:
    """Vrai si cette phrase doit arrêter le robot immédiatement."""
    words = normalise(text).split()

    # Mots entiers, et non sous-chaînes : « les astrophysiciens » ne doit pas arrêter un
    # robot sous prétexte qu'il contient les lettres de « stop ».
    return any(word in EMERGENCY_WORDS for word in words)
