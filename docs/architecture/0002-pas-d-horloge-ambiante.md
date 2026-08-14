# 0002 — Aucune horloge ambiante dans le cœur

**Statut** : acceptée — 2026-08-14

## Contexte

Le cœur doit gérer un watchdog de commande, une durée de démarrage et un limiteur
d'accélération. Trois mécanismes qui dépendent du temps.

L'approche naturelle consiste à lire `Instant::now()` à l'intérieur. Elle a trois défauts,
et le troisième est rédhibitoire :

1. Tester un timeout de 500 ms impose de dormir 500 ms. Une suite de tests qui couvre
   sérieusement les cas limites devient longue, donc on la lance moins souvent.
2. Ces tests deviennent sensibles à la charge de la machine — le genre d'échec
   intermittent qu'on finit par ignorer, jusqu'au jour où il signalait un vrai bug.
3. Sous Gazebo, le temps de simulation n'est pas le temps mur. Un cœur qui lit l'horloge
   système ne peut pas fonctionner correctement en simulation, ce qui ruine l'objectif
   d'exécuter le même code métier en simulation et sur le robot.

## Décision

Le cœur n'appelle jamais `Instant::now()`. Le temps est un paramètre d'entrée :

```rust
robot.submit_command(command, now);
let output = robot.tick(now);
```

Le type `Monotonic` est un newtype sur `Duration` depuis le démarrage. Toutes les
soustractions sont saturantes : un temps qui recule renvoie zéro plutôt que de paniquer.

## Conséquences

**Ce qu'on gagne.** Les tests de watchdog s'exécutent en microsecondes et sont
parfaitement déterministes. La suite complète tourne en moins d'une seconde, donc elle
tourne à chaque sauvegarde. Le même cœur accepte indifféremment le temps simulé de
Gazebo, le temps monotone du robot réel et le temps fabriqué des tests. Enfin, l'absence
d'appel système rend le cœur compatible `no_std` à terme, donc partageable avec le
firmware du microcontrôleur.

**Ce qu'on paie.** Chaque méthode publique porte un paramètre `now`, ce qui est un peu
plus verbeux. L'appelant devient responsable de fournir un temps cohérent — c'est le rôle
de `robot-ros2` en Milestone 2. Un appelant qui fournirait un temps figé gèlerait le
watchdog ; les soustractions saturantes garantissent au moins qu'aucune panique n'en
résulte.

**Effet de bord notable.** Le premier `tick` établit la référence de démarrage. Une
construction suivie d'un unique `tick` très tardif ne fait donc pas passer le robot en
`IDLE` — comportement documenté, et vérifié par la doctest de `robot-core`.
