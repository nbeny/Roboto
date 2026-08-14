# API

REST et WebSocket entre le tableau de bord et le cœur du robot.

```
Next.js ──HTTP/WS──► API ──WebSocket JSON──► rosbridge ──► ROS 2 ──► robot_core
```

---

## Pourquoi rosbridge et non un binding natif

`rclnodejs` existe et parlerait directement à ROS 2. Il impose en revanche une
compilation native contre l'installation ROS 2 et une génération de messages : l'API
deviendrait inconstruisible sans ROS.

Avec rosbridge, l'API reste **du TypeScript pur**. Elle se construit et se teste sous
Windows, sans ROS 2, exactement comme le workspace Rust — voir
[ADR 0001](../docs/architecture/0001-coeur-rust-sans-ros2.md). C'est le même principe
appliqué à l'autre extrémité du système.

Conséquence directe : les tests font tourner un **faux pont rosbridge** en mémoire. Tout
le comportement de l'API est vérifiable sans robot, sans simulateur et sans ROS.

---

## L'API ne décide rien

Elle valide ses entrées — c'est le travail d'une frontière — puis transmet au cœur.

Aucune règle robotique n'est dupliquée ici. Si l'API décidait, elle le ferait sans les
garanties de `robot-safety`, et un jour les deux ne diraient plus la même chose. Les
codes de retour disent exactement cela :

| Code | Signification |
|---|---|
| `200` | lecture, ou commande exécutée et confirmée par le cœur |
| `202` | ordre **transmis** — le cœur reste libre de le refuser |
| `400` | entrée invalide, rejetée à la frontière |
| `409` | le cœur a refusé (par exemple : reprise pendant un arrêt d'urgence) |
| `502` | le cœur est injoignable |

---

## Routes

| Méthode | Route | Rôle |
|---|---|---|
| `GET` | `/api/health` | vérification de vie |
| `GET` | `/api/robot/status` | état, vitesse, batterie, mission |
| `GET` | `/api/robot/pose` | position dans le repère `map` |
| `GET` | `/api/robot/sensors` | résumé du LiDAR |
| `GET` | `/api/robot/map` | grille d'occupation ; `404` tant que la cartographie n'a rien publié |
| `POST` | `/api/robot/stop` | arrêt d'urgence logiciel |
| `POST` | `/api/robot/clear-emergency-stop` | lève le verrou d'arrêt d'urgence |
| `POST` | `/api/robot/resume` | quitte `SAFE_STOP` ; `409` tant que l'arrêt d'urgence tient |
| `POST` | `/api/robot/navigation` | but de navigation `{ x, y, theta? }` |
| `POST` | `/api/robot/state` | demande de transition `{ state }` |

### WebSocket `/ws`

Un client reçoit l'instantané complet **dès sa connexion** — un robot immobile ne publie
rien de nouveau pendant des minutes, et attendre le prochain changement laisserait un
écran vide.

Ensuite : `robot.state`, `robot.pose`, `robot.battery`, `robot.navigation`, et
`robot.alert` à l'entrée en `SAFE_STOP` ou `ERROR`.

---

## Deux choix qui méritent une explication

**La batterie s'annonce comme non instrumentée.** Le robot n'a aucune mesure de tension :
ni BMS instrumenté en simulation, ni matériel. Le type le dit
(`{ instrumented: false }`) plutôt que de rendre un pourcentage. La maquette d'origine
affichait « 87 % » ; l'afficher sans rien mesurer serait inventer une donnée de sécurité.

**Un but de navigation rend `202`, pas `200`.** Une mission dure des minutes. Suspendre
une requête HTTP jusqu'à l'arrivée n'apprendrait rien à personne : le but est *accepté*,
et la progression passe par le WebSocket.

---

## Commandes

```bash
npm install
npm test          # 55 essais, sans ROS
npm run typecheck
npm run build
npm start         # ROSBRIDGE_URL, API_HOST, API_PORT
```
