import type { RosbridgeClient } from "../rosbridge/client.ts";
import {
  emptySnapshot,
  occupancyMapFromMessage,
  parseRobotState,
  poseFromOdometry,
  scanSummary,
  velocityFromTwist,
  type OccupancyMap,
  type RobotSnapshot,
  type RobotStateName,
} from "./snapshot.ts";

/**
 * Vue de l'API sur le robot.
 *
 * Cette classe **ne decide rien**. Elle met en cache ce que le robot publie et transmet
 * ce que l'operateur demande. Toute question de « le robot a-t-il le droit de » revient a
 * `robot-core` et `robot-safety`, cote Rust. L'API qui prendrait ces decisions les
 * prendrait sans les garanties du coeur, et un jour les deux ne diraient plus la meme
 * chose.
 */

export interface CommandOutcome {
  readonly accepted: boolean;
  readonly message: string;
}

export interface NavigationGoal {
  readonly x: number;
  readonly y: number;
  readonly theta: number;
}

export interface RobotServiceOptions {
  readonly client: RosbridgeClient;
  /** Nom du noeud du coeur, qui prefixe ses topics et services. */
  readonly nodeName?: string;
  /** Horloge injectable, pour rendre les essais deterministes. */
  readonly now?: () => Date;
}

const TRIGGER = "std_srvs/srv/Trigger";

export class RobotService {
  readonly #client: RosbridgeClient;
  readonly #prefix: string;
  readonly #now: () => Date;

  #snapshot: RobotSnapshot = emptySnapshot();
  #map: OccupancyMap | null = null;
  #listeners = new Set<(snapshot: RobotSnapshot) => void>();

  constructor(options: RobotServiceOptions) {
    this.#client = options.client;
    this.#prefix = `/${options.nodeName ?? "robot_core"}`;
    this.#now = options.now ?? (() => new Date());
  }

  get snapshot(): RobotSnapshot {
    return this.#snapshot;
  }

  /** Derniere carte recue, ou `null` si la cartographie n'a rien publie. */
  get map(): OccupancyMap | null {
    return this.#map;
  }

  onUpdate(listener: (snapshot: RobotSnapshot) => void): void {
    this.#listeners.add(listener);
  }

  /** Souscrit aux topics dont le tableau de bord a besoin. */
  start(): void {
    this.#client.subscribe(`${this.#prefix}/state`, "std_msgs/msg/String", (message) => {
      const state = parseRobotState((message as { data?: unknown } | null)?.data);
      if (state !== null) {
        this.#update({ state });
      }
    });

    this.#client.subscribe("/odom", "nav_msgs/msg/Odometry", (message) => {
      const pose = poseFromOdometry(message);
      if (pose !== null) {
        this.#update({ pose });
      }
    });

    this.#client.subscribe(
      `${this.#prefix}/cmd_vel_safe`,
      "geometry_msgs/msg/Twist",
      (message) => {
        const velocity = velocityFromTwist(message);
        if (velocity !== null) {
          this.#update({ velocity });
        }
      },
    );

    this.#client.subscribe("/scan", "sensor_msgs/msg/LaserScan", (message) => {
      const scan = scanSummary(message);
      if (scan !== null) {
        this.#update({ scan });
      }
    });

    // La carte est mise de cote, pas diffusee : voir `OccupancyMap`.
    this.#client.subscribe("/map", "nav_msgs/msg/OccupancyGrid", (message) => {
      const map = occupancyMapFromMessage(message);
      if (map !== null) {
        this.#map = map;
      }
    });
  }

  /**
   * Engage l'arret d'urgence.
   *
   * Ce n'est **pas** l'arret d'urgence du robot. Celui-la est un bouton champignon qui
   * coupe la puissance par relais, et aucun logiciel ne peut s'y substituer. Voir
   * SECURITY.md.
   */
  emergencyStop(): Promise<CommandOutcome> {
    return this.#trigger(`${this.#prefix}/emergency_stop`, "arret d'urgence engage");
  }

  clearEmergencyStop(): Promise<CommandOutcome> {
    return this.#trigger(`${this.#prefix}/clear_emergency_stop`, "arret d'urgence leve");
  }

  clearSafeStop(): Promise<CommandOutcome> {
    return this.#trigger(`${this.#prefix}/clear_safe_stop`, "arret de securite leve");
  }

  /**
   * Envoie un but de navigation a Nav2.
   *
   * Rend la main des que l'ordre est parti, sans attendre l'arrivee du robot : une
   * mission dure des minutes, et une requete HTTP suspendue si longtemps n'apprend rien
   * a personne. La progression passe par l'instantane et le WebSocket.
   *
   * Le coeur garde le dernier mot : Nav2 publie sur `/nav/cmd_vel`, dont les consignes
   * ne sont acceptees qu'en etat `NAVIGATING`.
   */
  navigateTo(goal: NavigationGoal): CommandOutcome {
    const id = this.#client.sendActionGoal(
      "/navigate_to_pose",
      "nav2_msgs/action/NavigateToPose",
      {
        pose: {
          header: { frame_id: "map" },
          pose: {
            position: { x: goal.x, y: goal.y, z: 0 },
            orientation: {
              x: 0,
              y: 0,
              z: Math.sin(goal.theta / 2),
              w: Math.cos(goal.theta / 2),
            },
          },
        },
      },
      {
        onFeedback: (feedback) => {
          const remaining = (feedback as { distance_remaining?: unknown } | null)
            ?.distance_remaining;

          this.#update({
            navigation: {
              phase: "active",
              goal,
              distanceRemaining:
                typeof remaining === "number" && Number.isFinite(remaining) ? remaining : null,
            },
          });
        },
        onResult: (outcome) => {
          this.#update({
            navigation: outcome.succeeded
              ? { phase: "succeeded", goal }
              : { phase: "failed", goal, reason: `statut ${outcome.status ?? "inconnu"}` },
          });
        },
      },
    );

    if (id === null) {
      return { accepted: false, message: "rosbridge injoignable" };
    }

    this.#update({ navigation: { phase: "active", goal, distanceRemaining: null } });

    return { accepted: true, message: `but transmis a Nav2 (${id})` };
  }

  /** Demande une transition d'etat. Le coeur reste libre de la refuser. */
  requestState(state: RobotStateName): CommandOutcome {
    this.#client.publish(`${this.#prefix}/request_state`, "std_msgs/msg/String", {
      data: state,
    });

    return {
      accepted: true,
      message: `transition vers ${state} demandee ; le coeur reste libre de la refuser`,
    };
  }

  async #trigger(service: string, success: string): Promise<CommandOutcome> {
    try {
      const result = await this.#client.callService(service, TRIGGER, {});
      const values = result.values as { success?: unknown; message?: unknown } | null;

      // Un service peut repondre « bien recu » tout en refusant l'ordre : c'est le champ
      // `success` du service qui fait foi, pas la reussite de l'appel.
      const accepted = result.ok && values?.success !== false;

      return {
        accepted,
        message: typeof values?.message === "string" && values.message.length > 0
          ? values.message
          : success,
      };
    } catch (error) {
      return {
        accepted: false,
        message: error instanceof Error ? error.message : "echec de l'appel de service",
      };
    }
  }

  #update(patch: Partial<RobotSnapshot>): void {
    this.#snapshot = {
      ...this.#snapshot,
      ...patch,
      updatedAt: this.#now().toISOString(),
    };

    for (const listener of this.#listeners) {
      listener(this.#snapshot);
    }
  }
}
