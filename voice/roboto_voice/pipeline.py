"""De la parole à l'action.

    parole ──► transcription ──► ordre d'arrêt ? ──oui──► POST /stop
                                        │
                                       non
                                        ▼
                                     agent ──► réponse ──► voix

Le point important est la bifurcation. **Un ordre d'arrêt ne traverse jamais le modèle
de langage.** S'il le faisait, il hériterait de sa latence, de ses limites de débit et
de ses pannes — et « stop » doit fonctionner précisément quand le reste ne fonctionne
plus.

Conséquence directe : sans clé API, la conversation devient impossible mais l'arrêt
reste intact. Une commande vocale qui ne sait plus arrêter serait pire qu'une absence de
commande vocale.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Protocol, Sequence

from .urgent import is_emergency

__all__ = ["Responder", "Speaker", "StopCapable", "Utterance", "VoicePipeline"]


class StopCapable(Protocol):
    """Le strict nécessaire pour arrêter le robot."""

    def stop(self) -> dict[str, Any]: ...


class Responder(Protocol):
    """Ce que la chaîne attend d'un agent conversationnel."""

    def send(self, text: str) -> str: ...


class Speaker(Protocol):
    """Restitution vocale, ou tout autre canal de sortie."""

    def say(self, text: str) -> None: ...


@dataclass(frozen=True)
class Utterance:
    """Une phrase entendue, et ce qu'elle a déclenché."""

    transcript: str
    urgent: bool
    answer: str
    at: str


@dataclass
class VoicePipeline:
    """Aiguille chaque phrase entendue vers l'arrêt ou vers l'agent."""

    robot: StopCapable
    agent: Responder | None
    speaker: Speaker
    _history: list[Utterance] = field(default_factory=list)

    @property
    def history(self) -> Sequence[Utterance]:
        return tuple(self._history)

    def handle(self, transcript: str | None) -> str:
        """Traite une phrase et rend ce qui a été dit en retour."""
        if not transcript or not transcript.strip():
            return ""

        # La bifurcation. Elle vient avant tout appel réseau, avant tout modèle, et ne
        # peut ni échouer ni attendre.
        if is_emergency(transcript):
            answer = self._stop_now()
            urgent = True
        else:
            answer = self._ask_agent(transcript)
            urgent = False

        if answer:
            self.speaker.say(answer)

        self._history.append(
            Utterance(
                transcript=transcript,
                urgent=urgent,
                answer=answer,
                at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
            )
        )
        return answer

    def _stop_now(self) -> str:
        try:
            outcome = self.robot.stop()
        except Exception as error:  # noqa: BLE001
            # La pire nouvelle possible : elle est dite en clair, avec quoi faire
            # ensuite. Jamais avalée dans un message générique.
            return (
                f"ARRÊT NON CONFIRMÉ : {error}. "
                "Utilise l'arrêt d'urgence matériel immédiatement."
            )

        if outcome.get("accepted") is False:
            return f"Arrêt refusé : {outcome.get('message', 'motif inconnu')}"

        return "Arrêt demandé. Le robot est à l'arrêt de sécurité."

    def _ask_agent(self, transcript: str) -> str:
        if self.agent is None:
            return (
                "Je n'ai pas d'agent conversationnel actif : seule la commande d'arrêt "
                "fonctionne. Dis « stop » pour arrêter le robot."
            )

        try:
            return self.agent.send(transcript)
        except Exception as error:  # noqa: BLE001
            # Un agent en panne ne doit pas faire tomber la chaîne : la phrase suivante
            # pourrait être « stop ».
            return f"Erreur de l'agent : {error}. La commande d'arrêt reste active."
