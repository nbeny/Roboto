# 0007 — Les limites de Nav2 restent à l'intérieur de l'enveloppe de sécurité

**Statut** : acceptée — 2026-08-14

## Contexte

Nav2 et `robot-safety` bornent tous deux la vitesse du robot. La question n'est donc pas
*s'il faut* des limites des deux côtés — la couche de sécurité est non négociable — mais
**quelle relation** leurs valeurs doivent entretenir.

Le réflexe naturel est de leur donner les mêmes valeurs : une seule vérité, pas de
duplication. C'est une erreur, et elle se paie de façon déroutante.

Nav2 planifie en simulant des trajectoires, sous l'hypothèse que le robot exécute ce
qu'il commande. Si la couche de sécurité venait rogner ses consignes — ne serait-ce qu'un
peu, à cause d'un arrondi ou d'une différence de convention — le modèle interne de Nav2
deviendrait faux. Il commanderait une trajectoire, constaterait que le robot ne l'a pas
suivie, corrigerait, et recommencerait. Le symptôme observé serait une navigation qui
oscille et qui rate ses virages.

Et le diagnostic serait faux. On conclurait que « Nav2 suit mal », on passerait des heures
à retoucher les critiques de DWB, alors que le problème serait un désaccord d'un
centième entre deux fichiers de configuration.

## Décision

Les limites de Nav2 sont **strictement inférieures** à celles de `robot-safety`, avec une
marge d'au moins 20 %.

| | Nav2 | `robot-safety` | Marge |
|---|---|---|---|
| Vitesse linéaire | 0,35 m/s | 0,50 m/s | 30 % |
| Vitesse angulaire | 0,80 rad/s | 1,00 rad/s | 20 % |
| Accélération linéaire | 0,40 m/s² | 0,50 m/s² | 20 % |
| Accélération angulaire | 1,20 rad/s² | 1,50 rad/s² | 20 % |

En fonctionnement nominal, la couche de sécurité ne borne donc **jamais** une consigne de
Nav2. Elle reste entièrement passive sur ce chemin — ce qui est exactement le rôle d'un
filet : ne rien faire, jusqu'au jour où il rattrape quelque chose.

## Conséquences

**Ce qu'on gagne.** Nav2 travaille sur un modèle exact du robot. Et la couche de sécurité
retrouve son vrai rôle : si elle se met à borner, c'est qu'il se passe quelque chose
d'anormal — un bug de configuration, un composant qui déraille, une pile de navigation
compromise. Une saturation devient un signal, au lieu d'être le régime normal.

**Ce qu'on paie.** Le robot n'exploite pas 100 % de son enveloppe en navigation
autonome. À 0,35 m/s au lieu de 0,5, c'est 30 % de vitesse de pointe abandonnée. Sur un
robot d'intérieur qui passe l'essentiel de son temps à contourner des obstacles, le coût
réel est proche de zéro.

**La règle à ne pas inverser.** Relever les limites de `robot-safety` sans toucher à
celles de Nav2 est sans danger. L'inverse ne l'est pas. Toute modification des limites de
Nav2 doit vérifier que la marge tient.

**Vérifié automatiquement.** `scripts/nav-smoke-test.sh` mesure la vitesse maximale
observée sur `~/cmd_vel_safe` pendant une navigation complète et échoue si elle dépasse la
limite de Nav2. Une marge qui disparaît est donc détectée par la chaîne d'intégration, et
non des semaines plus tard sur un robot qui zigzague.

## Portée

Le même raisonnement s'appliquera au microcontrôleur en Milestone 5. Sa boucle
d'asservissement borne le rapport cyclique, et son watchdog se déclenche à 200 ms contre
500 ms pour celui du cœur. La règle générale est la même à chaque étage :

> Chaque couche agit avant celle qui l'englobe, de sorte que la couche englobante reste
> passive tant que rien ne va mal.
