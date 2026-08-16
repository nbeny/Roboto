# 0012 — Un ordre d'arrêt vocal ne traverse jamais le modèle de langage

**Statut** : acceptée — 2026-08-16

## Contexte

La commande vocale a une forme évidente : on transcrit la parole, on envoie le texte à
l'agent, l'agent décide et appelle ses outils. L'agent a déjà un outil `stop`, et il est
instruit d'arrêter au moindre doute.

Cette forme évidente met un modèle de langage sur le chemin de l'arrêt d'urgence.

Ce que cela implique concrètement : l'arrêt hérite de la latence du modèle — une à
plusieurs secondes —, de ses limites de débit, de ses pannes de service, et de la
possibilité qu'il décide de poser une question de clarification plutôt que d'agir.

Quelqu'un qui crie « stop » ne veut pas d'une question de clarification.

## Décision

La transcription passe d'abord par une **reconnaissance de mots d'arrêt** :

```
parole ──► transcription ──► ordre d'arrêt ? ──oui──► POST /api/robot/stop
                                    │
                                   non
                                    ▼
                                 agent ──► réponse ──► voix
```

Le module qui décide n'importe **rien** : ni réseau, ni modèle, ni client HTTP. C'est une
fonction pure sur une chaîne de caractères, et un essai vérifie qu'elle le reste.

## Justification

**« stop » doit fonctionner quand le reste ne fonctionne plus.** Sans clé API, sans
réseau, avec un quota dépassé, la conversation devient impossible — et l'arrêt reste
intact. Une commande vocale qui ne sait plus arrêter serait pire qu'une absence de
commande vocale, parce que l'opérateur croit avoir un moyen d'arrêter.

## Le choix qui surprend : « ne t'arrête pas » arrête

La reconnaissance porte sur des mots, sans analyse de la négation. « ne t'arrête pas »
contient « arrête », et déclenche donc l'arrêt.

C'est délibéré, et l'asymétrie justifie tout :

| | Coût |
|---|---|
| Arrêt inutile | quelques secondes, et une reprise explicite en deux gestes |
| Arrêt manqué | potentiellement quelqu'un |

Analyser la négation pour éviter le premier reviendrait à accepter le risque du second,
contre un gain sans commune mesure. Et une analyse de négation est précisément le genre
de traitement qui se trompe sur une transcription bruitée — c'est-à-dire dans les
conditions où l'on crie.

Le pendant est tout aussi nécessaire : une phrase ordinaire ne doit **pas** arrêter. Un
détecteur qui arrête à tout propos rend la commande vocale inutilisable, l'utilisateur
finit par la désactiver, et cela supprime aussi le vrai arrêt. D'où la correspondance sur
des mots entiers : « les astrophysiciens » n'arrête pas le robot.

## Conséquences

**L'arrêt vocal reste un arrêt logiciel.** Il traverse l'API, le cœur et
`robot-safety` — donc il est aussi fiable que cette chaîne, et pas davantage. Il ne
remplace pas le bouton champignon qui coupe la puissance par relais. Voir
[SECURITY.md](../../SECURITY.md).

**Whisper est optionnel.** La transcription est un protocole ; l'implémentation Whisper
charge son modèle paresseusement. C'est le cas où Python s'impose sans discussion — le
réimplémenter en Rust pour respecter une préférence de langage serait absurde — mais rien
de tout cela ne conditionne le court-circuit.

**Un échec d'arrêt est annoncé en majuscules**, avec la consigne d'utiliser l'arrêt
d'urgence matériel. Il n'est jamais fondu dans un message générique.
