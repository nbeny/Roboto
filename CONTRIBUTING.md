# Contribuer à Roboto

## Avant tout

Ce code fait bouger une machine physique. La barre de qualité est celle d'un système qui
peut blesser quelqu'un, pas celle d'une application web.

Lisez [`SECURITY.md`](SECURITY.md) avant de toucher à `robot-safety` ou `robot-core`.

---

## Environnement

Le cœur Rust se compile et se teste **sans ROS 2, sans Docker, sans WSL**, y compris sous
Windows. C'est une garantie vérifiée par la CI, pas un effet de bord : ne l'abîmez pas en
ajoutant une dépendance ROS 2 à `robot-types`, `robot-safety`, `robot-core` ou
`robot-telemetry`.

ROS 2 Jazzy, Gazebo Harmonic et Nav2 vivent dans WSL2 / Ubuntu 24.04, et ne concernent que
`robot-ros2` à partir de la Milestone 2.

---

## Les trois commandes

Elles doivent passer avant toute proposition de fusion :

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

---

## Développement piloté par les tests

Le projet est développé en TDD, et ce n'est pas une préférence esthétique : un test écrit
après coup passe immédiatement, ce qui ne prouve rien. On n'a jamais vu le test échouer,
donc on ne sait pas s'il teste quelque chose.

Le cycle :

1. **Rouge** — écrire le test. En Rust, la fonction visée existe avec la bonne signature
   et un corps `todo!()`.
2. **Vérifier le rouge** — lancer les tests. L'échec doit venir du `todo!()`, pas d'une
   faute de frappe.
3. **Vert** — écrire le code minimal qui fait passer le test.
4. **Refactorer** — nettoyer, en restant vert.

Un bug se corrige de la même façon : d'abord un test qui le reproduit et qui échoue.

### Ce que doit être un test

Un comportement par test. Un nom qui décrit ce comportement, pas la fonction appelée :
`a_command_timeout_drives_the_robot_to_safe_stop`, pas `test_watchdog`.

Aucun test ne doit dormir. Le cœur ne lit pas l'horloge — fabriquez le temps :

```rust
let output = robot.tick(Monotonic::from_millis(1_500));
```

Si un test a besoin de `sleep`, c'est le code qui a un problème, pas le test.

---

## Frontières à respecter

| Crate | Ne doit jamais dépendre de |
|---|---|
| `robot-types` | quoi que ce soit d'obligatoire |
| `robot-safety` | ROS 2, matériel, runtime async |
| `robot-core` | ROS 2, matériel, runtime async |
| `robot-telemetry` | ROS 2, matériel |

Toute consigne de vitesse sort de `robot-safety`. Si vous écrivez un chemin qui produit
une vitesse sans passer par `SafetyLayer::evaluate`, l'architecture est cassée.

L'ordre d'évaluation de `robot-safety` est **normatif** et documenté dans la crate. Le
modifier demande une justification écrite et un test de régression.

---

## Langages

- **Rust** — cœur robotique : état, contrôle, sécurité, capteurs, communication.
- **Python** — IA et ML, là où l'écosystème est franchement supérieur (PyTorch, OpenCV,
  Whisper, VLM). Ne réimplémentez pas en Rust une bibliothèque Python mature.
- **TypeScript / Next.js** — API et dashboard. Volontairement simples : pas de
  microservices, pas de file de messages, pas d'orchestrateur.
- **C / C++** — uniquement lorsqu'un composant ROS 2 ou matériel existant le justifie.
  Ne réimplémentez jamais une fonctionnalité ROS 2 complexe pour éviter quelques lignes
  de C++.

---

## Dépendances

Chaque nouvelle dépendance se justifie. `robot-types` n'en a aucune d'obligatoire, et
cela doit le rester.

---

## Commits et pull requests

Messages au format impératif, en anglais, avec un préfixe de type : `feat:`, `fix:`,
`docs:`, `refactor:`, `test:`, `ci:`.

Le corps explique **pourquoi**, pas quoi — le diff dit déjà quoi.

Une pull request devrait indiquer ce qui a été vérifié et comment. Si un point reste
non testé, dites-le explicitement plutôt que de le laisser deviner.

---

## Méthode

Une milestone à la fois : inspecter l'existant, proposer le design, identifier les
risques, implémenter, tester, documenter, faire valider — puis passer à la suivante.

Ne complexifiez pas l'architecture pour anticiper un problème qui n'existe pas encore.
