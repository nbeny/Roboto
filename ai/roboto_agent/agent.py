"""La boucle conversationnelle.

Volontairement écrite à la main plutôt que confiée au *tool runner* du SDK, pour deux
raisons précises :

1. **Ce qui est vérifié est ce qui est envoyé.** La propriété qui porte ce milestone —
   aucun outil ne prend une vitesse en entrée — est testée sur `TOOL_SCHEMAS`, et c'est
   `TOOL_SCHEMAS` qui part dans la requête. Faire dériver les schémas de signatures
   Python décalerait l'artefact testé de l'artefact expédié.
2. **L'exécuteur porte un état** — client HTTP, zone d'évolution, journal d'audit. Le
   décorateur du SDK attend des fonctions de module, ce qui obligerait à passer par une
   variable globale.

La boucle tient en une trentaine de lignes et c'est une frontière de sécurité : elle
mérite d'être lisible d'un seul tenant.
"""

from __future__ import annotations

from typing import Any

from .tools import TOOL_SCHEMAS, ToolExecutor

__all__ = ["MAX_TOOL_ROUNDS", "MODEL", "SYSTEM_PROMPT", "RobotAgent"]

MODEL = "claude-opus-5"

# Un modèle peut s'enfermer dans une suite d'appels d'outils. Un agent qui pilote un
# robot ne doit pas pouvoir tourner sans fin sans que personne ne le sache.
MAX_TOOL_ROUNDS = 12

SYSTEM_PROMPT = """\
Tu pilotes Roboto, un robot mobile réel, par l'intermédiaire d'outils de haut niveau.

# Ce que tu peux et ne peux pas faire

Tu n'as aucun accès direct aux moteurs. Tu dis où aller ; la trajectoire est calculée \
par la pile de navigation, puis bornée par une couche de sécurité écrite en Rust qui a \
toujours le dernier mot. Tu ne peux pas la contourner, et tu ne dois pas essayer.

N'invente jamais une coordonnée. Si tu ne sais pas où se trouve un endroit, lis la \
position du robot ou demande à l'utilisateur. Une destination plausible mais fausse \
envoie une machine de plusieurs kilos dans un mur.

# Sécurité

En cas de doute, arrête. La reprise est explicite et sans conséquence ; la poursuite ne \
l'est pas. Arrête aussi dès que l'utilisateur exprime une inquiétude, même vague, et \
sans attendre qu'il le demande formellement.

Si un outil te refuse quelque chose, dis-le à l'utilisateur, avec le motif. Ne réessaie \
pas la même chose en espérant un autre résultat, et ne présente jamais comme fait ce \
qui a échoué.

# Ton

Réponds brièvement. L'utilisateur surveille un robot, pas une conversation : donne le \
résultat d'abord, le détail ensuite s'il éclaire une décision. Une phrase suffit \
souvent.

Fais ce qui est demandé, à la portée demandée. Si tu penses qu'une autre approche \
vaudrait mieux, dis-le en une phrase et fais quand même ce qui a été demandé.\
"""


class RobotAgent:
    """Conduit la conversation et exécute les outils que le modèle demande."""

    def __init__(
        self,
        client: Any,
        executor: ToolExecutor,
        *,
        model: str = MODEL,
        system: str = SYSTEM_PROMPT,
        max_tokens: int = 8192,
    ) -> None:
        self._client = client
        self._executor = executor
        self._model = model
        self._system = system
        self._max_tokens = max_tokens
        self._messages: list[dict[str, Any]] = []

    @property
    def history(self) -> list[dict[str, Any]]:
        return list(self._messages)

    def send(self, message: str) -> str:
        """Traite un tour complet, outils compris, et rend la réponse finale."""
        self._messages.append({"role": "user", "content": message})

        for _ in range(MAX_TOOL_ROUNDS):
            response = self._client.messages.create(
                model=self._model,
                max_tokens=self._max_tokens,
                system=self._system,
                thinking={"type": "adaptive"},
                output_config={"effort": "high"},
                tools=TOOL_SCHEMAS,
                # Un instantané, pas la liste vivante : la suite de la boucle y ajoute
                # des messages, et une requête ne doit pas refléter ce qui lui est
                # postérieur.
                messages=list(self._messages),
            )

            # Les classificateurs peuvent décliner : le contenu est vide ou partiel, et
            # le lire sans vérifier d'abord planterait sur une liste vide.
            if response.stop_reason == "refusal":
                category = getattr(response.stop_details, "category", None)
                self._messages.pop()
                return (
                    "Demande refusée par les garde-fous du modèle"
                    + (f" (catégorie : {category})" if category else "")
                    + "."
                )

            self._messages.append(
                {"role": "assistant", "content": self._as_params(response.content)}
            )

            if response.stop_reason == "pause_turn":
                # Un outil côté serveur a atteint sa limite d'itérations ; renvoyer la
                # conversation telle quelle la reprend là où elle s'est arrêtée.
                continue

            if response.stop_reason != "tool_use":
                return self._text_of(response.content)

            # Un `tool_result` par `tool_use`, tous dans un seul message : les séparer
            # apprend au modèle à ne plus demander d'appels groupés.
            results = [
                {
                    "type": "tool_result",
                    "tool_use_id": block.id,
                    "content": self._executor.dispatch(block.name, dict(block.input or {})),
                }
                for block in response.content
                if getattr(block, "type", None) == "tool_use"
            ]
            self._messages.append({"role": "user", "content": results})

        return (
            "Conversation interrompue : le modèle a enchaîné trop d'appels d'outils "
            "sans conclure. Le robot n'a pas été laissé en mouvement par cette boucle, "
            "mais vérifie son état."
        )

    @staticmethod
    def _text_of(content: Any) -> str:
        parts = [
            block.text
            for block in content
            if getattr(block, "type", None) == "text" and block.text
        ]
        return "\n".join(parts).strip() or "(pas de réponse)"

    @staticmethod
    def _as_params(content: Any) -> list[dict[str, Any]]:
        """Convertit les blocs de réponse en blocs de requête pour le tour suivant."""
        params: list[dict[str, Any]] = []
        for block in content:
            kind = getattr(block, "type", None)
            if kind == "text":
                params.append({"type": "text", "text": block.text})
            elif kind == "tool_use":
                params.append(
                    {
                        "type": "tool_use",
                        "id": block.id,
                        "name": block.name,
                        "input": block.input,
                    }
                )
            elif kind == "thinking":
                # Renvoyé tel quel, signature comprise : l'API rejette un bloc de
                # réflexion modifié.
                params.append(
                    {
                        "type": "thinking",
                        "thinking": block.thinking,
                        "signature": block.signature,
                    }
                )
            elif kind == "redacted_thinking":
                params.append({"type": "redacted_thinking", "data": block.data})
        return params
