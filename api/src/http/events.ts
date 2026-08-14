import type { RobotSnapshot } from "../robot/snapshot.ts";

/**
 * Traduction d'un changement d'instantane en evenements pour le tableau de bord.
 *
 * Fonction pure : c'est ce qui la rend testable sans ouvrir la moindre socket.
 */

export interface RobotEvent {
  readonly event: string;
  readonly data: unknown;
}

/** Etats qui meritent d'alerter l'operateur plutot que d'etre simplement affiches. */
const ALERTING_STATES = new Set(["SAFE_STOP", "ERROR"]);

/** Compare deux instantanes et rend les evenements a diffuser. */
export function diffToEvents(
  previous: RobotSnapshot | null,
  current: RobotSnapshot,
): RobotEvent[] {
  const events: RobotEvent[] = [];

  if (previous?.state !== current.state) {
    events.push({ event: "robot.state", data: { state: current.state } });

    // Une entree en SAFE_STOP ou ERROR n'est pas un changement d'affichage parmi
    // d'autres : c'est ce que l'operateur doit voir en premier.
    if (current.state !== null && ALERTING_STATES.has(current.state)) {
      events.push({
        event: "robot.alert",
        data: {
          severity: current.state === "ERROR" ? "error" : "warning",
          state: current.state,
          message:
            current.state === "SAFE_STOP"
              ? "arret de securite : la reprise doit etre demandee explicitement"
              : "le robot est en faute",
        },
      });
    }
  }

  if (!samePose(previous, current)) {
    events.push({ event: "robot.pose", data: current.pose });
  }

  if (previous?.battery.instrumented !== current.battery.instrumented) {
    events.push({ event: "robot.battery", data: current.battery });
  }

  if (previous?.navigation.phase !== current.navigation.phase) {
    events.push({ event: "robot.navigation", data: current.navigation });
  }

  return events;
}

function samePose(previous: RobotSnapshot | null, current: RobotSnapshot): boolean {
  const before = previous?.pose ?? null;
  const after = current.pose;

  if (before === null || after === null) {
    return before === after;
  }

  // Un seuil, et non une egalite stricte : l'odometrie bouge en permanence de quelques
  // micrometres, et diffuser cela a 50 Hz noierait le tableau de bord sans rien lui
  // apprendre.
  return (
    Math.abs(before.x - after.x) < 1e-3 &&
    Math.abs(before.y - after.y) < 1e-3 &&
    Math.abs(before.theta - after.theta) < 1e-3
  );
}
