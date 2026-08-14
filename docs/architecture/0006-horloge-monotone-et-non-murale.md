# 0006 — La boucle de contrôle se cadence sur une horloge monotone

**Statut** : acceptée — 2026-08-14

## Contexte

L'adaptateur ROS 2 horodate ses commandes et ses cycles de contrôle avec un instant
fourni au cœur. Le choix naturel semblait être `node.get_clock()` : c'est l'horloge du
nœud, celle que ROS 2 bascule automatiquement sur le temps simulé quand `use_sim_time`
est actif.

À l'exécution, le robot basculait en `SAFE_STOP` sans raison apparente, à des moments
variables. [`MonotonicClock`](0002-pas-d-horloge-ambiante.md) signalait un recul de
l'horloge source.

Deux hypothèses ont été formulées puis **réfutées par la mesure** :

1. *Une conversion fautive fabriquait un zéro.* Faux : la valeur brute journalisée était
   un temps epoch parfaitement valide.
2. *Plusieurs instances de `Clock` clonées ne partageaient pas leur source.* Faux : le
   passage à une instance unique partagée par `Arc` n'a rien changé.

La troisième mesure a tranché. Dans l'image `docker/ros2/`, sur 20 secondes et
57,7 millions d'échantillons :

| Horloge | Reculs observés | Amplitude maximale |
|---|---|---|
| murale (`time.time`) | **1** | **391 ms** |
| monotone (`time.monotonic`) | 0 | — |

Hors simulation, le temps ROS est adossé à l'horloge murale. Celle-ci recule réellement,
d'une amplitude de plusieurs centaines de millisecondes, lorsque la machine virtuelle se
resynchronise sur son hôte. Docker Desktop et WSL2 y sont sujets, mais le phénomène n'a
rien de spécifique : NTP produit le même effet sur une machine réelle.

Un recul de 391 ms représente les quatre cinquièmes du timeout de commande. Le cœur
n'avait pas tort de s'arrêter — c'est la source de temps qui ne convenait pas.

## Décision

L'adaptateur se cadence sur `rclrs::Clock::steady()`, une horloge monotone, et non sur
l'horloge du nœud.

## Conséquences

**Ce qu'on gagne.** Plus aucun `SAFE_STOP` fantôme. Le test de bout en bout, auparavant
intermittent, passe désormais de façon reproductible.

**Le temps simulé — réglé en Milestone 3.** Le nœud choisit désormais sa source selon le
paramètre standard `use_sim_time` : horloge monotone quand il vaut faux, horloge du nœud
quand il vaut vrai. Dans ce second cas, c'est `rclrs` qui fait le travail — son
`TimeSource` interne s'abonne à `/clock` et pilote l'horloge du nœud, ce qui a rendu
inutile l'abonnement maison écrit d'abord, puis supprimé.

Un détail à connaître : `use_sim_time` est **déjà déclaré** par `rclrs`. Le déclarer une
seconde fois échoue. Le nœud le lit donc via `use_undeclared_parameters()`. La première
version déclarait le paramètre et, en cas d'échec, retombait « proprement » sur l'horloge
monotone — si bien que la simulation tournait en temps mural sans que rien ne le signale.
Un repli silencieux sur un mode dégradé est pire que l'échec qu'il prétend absorber.

Sous temps simulé, un recul reste une vraie discontinuité — une relance du simulateur —
et le signaler demeure le bon comportement.

**Ce qui n'était pas le correctif.** Une tolérance aux petits reculs a été implémentée
au cours de l'enquête, sur l'hypothèse d'une dérive NTP de l'ordre de la microseconde.
La mesure l'a invalidée : un saut de 391 ms dépasse de loin toute tolérance raisonnable,
et aucune valeur n'aurait à la fois absorbé ce recul et laissé passer le timeout de 500 ms.

La tolérance a néanmoins été conservée, pour deux raisons qui tiennent d'elles-mêmes :
`ClockReading::backward_step` mesure l'ampleur du recul — c'est cette mesure qui a permis
de trancher — et une source non monotone reste possible ailleurs, notamment le topic
`/clock` en Milestone 3. Sa valeur par défaut de 50 ms se situe délibérément entre le
recalage d'horloge et la relance de simulateur.

## Leçon

Sur ce sujet, deux hypothèses plausibles se sont révélées fausses avant que la mesure ne
donne la réponse. Il aurait été facile — et faux — de conclure à la première, ou de
masquer le symptôme par une tolérance suffisamment large.
