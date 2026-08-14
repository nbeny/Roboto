# 0003 — Aucun runtime asynchrone dans le cœur

**Statut** : acceptée — 2026-08-14

## Contexte

L'écosystème Rust robotique s'appuie largement sur Tokio : `r2r` est async-natif,
`tokio-serial` est la référence pour la liaison série, et les clients HTTP le sont tous.
La tentation est de rendre le cœur `async` par cohérence.

## Décision

`robot-types`, `robot-safety`, `robot-core` et `robot-telemetry` sont **synchrones**.
Aucune fonction `async`, aucune dépendance à un exécuteur.

L'asynchrone vit exclusivement aux bords : `robot-ros2` (Milestone 2), `robot-mcu`
(Milestone 4) et le client d'API. Tokio y est le choix par défaut.

## Justification

Une boucle de contrôle temps-réel souple a besoin d'un pas de temps borné et prévisible.
Un ordonnanceur work-stealing lui apporte exactement l'inverse : de la variabilité de
latence, en échange d'un débit d'entrées-sorties dont le cœur n'a aucun usage. Le cœur ne
fait ni I/O ni attente — il transforme un état et une commande en une consigne bornée.

S'ajoute un argument de conception : une bibliothèque `async` impose son runtime à tous
ses consommateurs. Un cœur synchrone s'appelle depuis un contexte Tokio, depuis un thread
dédié à priorité temps-réel, ou depuis un test — sans rien imposer.

## Conséquences

**Ce qu'on gagne.** Un cœur appelable depuis n'importe quel contexte d'exécution. Des
tests sans `#[tokio::test]` ni exécuteur à configurer. Une latence de cycle prévisible.
Et, combiné à [l'absence d'horloge ambiante](0002-pas-d-horloge-ambiante.md), un cœur
compatible `no_std` à terme, donc partageable avec le firmware du Pico 2 si l'on passe à
Rust embarqué.

**Ce qu'on paie.** L'adaptateur ROS 2 devra faire le pont entre un monde async et un cœur
synchrone : recevoir les messages en tâches Tokio, puis appeler `tick` depuis un point
unique. C'est un travail d'adaptation classique, et il rend explicite le point de
sérialisation de l'accès au cœur — ce qui est un bien, pas un mal.
