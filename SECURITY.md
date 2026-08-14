# Sécurité

Ce dépôt pilote une machine physique. « Sécurité » y recouvre deux choses distinctes qu'il
ne faut pas confondre : la **sûreté de fonctionnement** (le robot ne blesse personne et ne
se détruit pas) et la **sécurité informatique** (personne ne prend le contrôle du robot).

---

## 1. Sûreté de fonctionnement

### L'arrêt d'urgence matériel n'est pas négociable

Le bouton champignon **coupe physiquement la puissance moteur par relais**. Il ne passe
pas par le logiciel, ne dépend d'aucun processus, et fonctionne même si le Raspberry Pi
est planté ou si le firmware du microcontrôleur est bloqué.

Le `SAFE_STOP` logiciel de `robot-core` en est le **complément, jamais le substitut**. Il
garantit que l'état interne reflète la réalité et que rien ne redémarre tout seul.

Ne mettez jamais un robot sous tension sans arrêt d'urgence matériel fonctionnel.
Testez-le avant chaque session, pas une fois pour toutes.

### Invariants tenus par le code

Ces propriétés sont couvertes par des tests. Toute modification qui les casse doit être
rejetée, quelle qu'en soit la justification :

- aucune consigne ne parvient au matériel sans avoir traversé `robot-safety` ;
- une vitesse `NaN` ou infinie ne se propage jamais jusqu'à la sortie ;
- l'absence de commande pendant `command_timeout` en état de mouvement force `SAFE_STOP` ;
- l'arrêt d'urgence produit une vitesse nulle immédiate depuis n'importe quel état, y
  compris à l'arrêt ;
- `SAFE_STOP` ne se quitte que par une action explicite, et jamais tant que l'arrêt
  d'urgence est engagé ;
- `CommandSource::Ai` n'est habilitée dans **aucun** état : l'IA exprime des objectifs,
  jamais des vitesses brutes.

### Limites par défaut

Les valeurs par défaut de `SafetyLimits` sont délibérément conservatrices : 0,5 m/s,
1,0 rad/s, 0,5 m/s², 1,5 rad/s², timeout 500 ms. Les relever est une décision consciente,
à prendre **après validation en simulation**, jamais pour faire passer un test.

### Batteries lithium

Le pack Li-ion doit avoir un BMS. Ne chargez pas sans surveillance, ne chargez pas un pack
gonflé ou endommagé, et prévoyez un fusible côté batterie avant tout le reste du câblage.

---

## 2. Sécurité informatique

### Périmètre actuel

Au stade de la Milestone 1, le projet est une bibliothèque Rust sans surface réseau. La
surface d'attaque apparaîtra avec :

- l'adaptateur ROS 2 (M2) — DDS écoute sur le réseau local, **sans authentification par
  défaut** ;
- l'API TypeScript et le dashboard (M7) ;
- l'agent IA et ses outils (M10).

### Recommandations dès maintenant

- N'exposez jamais le domaine DDS d'un robot sur un réseau non maîtrisé. Utilisez un
  réseau dédié ou un VPN. `ROS_DOMAIN_ID` n'est pas une mesure de sécurité.
- Considérez toute entrée réseau comme hostile : l'API transmet des commandes, elle ne
  décide de rien. La validation fait autorité côté `robot-core`.
- Quand l'agent IA arrivera, traitez ses sorties comme des entrées non fiables. Il ne doit
  disposer que d'outils de haut niveau, dont chaque effet reste borné par la couche de
  sécurité.

### Signaler une vulnérabilité

Ouvrez un **security advisory privé** via l'onglet Security de GitHub, ou contactez le
mainteneur directement. N'ouvrez pas d'issue publique pour une faille exploitable.

Merci d'inclure : la version ou le commit concerné, les étapes de reproduction, et l'impact
que vous estimez — en particulier si la faille permet un mouvement non commandé.
