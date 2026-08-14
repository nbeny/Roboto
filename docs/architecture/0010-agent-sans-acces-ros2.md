# 0010 — L'agent IA n'atteint le robot que par l'API HTTP

**Statut** : acceptée — 2026-08-14

## Contexte

Le brief est catégorique : *« L'IA ne doit jamais pouvoir contourner la couche de
sécurité Rust »*, et *« Jamais : AI → moteurs »*. Restait à décider **comment** rendre
cela vrai par construction plutôt que par discipline.

Un agent Python aurait pu être un nœud ROS 2. C'est la solution évidente : il aurait vu
tous les topics, appelé tous les services, et publié directement sur `/cmd_vel`.

C'est précisément le problème. Un agent qui *peut* publier une vitesse finira par le
faire — parce qu'un modèle de langage produit du texte plausible, pas des garanties, et
qu'il suffit d'un outil mal nommé, d'une bibliothèque un peu trop serviable ou d'une
injection dans le message d'un utilisateur pour que ce chemin s'ouvre.

## Décision

**L'agent ne parle qu'à l'API HTTP.** Il n'a pas de client ROS 2, pas de dépendance à
`rclpy`, aucun moyen d'atteindre le middleware.

Son catalogue d'outils ne contient **aucun paramètre de vitesse**. Il dit *où* aller ;
la trajectoire est calculée par Nav2 puis bornée par `robot-safety`.

## Justification

La propriété devient **structurelle et vérifiable**, pas seulement documentée.

`CommandSource::Ai` est refusée dans les huit états du cœur depuis la Milestone 1. Cette
décision lui donne son sens : ce n'est pas une règle qu'un agent bien élevé respecte,
c'est une porte qu'il n'a aucun moyen d'atteindre. Même une commande `Ai` fabriquée
serait rejetée par `robot-core` — mais l'agent ne peut pas même en former une.

Trois couches se superposent, et chacune suffirait :

| Couche | Ce qu'elle empêche |
|---|---|
| Catalogue d'outils | Aucun outil ne prend une vitesse en entrée — vérifié par un essai structurel |
| Zone d'évolution | Un but hallucinaté est refusé **avant** tout appel réseau |
| `robot-core` | `CommandSource::Ai` n'est habilitée dans aucun état |

Le second point mérite d'être souligné. Un modèle peut produire « va au point 500, 500 »
avec une confiance parfaite. La zone d'évolution transforme cette hallucination en refus
motivé, que l'agent relit et corrige — au lieu d'un trajet de cinq cents mètres.

## Conséquences

**Ce qu'on gagne.** La couche de sécurité de l'agent n'a **aucune dépendance** : ni le
SDK Anthropic, ni le client HTTP, ni ROS 2. Elle se lit d'un trait et se vérifie en
quelques millisecondes, sans réseau, sans robot et sans modèle. Ce qui borne un modèle
de langage doit pouvoir être vérifié sans lui.

**Ce qu'on paie.** L'agent est limité à ce que l'API expose. Il ne peut pas lire un
topic arbitraire pour diagnostiquer une panne, ni appeler un service que l'API n'a pas
prévu. Élargir ses capacités demande d'élargir l'API — et c'est voulu : chaque
élargissement passe par une frontière qui valide, et non par un accès direct au
middleware.

**Un mode de vérification sans modèle.** `roboto-agent --self-test --drive-test` exerce
les outils directement, sans clé API. Le jugement d'un modèle ne se teste pas de façon
déterministe ; la plomberie qui l'entoure, si.

## Ce que cela ne dit pas

Que l'agent est sûr. Il ne l'est pas plus que le modèle qui le conduit. Ce que cette
décision garantit est plus étroit et plus solide : **quoi que le modèle décide, il ne
peut pas contourner `robot-safety`**, parce qu'il n'existe aucun chemin de code qui le
lui permettrait.

L'arrêt d'urgence **matériel** reste hors de portée de tout logiciel, agent compris.
Voir [SECURITY.md](../../SECURITY.md).
