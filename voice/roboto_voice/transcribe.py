"""Transcription de la parole en texte.

Whisper est le bon outil ici, et c'est exactement le cas où Python s'impose : le
réimplémenter en Rust pour respecter une préférence de langage serait absurde.

Il n'est pas obligatoire pour autant. Le contrat est un protocole, la dépendance est
optionnelle, et le reste de la chaîne — dont le court-circuit d'arrêt — fonctionne sans.
"""

from __future__ import annotations

from typing import Protocol

__all__ = ["Transcriber", "WhisperTranscriber", "TextTranscriber"]


class Transcriber(Protocol):
    """Un chemin de fichier audio en entrée, du texte en sortie."""

    def transcribe(self, audio_path: str) -> str: ...


class TextTranscriber:
    """Transcripteur de substitution qui lit un fichier texte.

    Sert aux essais et à la mise au point : on veut pouvoir exercer toute la chaîne
    vocale sans micro, sans modèle et sans carte son.
    """

    def transcribe(self, audio_path: str) -> str:
        with open(audio_path, "r", encoding="utf-8") as handle:
            return handle.read().strip()


class WhisperTranscriber:
    """Transcription locale par Whisper.

    Le modèle est chargé **paresseusement**, au premier usage : `tiny` pèse déjà
    75 Mo et se télécharge à la première exécution. Le charger à la construction
    ferait attendre le démarrage même si personne ne parle jamais.
    """

    def __init__(self, model_name: str = "base", language: str = "fr") -> None:
        self._model_name = model_name
        self._language = language
        self._model = None

    def _ensure_model(self):
        if self._model is not None:
            return self._model

        try:
            import whisper
        except ImportError as error:
            raise RuntimeError(
                "Whisper n'est pas installé. `pip install -e 'voice/[whisper]'`, "
                "ou utilise --text pour dicter au clavier."
            ) from error

        self._model = whisper.load_model(self._model_name)
        return self._model

    def transcribe(self, audio_path: str) -> str:
        model = self._ensure_model()
        result = model.transcribe(audio_path, language=self._language, fp16=False)
        return str(result.get("text", "")).strip()
