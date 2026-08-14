# Protocole série Rust ↔ microcontrôleur

**Version du protocole : 1**
**Statut** : implémenté dans [`crates/robot-mcu`](../../crates/robot-mcu), non encore
éprouvé sur matériel.

Liaison entre le calculateur embarqué (Raspberry Pi 4) et le microcontrôleur
(Raspberry Pi Pico 2), sur USB CDC.

---

## Principes

**Le microcontrôleur ne connaît pas le robot.** Il reçoit des consignes de vitesse *par
roue*, en rad/s, et rend des mesures *par roue*. La cinématique — passer d'une vitesse de
châssis à des vitesses de roues — vit côté calculateur, dans `robot-hal`, où le rayon de
roue et l'empattement sont déjà connus. Dupliquer ces constantes dans le firmware
garantirait qu'un jour les deux ne concordent plus.

**Le protocole est versionné et le contrôle est strict.** Un octet de version ouvre chaque
trame. Un décodeur qui rencontre une version inconnue refuse la trame et le dit ; il ne
tente pas de deviner. Une incompatibilité silencieuse entre un firmware et un calculateur
mis à jour séparément est exactement le genre de panne qu'on ne diagnostique pas.

**Un seul code pour les deux extrémités.** `robot-mcu` est une crate `no_std` : le même
encodeur et le même décodeur tournent sur le calculateur et dans le firmware. Un protocole
écrit deux fois finit toujours par diverger.

**Le microcontrôleur a son propre watchdog.** Il est indépendant de celui du cœur Rust, et
c'est volontaire : si le calculateur se fige, plante, ou si le câble USB se débranche, les
moteurs doivent s'arrêter sans que personne n'ait à le demander.

---

## Trame

Chaque trame est encodée en [COBS](https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing)
puis terminée par un octet `0x00`.

```
                 ┌──── protégé par le CRC ────┐
+---------+------+------+-----+---------+------+
| VERSION | TYPE | SEQ  | ... | PAYLOAD | CRC16|
|   u8    |  u8  |  u8  |     |  0..48  | u16  |
+---------+------+------+-----+---------+------+
                                              │
                            COBS ─────────────┘ + 0x00
```

| Champ | Taille | Rôle |
|---|---|---|
| `VERSION` | 1 | Version du protocole. Vaut 1. |
| `TYPE` | 1 | Type de message. Bit 7 à 1 = émis par le microcontrôleur. |
| `SEQ` | 1 | Compteur cyclique de l'émetteur. Sert à détecter les trames perdues. |
| `PAYLOAD` | 0..48 | Contenu, dépend du type. Entiers et flottants en **petit-boutien**. |
| `CRC16` | 2 | CRC-16/CCITT-FALSE sur `VERSION..PAYLOAD`, petit-boutien. |

**Pourquoi COBS plutôt qu'un octet de début et des échappements.** COBS garantit qu'aucun
`0x00` n'apparaît dans la trame encodée. Le délimiteur est donc sans ambiguïté : après du
bruit sur la ligne, le récepteur se resynchronise au prochain `0x00`, sans risque de
confondre un octet de données avec un début de trame. Le surcoût est d'un octet par tranche
de 254, contre jusqu'au double pour un échappement classique.

**Pourquoi le CRC en plus.** COBS restitue les limites de trame, pas l'intégrité du
contenu. Le CRC-16/CCITT-FALSE détecte toutes les erreurs simples et doubles, et toutes
les rafales jusqu'à 16 bits — largement suffisant pour un câble USB court.

---

## Messages du calculateur vers le microcontrôleur

### `0x01` — `WheelVelocity`

Consigne de vitesse pour chaque roue. C'est le message nominal, émis à 50 Hz.

| Décalage | Type | Champ | Unité |
|---|---|---|---|
| 0 | `f32` | `left` | rad/s, positif vers l'avant |
| 4 | `f32` | `right` | rad/s, positif vers l'avant |
| 8 | `u32` | `timestamp_ms` | horloge du calculateur |

Réception de ce message : le watchdog du microcontrôleur est réarmé.

L'horodatage est celui de l'émetteur. C'est ce qui donne enfin du sens aux contrôles de
péremption du cœur : `geometry_msgs/Twist`, côté ROS 2, ne porte aucun horodatage, si bien
qu'une commande y était forcément datée de son instant d'arrivée.

### `0x02` — `Stop`

Arrêt immédiat. Les moteurs sont coupés sans rampe, et l'état reste verrouillé jusqu'à la
réception d'un `WheelVelocity`.

| Décalage | Type | Champ |
|---|---|---|
| 0 | `u8` | `reason` (voir codes ci-dessous) |
| 1 | `u32` | `timestamp_ms` |

Codes de raison : `0` opérateur, `1` arrêt de sécurité du cœur, `2` arrêt d'urgence,
`3` extinction.

### `0x03` — `Heartbeat`

Signe de vie sans consigne. Réarme le watchdog sans modifier la consigne courante — utile
lorsque le robot doit rester à l'arrêt sans que le microcontrôleur ne conclue à une perte
de liaison.

| Décalage | Type | Champ |
|---|---|---|
| 0 | `u32` | `timestamp_ms` |

---

## Messages du microcontrôleur vers le calculateur

### `0x81` — `Telemetry`

Émis à 50 Hz, sans sollicitation.

| Décalage | Type | Champ | Unité |
|---|---|---|---|
| 0 | `f32` | `left` | rad/s mesurés |
| 4 | `f32` | `right` | rad/s mesurés |
| 8 | `i32` | `left_ticks` | cumul d'impulsions d'encodeur |
| 12 | `i32` | `right_ticks` | cumul d'impulsions d'encodeur |
| 16 | `u32` | `timestamp_ms` | horloge du microcontrôleur |
| 20 | `u8` | `flags` | voir ci-dessous |

Les cumuls d'impulsions sont transmis en plus des vitesses : ils permettent au calculateur
de reconstruire l'odométrie sans dépendre du filtrage appliqué dans le firmware.

**Drapeaux** (bit à 1 = actif) :

| Bit | Nom | Signification |
|---|---|---|
| 0 | `MOTORS_ENABLED` | Étage de puissance actif |
| 1 | `WATCHDOG_EXPIRED` | Aucune commande reçue dans le délai imparti |
| 2 | `DRIVER_FAULT` | Le pont en H signale un défaut |
| 3 | `OVERCURRENT` | Surintensité détectée |
| 4 | `ENCODER_FAULT` | Encodeur incohérent ou muet |

### `0x82` — `Fault`

Émis une fois à l'apparition d'un défaut.

| Décalage | Type | Champ |
|---|---|---|
| 0 | `u8` | `code` |
| 1 | `u32` | `timestamp_ms` |

---

## Watchdog du microcontrôleur

Sans `WheelVelocity` ni `Heartbeat` pendant **200 ms**, le microcontrôleur coupe les
moteurs et lève `WATCHDOG_EXPIRED`.

Ce délai est délibérément plus court que le timeout de commande du cœur, qui vaut 500 ms.
La hiérarchie est voulue : le microcontrôleur réagit à une perte de liaison avant que le
cœur ne réagisse à une perte de commandes. Chaque étage protège contre la défaillance de
celui au-dessus.

```
arrêt d'urgence matériel     coupe la puissance          instantané
watchdog microcontrôleur     coupe les moteurs           200 ms
watchdog du cœur             passage en SAFE_STOP        500 ms
```

---

## Ce que le protocole ne fait pas

Pas de chiffrement ni d'authentification : la liaison est un câble USB de vingt
centimètres à l'intérieur du robot. Un attaquant capable de s'y brancher a déjà accès aux
moteurs.

Pas de retransmission ni d'accusé de réception. Une consigne de vitesse perdue est
remplacée 20 ms plus tard par la suivante ; la retransmettre livrerait une consigne
périmée, ce qui est pire. Le compteur `SEQ` sert à *mesurer* les pertes, pas à les
réparer.

Pas de réglage des gains PID à chaud en version 1. Ce sera un ajout utile — il fait la
différence entre une journée et une semaine de mise au point — mais il attendra que la
mécanique existe.
