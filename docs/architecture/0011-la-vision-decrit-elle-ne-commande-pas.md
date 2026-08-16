# 0011 — La vision décrit, elle ne commande pas

**Statut** : acceptée — 2026-08-16

## Contexte

La perception visuelle arrive dans un système où le chemin de commande est déjà fermé :
Nav2 publie sur `/nav/cmd_vel`, le cœur borne, `~/cmd_vel_safe` sort, et le robot simulé
n'écoute rien d'autre.

Un nœud de vision est une tentation naturelle d'ouvrir ce chemin. « Je vois un obstacle,
je freine » est une phrase qui semble raisonnable, et c'est exactement ainsi qu'apparaît
un second chemin vers les roues — un chemin qui ne passe ni par la table d'habilitation
du cœur, ni par la borne de vitesse, ni par le watchdog.

## Décision

**Le nœud de vision n'a aucun éditeur de commande.** Il s'abonne à la caméra, publie une
description de scène sur `/vision/scene`, et rien d'autre. Aucun `Twist`, aucun client de
service de mouvement.

C'est vérifié structurellement : l'essai de bout en bout échoue si le mot `cmd_vel` ou
`Twist` apparaît quelque part dans le paquet `robot_vision`.

## Justification

Si la vision devait freiner, elle le ferait sans les garanties de `robot-safety` : sans
limite d'accélération, sans watchdog, sans arrêt de sécurité verrouillé. On aurait deux
autorités de freinage aux règles différentes, et un jour elles ne diraient pas la même
chose — au pire moment.

Le freinage d'urgence sur obstacle a déjà son propriétaire : les couches de coût de Nav2,
alimentées par le LiDAR, et derrière elles `robot-safety`. Ajouter une troisième voie ne
rendrait pas le robot plus sûr, seulement plus difficile à raisonner.

## Ce que la vision apporte quand même

Une **identité**. Le LiDAR sait qu'il y a quelque chose à 1,2 m ; il ne sait pas que
c'est la station de charge. Les marqueurs ArUco donnent cette identité de façon exacte —
le marqueur 7 est le marqueur 7, il n'y a pas de « chien à 62 % » — et l'agent peut s'en
servir pour choisir une destination. Choisir, pas conduire.

## Pourquoi ArUco plutôt qu'un détecteur d'objets

Trois raisons, dans cet ordre.

**Il se vérifie hors ligne.** OpenCV sait produire les marqueurs qu'il sait lire : les
essais fabriquent leurs propres images, sans photo, sans poids de modèle, sans caméra.
Un détecteur généraliste aurait imposé un téléchargement de dizaines de mégaoctets pour
un essai déterministe — ou, plus vraisemblablement, l'absence d'essai.

**Il tient sur un Raspberry Pi 4.** Sans accélérateur, à côté de Nav2 et de SLAM.

**Il ne se trompe pas à moitié.** Le code correcteur du dictionnaire fait qu'un marqueur
est lu ou ne l'est pas. D'où une confiance de 1.0, plutôt qu'un score inventé pour
ressembler à un réseau de neurones.

Un détecteur d'objets se branche à côté par le protocole `Detector`, sans rien changer
au reste. Le socle vérifiable d'abord ; le reste s'ajoute.

## Conséquences

**Une couche de compatibilité OpenCV.** L'image ROS 2 Jazzy livre OpenCV 4.6, une machine
de développement récente 5.x, et l'API ArUco a été refondue en 4.7. `compat.py` réconcilie
les deux : les 26 essais de la bibliothèque passent sous les deux versions. Découvrir
l'incompatibilité sur le robot aurait coûté cher.

**La bibliothèque vit hors de ROS 2.** `vision/` est un paquet Python ordinaire, testable
sous Windows sans middleware ; `ros2/src/robot_vision/` n'est que le câblage. Même
partage que partout ailleurs — voir [ADR 0001](0001-coeur-rust-sans-ros2.md).

**La conversion d'image est faite à la main**, sans `cv_bridge`. Cinq lignes pour les
encodages qui nous concernent, contre une dépendance absente de l'image de simulation.
