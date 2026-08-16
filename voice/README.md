# Voix

Commande vocale, avec un court-circuit d'arrêt.

```
parole ──► transcription ──► ordre d'arrêt ? ──oui──► POST /api/robot/stop
                                    │
                                   non
                                    ▼
                                 agent ──► réponse ──► voix
```

---

## Un ordre d'arrêt ne traverse jamais le modèle

C'est toute la conception de ce module.

La forme évidente — tout envoyer à l'agent, qui a déjà un outil `stop` — met un modèle de
langage sur le chemin de l'arrêt d'urgence. L'arrêt hériterait alors de sa latence, de ses
limites de débit, de ses pannes, et de la possibilité qu'il pose une question de
clarification.

**Quelqu'un qui crie « stop » ne veut pas d'une question de clarification.**

Le module qui décide n'importe rien : ni réseau, ni modèle, ni client HTTP. Un essai
vérifie qu'il le reste. Conséquence directe : sans clé API, sans réseau, quota dépassé —
la conversation devient impossible et l'arrêt reste intact.

Vérifié de bout en bout contre le robot en mouvement, dans un conteneur **sans aucune clé
Anthropic** :

```
robot > Arrêt demandé. Le robot est à l'arrêt de sécurité.
  OK  « arrête-toi » a arrêté le robot, sans aucun modèle de langage
```

---

## « ne t'arrête pas » arrête aussi

C'est délibéré, et l'asymétrie justifie tout :

| | Coût |
|---|---|
| Arrêt inutile | quelques secondes, et une reprise explicite en deux gestes |
| Arrêt manqué | potentiellement quelqu'un |

Analyser la négation pour éviter le premier reviendrait à accepter le risque du second.
Et c'est précisément le genre de traitement qui se trompe sur une transcription
bruitée — c'est-à-dire dans les conditions où l'on crie.

Le pendant est tout aussi nécessaire : une phrase ordinaire ne doit **pas** arrêter. Un
détecteur qui arrête à tout propos rend la commande vocale inutilisable, l'utilisateur la
désactive, et cela supprime aussi le vrai arrêt. D'où la correspondance sur des mots
entiers : « les astrophysiciens » n'arrête pas le robot.

[ADR 0012](../docs/architecture/0012-le-mot-d-arret-court-circuite-le-modele.md).

---

## Utilisation

```bash
python -m pytest                    # 40 essais, sans micro ni modèle

# Voir ce qui arrête et ce qui n'arrête pas
python -m roboto_voice --self-test

# Dicter au clavier : toute la chaîne, sauf la transcription
ROBOTO_API=http://127.0.0.1:8080 python -m roboto_voice --text

# Une seule phrase, pour les essais
python -m roboto_voice --say "arrête-toi" --quiet

# Depuis un fichier audio (installe Whisper au préalable)
pip install -e ".[whisper]"
python -m roboto_voice --audio enregistrement.wav
```

---

## Dépendances

**Aucune n'est obligatoire.** Le court-circuit d'arrêt est du Python standard, et doit le
rester : il ne peut pas dépendre de ce qui peut manquer.

| Extra | Pour quoi |
|---|---|
| `whisper` | transcription locale ; le modèle se télécharge au premier usage |
| `mic` | capture micro |
| `dev` | pytest |

La synthèse vocale utilise `espeak-ng` s'il est présent, sinon écrit à l'écran. Ne pas
pouvoir parler n'est pas un incident.

---

## Ce que cela ne remplace pas

L'arrêt vocal est un **arrêt logiciel**. Il traverse l'API, le cœur et `robot-safety` —
donc il est aussi fiable que cette chaîne, et pas davantage.

Le vrai arrêt d'urgence est un bouton champignon qui coupe la puissance par relais. Aucune
phrase ne s'y substitue. Voir [SECURITY.md](../SECURITY.md).
