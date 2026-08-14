# 0008 — Le watchdog de commande n'escalade que si le robot roule

**Statut** : acceptée — 2026-08-14

## Contexte

Depuis la Milestone 1, la règle était simple : en état de mouvement, l'absence de commande
pendant `command_timeout` — 500 ms — force `SAFE_STOP`. C'est la protection contre la
perte du pilote, et elle est indispensable.

La première navigation autonome réelle a montré ce que cette règle produit une fois
branchée à Nav2.

Le robot part vers son but, l'atteint, Nav2 annonce `SUCCEEDED` et **cesse de publier** —
il n'a plus rien à commander. Cinq cents millisecondes plus tard, le watchdog du cœur
conclut à une perte du pilote et verrouille le robot en `SAFE_STOP`.

Observé en simulation : but à `x = 1,0`, robot arrêté à `x = 0,859` — dans la tolérance,
mission réussie — puis état `SAFE_STOP`.

Conséquence : **toute mission accomplie exige une intervention humaine avant la suivante.**
Un robot qui se verrouille chaque fois qu'il réussit n'est pas un robot autonome.

## Le raisonnement

La règle confondait deux situations que rien n'oblige à traiter pareillement.

**Le robot roule et son pilote se tait.** Dangereux, et c'est précisément la raison d'être
du watchdog. Personne ne contrôle plus une machine en mouvement : il faut l'arrêter, et
verrouiller cet arrêt jusqu'à ce qu'un humain regarde.

**Le robot est à l'arrêt et son pilote se tait.** Il est déjà dans l'état sûr. Le forcer en
`SAFE_STOP` n'ajoute aucune sécurité — il ne bougeait pas et ne bougera pas — mais coûte
une intervention humaine.

La distinction n'est pas un assouplissement de confort : c'est la reconnaissance que le
danger vient du mouvement, pas du silence.

## Décision

Le watchdog de commande escalade en `SAFE_STOP` **si et seulement si** le robot n'est pas
au repos. Il est considéré au repos quand **les deux** conditions tiennent :

- la dernière vitesse produite est stationnaire ;
- la dernière consigne reçue est stationnaire.

La seconde condition n'est pas redondante. Un robot qui vient de recevoir « avance à
0,3 m/s » mais n'a pas encore accéléré est immobile à l'instant présent, et pourtant sur
le point de partir sur une consigne périmée. Ce cas doit escalader.

Le constat reste émis dans les deux cas, sous deux formes distinctes :

| Constat | Signification | Escalade |
|---|---|---|
| `CommandTimeout` | flux tari alors que le robot roulait | `SAFE_STOP` |
| `CommandStreamIdle` | flux tari, robot déjà à l'arrêt | signalé seulement |

## Conséquences

**Ce qu'on gagne.** Les missions s'enchaînent. Nav2 peut rester silencieux entre deux
buts, la téléopération peut faire une pause, sans qu'il faille lever un verrou à chaque
fois.

**Ce qu'on préserve.** Le cas dangereux est inchangé. Un robot en mouvement dont le pilote
disparaît passe toujours en `SAFE_STOP`, et ce verrou se lève toujours explicitement. Le
test `a_command_gap_while_moving_still_forces_a_safe_stop` le vérifie, et il passait déjà
avant ce changement — le correctif n'a pas eu à toucher au cas dangereux.

**Ce qu'on ne perd pas non plus.** Les autres étages sont intacts. Le watchdog du
microcontrôleur, lui, coupe les moteurs à 200 ms sans se poser cette question : il n'a
aucun moyen de savoir si le robot roule, et son rôle est de réagir à une liaison morte.
L'arrêt d'urgence matériel coupe la puissance sans rien demander à personne.

**Vérifié automatiquement.** `scripts/nav-smoke-test.sh` enchaîne deux buts consécutifs
sans lever quoi que ce soit entre les deux. Si la règle redevenait trop grossière, le
second échouerait.

## Leçon

Cette règle a survécu à quatre milestones et à plus de deux cents tests unitaires. Aucun
ne pouvait la mettre en défaut : ils vérifiaient tous qu'un robot **en mouvement** privé de
commandes s'arrête, ce qui est vrai et le restera. Il a fallu brancher un vrai
planificateur, qui se tait quand il a fini, pour que le problème apparaisse.
