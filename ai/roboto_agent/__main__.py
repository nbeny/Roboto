"""Point d'entrée en ligne de commande.

Deux modes :

- `--self-test` exerce les outils directement contre l'API du robot, **sans modèle et
  sans clé API**. C'est ce qui permet de vérifier toute la plomberie — outils, barrière,
  journal, API, cœur, robot — indépendamment du comportement d'un modèle de langage,
  qui lui ne se teste pas de façon déterministe.
- sans argument, une conversation interactive.
"""

from __future__ import annotations

import argparse
import json
import os
import sys

from .agent import MODEL, RobotAgent
from .client import RobotApiClient, RobotApiError
from .safety import AuditLog, OperatingArea
from .tools import TOOL_SCHEMAS, ToolExecutor, tool_names


def build_area() -> OperatingArea:
    """Zone d'évolution, réglable par `ROBOTO_AREA` : `min_x,max_x,min_y,max_y`."""
    raw = os.environ.get("ROBOTO_AREA", "-5,5,-4,4")
    try:
        min_x, max_x, min_y, max_y = (float(part) for part in raw.split(","))
    except ValueError as error:
        raise SystemExit(
            f"ROBOTO_AREA invalide ({raw!r}) : attendu min_x,max_x,min_y,max_y"
        ) from error
    return OperatingArea(min_x=min_x, max_x=max_x, min_y=min_y, max_y=max_y)


def self_test(executor: ToolExecutor, audit: AuditLog, area: OperatingArea) -> int:
    """Exerce chaque outil sans modèle. Rend un code de sortie POSIX."""
    failures: list[str] = []

    def check(label: str, answer: str, must_fail: bool = False) -> None:
        broken = any(
            marker in answer.lower()
            for marker in ("impossible", "refusé", "inconnu", "non confirmé")
        )
        if broken is not must_fail:
            failures.append(f"{label} : {answer}")
            print(f"  ECHEC  {label} — {answer}")
        else:
            print(f"  OK     {label}")

    print(f"== Catalogue : {', '.join(tool_names())} ==")
    print(f"== Zone d'évolution : {area.describe()} ==")

    print("== Lectures ==")
    check("get_robot_state", executor.get_robot_state())
    check("get_pose", executor.get_pose())

    perceived = executor.inspect()
    check("inspect", perceived)
    # La vision est facultative : son absence n'est pas un échec, mais elle doit être
    # visible. Un « OK » silencieux laisserait croire que le robot a regardé.
    if '"vision": "indisponible' in perceived:
        print("  NOTE   la perception visuelle ne publie pas — le robot est aveugle")
    else:
        print("  OK     la caméra publie")

    print("== La barrière refuse ce qui sort de la zone ==")
    # Aucun appel réseau ne doit partir : c'est la propriété qui porte le milestone.
    check("navigate_to hors zone", executor.navigate_to(9_000.0, 0.0), must_fail=True)
    check("navigate_to non numérique", executor.navigate_to("là-bas", 0.0), must_fail=True)
    check("outil inconnu", executor.dispatch("set_wheel_speed", {}), must_fail=True)

    print("== Journal d'audit ==")
    for entry in audit.entries:
        print(f"  {entry.at}  {entry.name:<18} {entry.outcome}")

    if failures:
        print(f"\n{len(failures)} vérification(s) en échec.")
        return 1

    print("\nToutes les vérifications de l'agent sont passées.")
    return 0


def drive_test(executor: ToolExecutor, audit: AuditLog) -> int:
    """Fait réellement bouger le robot, par les outils de l'agent et rien d'autre.

    C'est la vérification qui compte : elle prouve que la chaîne agent -> API -> cœur ->
    sécurité -> Nav2 -> robot tient de bout en bout, et que l'arrêt la traverse aussi.
    """
    import time

    def state() -> str:
        return json.loads(executor.get_robot_state()).get("state") or "?"

    def speed() -> float:
        velocity = json.loads(executor.get_robot_state()).get("velocity")
        return abs(velocity["linear"]) if velocity else 0.0

    def wait(label: str, predicate, seconds: float = 90.0) -> bool:
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if predicate():
                print(f"  OK     {label}")
                return True
            time.sleep(1.0)
        print(f"  ECHEC  {label} — jamais atteint en {seconds:.0f} s")
        return False

    print("== 1. Autorisation de naviguer ==")
    print(f"  état initial : {state()}")
    executor.allow_navigation()
    if not wait("le cœur passe en NAVIGATING", lambda: state() == "NAVIGATING"):
        return 1

    print("== 2. Un but hors zone ne part pas ==")
    answer = executor.navigate_to(9_000.0, 0.0)
    if "refus" not in answer.lower():
        print(f"  ECHEC  but hors zone accepté : {answer}")
        return 1
    print("  OK     refusé par la barrière, avant tout appel réseau")

    print("== 3. Un but dans la zone met le robot en mouvement ==")
    answer = executor.navigate_to(1.2, 0.0)
    if "refus" in answer.lower() or "impossible" in answer.lower():
        print(f"  ECHEC  but refusé : {answer}")
        return 1
    if not wait("le robot avance", lambda: speed() > 0.05):
        return 1
    print(f"  vitesse lue par l'agent : {speed():.2f} m/s")

    print("== 4. L'arrêt de l'agent arrête réellement le robot ==")
    executor.stop()
    if not wait(
        "SAFE_STOP et vitesse nulle",
        lambda: state() == "SAFE_STOP" and speed() < 0.02,
        seconds=30.0,
    ):
        return 1

    print("== Journal d'audit ==")
    for entry in audit.entries:
        print(f"  {entry.at}  {entry.name:<18} {entry.outcome}")

    print("\nL'agent a conduit et arrêté le robot en traversant toute la chaîne.")
    return 0


def converse(agent: RobotAgent, audit: AuditLog) -> int:
    print(f"Agent Roboto ({MODEL}). Ctrl-C pour quitter.\n")
    try:
        while True:
            try:
                message = input("> ").strip()
            except EOFError:
                break
            if not message:
                continue
            print(f"\n{agent.send(message)}\n")
    except KeyboardInterrupt:
        print()

    if audit.entries:
        print("\n== Ce que l'agent a tenté ==")
        for entry in audit.entries:
            print(f"  {entry.at}  {entry.name:<18} {entry.outcome}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="roboto-agent", description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="exerce les outils contre l'API, sans modèle ni clé",
    )
    parser.add_argument(
        "--drive-test",
        action="store_true",
        help="FAIT BOUGER LE ROBOT : conduit et arrête par les outils, sans modèle",
    )
    parser.add_argument(
        "--print-tools", action="store_true", help="affiche le catalogue et sort"
    )
    parser.add_argument(
        "--api",
        default=os.environ.get("ROBOTO_API", "http://127.0.0.1:8080"),
        help="adresse de l'API du robot",
    )
    args = parser.parse_args()

    if args.print_tools:
        print(json.dumps(TOOL_SCHEMAS, ensure_ascii=False, indent=2))
        return 0

    area = build_area()
    audit = AuditLog()
    executor = ToolExecutor(RobotApiClient(args.api), area, audit)

    if args.self_test or args.drive_test:
        try:
            if args.self_test and self_test(executor, audit, area) != 0:
                return 1
            return drive_test(executor, audit) if args.drive_test else 0
        except RobotApiError as error:
            print(f"ECHEC  {error}")
            return 1

    try:
        import anthropic
    except ImportError:
        print("Le SDK anthropic est requis pour la conversation : pip install anthropic")
        return 1

    if not (os.environ.get("ANTHROPIC_API_KEY") or os.environ.get("ANTHROPIC_AUTH_TOKEN")):
        print(
            "Aucune identification Anthropic trouvée. Définis ANTHROPIC_API_KEY, ou "
            "lance `ant auth login`. Pour vérifier la plomberie sans modèle : "
            "roboto-agent --self-test"
        )
        return 1

    return converse(RobotAgent(anthropic.Anthropic(), executor), audit)


if __name__ == "__main__":
    sys.exit(main())
