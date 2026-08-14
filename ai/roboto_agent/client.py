"""Client HTTP de l'API du robot.

Bibliothèque standard seulement. C'est le seul chemin de l'agent vers le robot : il n'a
ni client ROS 2, ni socket brute, ni accès au matériel. Ce que l'API refuse, l'agent ne
peut pas contourner.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from typing import Any

__all__ = ["RobotApiClient", "RobotApiError"]


class RobotApiError(RuntimeError):
    """L'API n'a pas pu répondre, ou a répondu autre chose que ce qui était attendu."""


class RobotApiClient:
    """Appelle les routes de l'API du robot.

    Les codes de retour de l'API sont préservés tels quels : un `409` signifie que le
    cœur a refusé, et c'est une information utile pour l'agent — pas une panne à
    masquer.
    """

    def __init__(self, base_url: str = "http://127.0.0.1:8080", timeout: float = 10.0) -> None:
        self._base = base_url.rstrip("/")
        self._timeout = timeout

    # --- Lecture ---------------------------------------------------------------------

    def status(self) -> dict[str, Any]:
        return self._request("GET", "/api/robot/status")

    def pose(self) -> dict[str, Any]:
        return self._request("GET", "/api/robot/pose")

    def sensors(self) -> dict[str, Any]:
        return self._request("GET", "/api/robot/sensors")

    # --- Action ----------------------------------------------------------------------

    def navigate(self, x: float, y: float) -> dict[str, Any]:
        return self._request("POST", "/api/robot/navigation", {"x": x, "y": y})

    def request_state(self, state: str) -> dict[str, Any]:
        return self._request("POST", "/api/robot/state", {"state": state})

    def stop(self) -> dict[str, Any]:
        return self._request("POST", "/api/robot/stop")

    def clear_emergency_stop(self) -> dict[str, Any]:
        return self._request("POST", "/api/robot/clear-emergency-stop")

    def resume(self) -> dict[str, Any]:
        return self._request("POST", "/api/robot/resume")

    # --- Transport -------------------------------------------------------------------

    def _request(self, method: str, path: str, body: Any | None = None) -> dict[str, Any]:
        data = None if body is None else json.dumps(body).encode("utf-8")
        headers = {} if data is None else {"Content-Type": "application/json"}

        request = urllib.request.Request(
            f"{self._base}{path}", data=data, headers=headers, method=method
        )

        try:
            with urllib.request.urlopen(request, timeout=self._timeout) as response:
                return self._decode(response.read(), response.status)
        except urllib.error.HTTPError as error:
            # Un refus du cœur arrive ici : il porte un corps JSON explicatif qu'il
            # serait dommage de jeter au profit d'un simple numéro.
            payload = self._decode(error.read(), error.code)
            payload.setdefault("accepted", False)
            payload["status"] = error.code
            return payload
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            raise RobotApiError(f"API injoignable sur {self._base} : {error}") from error

    @staticmethod
    def _decode(raw: bytes, status: int) -> dict[str, Any]:
        if not raw:
            return {"status": status}
        try:
            parsed = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RobotApiError(f"réponse illisible de l'API : {error}") from error

        if not isinstance(parsed, dict):
            raise RobotApiError("réponse inattendue de l'API : objet JSON attendu")

        return parsed
