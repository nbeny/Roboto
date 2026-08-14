# 0009 — L'API parle à ROS 2 par rosbridge, pas par un binding natif

**Statut** : acceptée — 2026-08-14

## Contexte

Le tableau de bord a besoin d'une API. Le brief la veut simple : TypeScript, Node.js,
REST et WebSocket, sans microservices. Restait à décider **comment cette API atteint
ROS 2**.

Deux voies existent.

**`rclnodejs`** est un client ROS 2 pour Node. Il parle directement au middleware, sans
processus intermédiaire. En contrepartie, il se compile nativement contre l'installation
ROS 2 locale et génère ses liaisons de messages à l'installation. L'API deviendrait donc
inconstruisible sans ROS 2 : plus de `npm install` sur une machine de développement, plus
de tests sans robot, et une intégration continue qui exige une image ROS complète pour
vérifier trois routes HTTP.

**`rosbridge_server`** expose ROS 2 en JSON sur WebSocket. Il ajoute un processus, mais
l'API redevient du TypeScript ordinaire.

## Décision

L'API parle à ROS 2 par **rosbridge**.

## Justification

C'est la [décision 0001](0001-coeur-rust-sans-ros2.md) appliquée à l'autre extrémité du
système. Le cœur Rust ne connaît pas ROS 2 pour que son risque d'intégration reste confiné
et qu'il se teste sans middleware. Le même raisonnement vaut pour l'API : ce qu'elle fait
— valider des entrées, mettre en cache un état, exposer des routes — n'a aucun besoin de
ROS 2 pour être vérifié.

L'effet le plus concret est sur les tests. Les 55 essais de l'API font tourner un **faux
pont rosbridge en mémoire**, une trentaine de lignes bâties sur `ws`. Ils s'exécutent en
trois secondes, sous Windows, sans ROS 2, sans simulateur et sans robot. Avec un binding
natif, chacun d'eux aurait exigé un environnement ROS complet.

## Conséquences

**Ce qu'on gagne.** `npm install && npm test` fonctionne partout. La chaîne d'intégration
vérifie l'API dans un job Node de quelques dizaines de secondes, sans construire d'image
ROS. Et le protocole rosbridge étant du JSON documenté, un défaut d'intégration se
diagnostique en lisant les trames.

**Ce qu'on paie.** Un processus de plus à lancer, et un saut de sérialisation
supplémentaire. Pour un tableau de bord qui rafraîchit quelques valeurs par seconde, le
coût est sans objet — ce serait un mauvais choix pour un flux de nuage de points à 30 Hz,
mais ce n'est pas ce que fait cette API.

**Une dépendance de plus au démarrage.** `autonomy.launch.py` lance `rosbridge_server`,
derrière un argument `rosbridge` qui vaut `true` par défaut. La simulation et la
navigation fonctionnent sans lui : seul le tableau de bord en dépend.

## Ce que cela ne change pas

L'API ne contient **aucune logique robotique**. Elle valide ses entrées — c'est le travail
d'une frontière — puis transmet au cœur, qui décide. Le choix du transport ne déplace pas
cette limite d'un millimètre : `robot-safety` reste la seule chose qui borne une vitesse,
que la commande vienne d'une manette, de Nav2 ou d'un clic sur une carte.
