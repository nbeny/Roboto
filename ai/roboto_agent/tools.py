"""Ce que l'agent peut demander au robot.

Le catalogue est de la donnée pure : il se sérialise, se relit et se teste sans rien
exécuter. L'exécution est ailleurs, dans `ToolExecutor`, qui passe par la frontière de
`safety` avant de transmettre quoi que ce soit à l'API.

**Aucun outil ne prend une vitesse.** L'agent dit où aller, pas à quelle allure — la
consigne est calculée par Nav2, puis bornée par `robot-safety`. C'est ce qui donne son
sens à `CommandSource::Ai`, refusée dans les huit états du cœur depuis le premier jour :
l'agent n'a aucun moyen d'émettre une commande de mouvement, même s'il le voulait.
"""

from __future__ import annotations

import json
from typing import Any, Protocol

from .safety import AuditLog, OperatingArea, check_goal

__all__ = ["TOOL_SCHEMAS", "RobotApi", "ToolExecutor", "tool_names"]


TOOL_SCHEMAS: list[dict[str, Any]] = [
    {
        "name": "get_robot_state",
        "description": (
            "Rend l'état opérationnel du robot, son allure courante et sa mission en "
            "cours. Appelle cet outil avant toute décision de déplacement, et chaque "
            "fois que l'utilisateur demande ce que fait le robot ou pourquoi il ne "
            "bouge pas."
        ),
        "input_schema": {"type": "object", "properties": {}, "required": []},
    },
    {
        "name": "get_pose",
        "description": (
            "Rend la position et le cap du robot dans le repère de la carte. Utilise "
            "cet outil quand tu as besoin de savoir où le robot se trouve, notamment "
            "pour situer une destination par rapport à lui."
        ),
        "input_schema": {"type": "object", "properties": {}, "required": []},
    },
    {
        "name": "inspect",
        "description": (
            "Rend ce que le robot perçoit maintenant : obstacle le plus proche relevé "
            "par le télémètre laser, nombre de mesures valides, portée du capteur. "
            "Appelle cet outil lorsque l'utilisateur demande si la voie est libre, ce "
            "qu'il y a autour du robot, ou avant de lancer un déplacement dans une "
            "zone dont tu ignores l'encombrement."
        ),
        "input_schema": {"type": "object", "properties": {}, "required": []},
    },
    {
        "name": "navigate_to",
        "description": (
            "Envoie le robot à un point de la carte. Rend la main dès que l'ordre est "
            "transmis, sans attendre l'arrivée : une mission dure des minutes. Appelle "
            "cet outil quand l'utilisateur demande un déplacement vers un endroit "
            "précis. Le robot doit être autorisé à naviguer au préalable — vérifie son "
            "état si l'ordre est refusé."
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "x": {
                    "type": "number",
                    "description": "Abscisse de la destination, en mètres.",
                },
                "y": {
                    "type": "number",
                    "description": "Ordonnée de la destination, en mètres.",
                },
            },
            "required": ["x", "y"],
        },
    },
    {
        "name": "allow_navigation",
        "description": (
            "Demande au cœur d'autoriser la navigation autonome. Appelle cet outil "
            "quand un déplacement a été refusé parce que le robot n'était pas dans "
            "l'état voulu. Le cœur reste libre de refuser la transition."
        ),
        "input_schema": {"type": "object", "properties": {}, "required": []},
    },
    {
        "name": "stop",
        "description": (
            "Arrête le robot immédiatement. Appelle cet outil dès que l'utilisateur "
            "demande l'arrêt, exprime une inquiétude, ou dès que tu constates une "
            "situation que tu ne comprends pas. En cas de doute, arrête : la reprise "
            "est explicite et sans risque, la poursuite ne l'est pas."
        ),
        "input_schema": {"type": "object", "properties": {}, "required": []},
    },
]


def tool_names() -> list[str]:
    return [schema["name"] for schema in TOOL_SCHEMAS]


class RobotApi(Protocol):
    """Le peu que l'exécuteur attend de l'API.

    Un protocole plutôt qu'une classe concrète : les essais fournissent un double sans
    réseau, et l'exécuteur n'a aucun moyen d'atteindre ROS 2 même s'il le cherchait.
    """

    def status(self) -> dict[str, Any]: ...
    def pose(self) -> dict[str, Any]: ...
    def sensors(self) -> dict[str, Any]: ...
    def navigate(self, x: float, y: float) -> dict[str, Any]: ...
    def request_state(self, state: str) -> dict[str, Any]: ...
    def stop(self) -> dict[str, Any]: ...


class ToolExecutor:
    """Exécute les outils de l'agent, en passant par la frontière de sécurité.

    Chaque méthode rend une chaîne : c'est ce que le modèle relira. Les erreurs y sont
    formulées pour lui — un message qui explique *pourquoi* c'est refusé lui permet de
    corriger son tir, là où un code d'erreur le ferait réessayer à l'identique.
    """

    def __init__(self, api: RobotApi, area: OperatingArea, audit: AuditLog) -> None:
        self._api = api
        self._area = area
        self._audit = audit

    # --- Lecture ---------------------------------------------------------------------

    def get_robot_state(self) -> str:
        return self._read("get_robot_state", self._api.status)

    def get_pose(self) -> str:
        return self._read("get_pose", self._api.pose)

    def inspect(self) -> str:
        return self._read("inspect", self._api.sensors)

    def _read(self, name: str, call) -> str:
        try:
            payload = call()
        except Exception as error:  # noqa: BLE001 — rendu au modèle, pas propagé
            self._audit.record(name, {}, f"échec : {error}")
            return f"Lecture impossible : {error}"

        self._audit.record(name, {}, "lu")
        return json.dumps(payload, ensure_ascii=False)

    # --- Action ----------------------------------------------------------------------

    def navigate_to(self, x: Any, y: Any) -> str:
        refusal = check_goal(x, y, self._area)
        if refusal is not None:
            # Refusé avant tout appel réseau : la frontière est ici, pas dans l'API.
            self._audit.record("navigate_to", {"x": x, "y": y}, f"refusé : {refusal.reason}")
            return f"But refusé : {refusal.reason}"

        try:
            outcome = self._api.navigate(float(x), float(y))
        except Exception as error:  # noqa: BLE001
            self._audit.record("navigate_to", {"x": x, "y": y}, f"échec : {error}")
            return f"Envoi impossible : {error}"

        self._audit.record("navigate_to", {"x": x, "y": y}, str(outcome.get("message", outcome)))
        return json.dumps(outcome, ensure_ascii=False)

    def allow_navigation(self) -> str:
        try:
            outcome = self._api.request_state("NAVIGATING")
        except Exception as error:  # noqa: BLE001
            self._audit.record("allow_navigation", {}, f"échec : {error}")
            return f"Demande impossible : {error}"

        self._audit.record("allow_navigation", {}, str(outcome.get("message", outcome)))
        return json.dumps(outcome, ensure_ascii=False)

    def stop(self) -> str:
        try:
            outcome = self._api.stop()
        except Exception as error:  # noqa: BLE001
            # Un arrêt qui échoue est la pire nouvelle possible : elle est dite
            # explicitement, jamais avalée.
            self._audit.record("stop", {}, f"ÉCHEC : {error}")
            return (
                f"ARRÊT NON CONFIRMÉ : {error}. "
                "Préviens l'utilisateur immédiatement et demande-lui d'utiliser "
                "l'arrêt d'urgence matériel."
            )

        self._audit.record("stop", {}, str(outcome.get("message", outcome)))
        return json.dumps(outcome, ensure_ascii=False)

    def dispatch(self, name: str, arguments: dict[str, Any]) -> str:
        """Exécute par nom. Un outil inconnu est refusé, jamais deviné."""
        handler = {
            "get_robot_state": lambda: self.get_robot_state(),
            "get_pose": lambda: self.get_pose(),
            "inspect": lambda: self.inspect(),
            "allow_navigation": lambda: self.allow_navigation(),
            "stop": lambda: self.stop(),
            "navigate_to": lambda: self.navigate_to(
                arguments.get("x"), arguments.get("y")
            ),
        }.get(name)

        if handler is None:
            self._audit.record(name, arguments, "refusé : outil inconnu")
            return f"Outil inconnu : {name}"

        return handler()
