"""Restitution de la réponse.

Trois sorties possibles, par ordre de préférence, et **aucune n'est obligatoire** : une
synthèse vocale absente ne doit pas empêcher d'arrêter le robot.
"""

from __future__ import annotations

import shutil
import subprocess
import sys

__all__ = ["ConsoleSpeaker", "EspeakSpeaker", "best_available_speaker"]


class ConsoleSpeaker:
    """Écrit la réponse. Toujours disponible, et suffisant pour la mise au point."""

    def say(self, text: str) -> None:
        print(f"\n  robot > {text}\n", flush=True)


class EspeakSpeaker:
    """Synthèse par `espeak-ng`, présent sur la plupart des Linux.

    Choisi plutôt qu'une bibliothèque Python : c'est un binaire déjà là, sans compilation
    ni modèle à télécharger. Sur le robot, la qualité importe moins que le fait de
    fonctionner sans réseau.
    """

    def __init__(self, voice: str = "fr", speed: int = 160) -> None:
        self._voice = voice
        self._speed = speed

    @staticmethod
    def available() -> bool:
        return shutil.which("espeak-ng") is not None or shutil.which("espeak") is not None

    def say(self, text: str) -> None:
        binary = shutil.which("espeak-ng") or shutil.which("espeak")
        if binary is None:
            print(f"  robot > {text}", flush=True)
            return

        print(f"  robot > {text}", flush=True)
        try:
            subprocess.run(
                [binary, "-v", self._voice, "-s", str(self._speed), text],
                check=False,
                timeout=30,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            # Ne pas parler n'est pas un incident : le texte est déjà affiché.
            print(f"  (synthèse vocale indisponible : {error})", file=sys.stderr)


def best_available_speaker():
    """La meilleure sortie disponible, sans jamais échouer."""
    return EspeakSpeaker() if EspeakSpeaker.available() else ConsoleSpeaker()
