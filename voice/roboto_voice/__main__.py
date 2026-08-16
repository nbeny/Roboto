"""Commande vocale en ligne de commande.

Trois modes, du plus vérifiable au plus complet :

- `--self-test` : passe une série de phrases dans la chaîne et montre lesquelles
  arrêtent le robot. Sans micro, sans modèle, sans clé API.
- `--text` : on tape ce qu'on dirait. Toute la chaîne, sauf la transcription.
- par défaut : transcription de fichiers audio par Whisper.
"""

from __future__ import annotations

import argparse
import os
import sys

from .pipeline import VoicePipeline
from .speech import ConsoleSpeaker, best_available_speaker
from .urgent import is_emergency

#: Phrases de contrôle : ce qui doit arrêter, ce qui ne doit pas.
SELF_TEST_PHRASES = [
    ("stop", True),
    ("STOP !", True),
    ("arrête-toi", True),
    ("arrete toi", True),
    ("au secours", True),
    ("attention", True),
    ("ne t'arrête pas", True),
    ("va dans la cuisine", False),
    ("où es-tu ?", False),
    ("que vois-tu", False),
    ("tourne à gauche", False),
    ("", False),
]


class HttpRobot:
    """Arrêt du robot par l'API. Aucune autre capacité."""

    def __init__(self, base_url: str) -> None:
        self._base = base_url.rstrip("/")

    def stop(self) -> dict:
        import json
        import urllib.request

        request = urllib.request.Request(f"{self._base}/api/robot/stop", method="POST")
        with urllib.request.urlopen(request, timeout=10) as response:
            raw = response.read()
        return json.loads(raw) if raw else {"accepted": True}


def self_test() -> int:
    """Vérifie le tri des phrases, sans rien démarrer."""
    print("== Ce qui arrête le robot, et ce qui ne l'arrête pas ==\n")

    failures = 0
    for phrase, expected in SELF_TEST_PHRASES:
        actual = is_emergency(phrase)
        mark = "OK    " if actual == expected else "ECHEC "
        arrow = "ARRÊT" if actual else "agent"
        if actual != expected:
            failures += 1
        print(f"  {mark} {arrow:<6} « {phrase} »")

    print(
        "\n« ne t'arrête pas » arrête aussi : c'est délibéré. Un arrêt inutile coûte\n"
        "quelques secondes ; un arrêt manqué coûte potentiellement quelqu'un."
    )

    if failures:
        print(f"\n{failures} phrase(s) mal classée(s).")
        return 1

    print("\nToutes les phrases sont correctement classées.")
    return 0


def build_agent(api_url: str):
    """Agent conversationnel, ou `None` si rien n'est disponible.

    L'absence d'agent n'est pas une erreur : la commande d'arrêt continue de
    fonctionner, et c'est elle qui compte.
    """
    if not (os.environ.get("ANTHROPIC_API_KEY") or os.environ.get("ANTHROPIC_AUTH_TOKEN")):
        return None

    try:
        import anthropic
        from roboto_agent.agent import RobotAgent
        from roboto_agent.client import RobotApiClient
        from roboto_agent.safety import AuditLog, OperatingArea
        from roboto_agent.tools import ToolExecutor
    except ImportError:
        return None

    raw = os.environ.get("ROBOTO_AREA", "-5,5,-4,4")
    min_x, max_x, min_y, max_y = (float(part) for part in raw.split(","))

    executor = ToolExecutor(
        RobotApiClient(api_url),
        OperatingArea(min_x=min_x, max_x=max_x, min_y=min_y, max_y=max_y),
        AuditLog(),
    )
    return RobotAgent(anthropic.Anthropic(), executor)


def main() -> int:
    parser = argparse.ArgumentParser(prog="roboto-voice", description=__doc__)
    parser.add_argument(
        "--self-test", action="store_true", help="vérifie le tri des phrases et sort"
    )
    parser.add_argument(
        "--text", action="store_true", help="dicter au clavier plutôt qu'au micro"
    )
    parser.add_argument("--audio", help="transcrire un fichier audio et sortir")
    parser.add_argument(
        "--say",
        metavar="PHRASE",
        help="traiter une seule phrase et sortir — sert aux essais de bout en bout",
    )
    parser.add_argument(
        "--api",
        default=os.environ.get("ROBOTO_API", "http://127.0.0.1:8080"),
        help="adresse de l'API du robot",
    )
    parser.add_argument("--model", default="base", help="modèle Whisper")
    parser.add_argument("--quiet", action="store_true", help="pas de synthèse vocale")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    speaker = ConsoleSpeaker() if args.quiet else best_available_speaker()
    agent = build_agent(args.api)
    pipeline = VoicePipeline(HttpRobot(args.api), agent, speaker)

    if agent is None:
        print(
            "Aucun agent conversationnel : seule la commande d'arrêt est active.\n"
            "Définis ANTHROPIC_API_KEY pour la conversation.\n"
        )

    if args.say:
        answer = pipeline.handle(args.say)
        # Code de sortie parlant : 0 si la phrase a bien été traitée, 1 si un arrêt
        # demandé n'a pas été confirmé. Un essai peut s'y fier.
        return 1 if "NON CONFIRMÉ" in answer else 0

    if args.audio:
        from .transcribe import WhisperTranscriber

        transcript = WhisperTranscriber(args.model).transcribe(args.audio)
        print(f"  entendu > {transcript}")
        pipeline.handle(transcript)
        return 0

    if not args.text:
        print(
            "La capture micro n'est pas encore câblée (pas de carte son sur cette "
            "machine). Utilise --text pour dicter, ou --audio pour un fichier.\n"
        )
        return 1

    print("Dis quelque chose. « stop » arrête le robot. Ctrl-C pour quitter.\n")
    try:
        while True:
            try:
                said = input("vous > ").strip()
            except EOFError:
                break
            pipeline.handle(said)
    except KeyboardInterrupt:
        print()

    if pipeline.history:
        print("\n== Ce qui a été entendu ==")
        for utterance in pipeline.history:
            kind = "ARRÊT" if utterance.urgent else "agent"
            print(f"  {utterance.at}  {kind:<6} « {utterance.transcript} »")

    return 0


if __name__ == "__main__":
    sys.exit(main())
