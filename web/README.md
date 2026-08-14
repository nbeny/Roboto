# Tableau de bord

Poste de conduite du robot : carte, télémétrie, arrêt.

```bash
npm install
npm run dev       # http://localhost:3000
```

L'API est attendue sur `http://127.0.0.1:8080`, réglable par `NEXT_PUBLIC_API_BASE`.

---

## Le parti pris visuel

**Instrumentation industrielle, pas tableau de bord d'application.**

Le contexte l'impose : un opérateur surveille une machine qui peut blesser quelqu'un. Il
doit lire l'état d'un coup d'œil et trouver l'arrêt sans réfléchir. D'où :

- **fond sombre à fort contraste**, typographie technique (IBM Plex Mono et Condensed —
  une famille dessinée pour l'industrie, chiffres tabulaires lisibles) ;
- **l'accent change de couleur avec l'état** : la couleur dit l'état avant que le texte ne
  soit lu. Bleu en `IDLE`, ambre en téléopération, vert en navigation, rouge en
  `SAFE_STOP` ;
- **l'arrêt est traité comme un vrai champignon** : relief marqué, enfoncement au clic,
  taille disproportionnée par rapport au reste ;
- **animation rare**. Après une heure de surveillance, tout ce qui bouge sans raison
  devient du bruit.

---

## Ce que l'écran refuse d'afficher

La maquette d'origine montrait « Battery 87 % ». Le robot n'a **aucune mesure de
tension** — ni BMS instrumenté en simulation, ni matériel. Le tableau de bord affiche
donc `non instrumentée`, en italique et en gris.

Un opérateur qui lit « 87 % » sur un écran suppose qu'un capteur l'a mesuré. Sur un robot,
cette supposition finit par vider une batterie en pleine mission.

La même règle s'applique partout : ce qui n'est pas mesuré s'affiche comme tel.

---

## Interactions

- **Clic sur la carte** : envoie un but de navigation. C'est le seul geste qui met le
  robot en mouvement, et il traverse la même chaîne que tout le reste — Nav2, puis
  `robot-safety`, qui garde le dernier mot.
- **Arrêt** : appelle le service d'arrêt d'urgence du cœur.
- **Lever l'arrêt de sécurité** : actif seulement en `SAFE_STOP`, parce que la reprise est
  toujours explicite.

L'arrêt d'urgence **logiciel** n'est pas celui du robot. Celui-là est un bouton champignon
qui coupe la puissance par relais, et aucun écran ne s'y substitue. Voir
[SECURITY.md](../SECURITY.md).
