import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { WebSocketServer, type WebSocket } from "ws";

import type { RobotService } from "../robot/service.ts";
import { ROBOT_STATES, type RobotSnapshot, type RobotStateName } from "../robot/snapshot.ts";
import { diffToEvents } from "./events.ts";

/**
 * Serveur REST et WebSocket du tableau de bord.
 *
 * L'API ne contient **aucune logique robotique**. Elle valide ses entrees — c'est le
 * travail d'une frontiere — puis transmet au coeur, qui decide. Si l'API se mettait a
 * decider, elle le ferait sans les garanties de `robot-safety`, et un jour les deux ne
 * diraient plus la meme chose.
 */

export interface ApiServerOptions {
  readonly service: RobotService;
  readonly host?: string;
  readonly port?: number;
}

/** Taille maximale d'un corps de requete. Au-dela, c'est une erreur ou une attaque. */
const MAX_BODY_BYTES = 16 * 1024;

export class ApiServer {
  readonly #service: RobotService;
  readonly #host: string;
  readonly #port: number;
  readonly #http: Server;
  readonly #websocket: WebSocketServer;

  #previous: RobotSnapshot | null = null;

  constructor(options: ApiServerOptions) {
    this.#service = options.service;
    this.#host = options.host ?? "127.0.0.1";
    this.#port = options.port ?? 8080;

    this.#http = createServer((request, response) => {
      void this.#route(request, response);
    });

    this.#websocket = new WebSocketServer({ server: this.#http, path: "/ws" });

    this.#websocket.on("connection", (socket) => {
      // Un client qui arrive doit voir l'etat courant, pas attendre le prochain
      // changement : un robot immobile ne publie rien de nouveau pendant des minutes.
      send(socket, { event: "robot.snapshot", data: this.#service.snapshot });
    });

    this.#service.onUpdate((snapshot) => {
      for (const event of diffToEvents(this.#previous, snapshot)) {
        this.#broadcast(event);
      }
      this.#previous = snapshot;
    });
  }

  async listen(): Promise<number> {
    await new Promise<void>((resolve) => {
      this.#http.listen(this.#port, this.#host, resolve);
    });

    const address = this.#http.address();
    if (address === null || typeof address === "string") {
      throw new Error("adresse d'ecoute indisponible");
    }

    return address.port;
  }

  async close(): Promise<void> {
    for (const client of this.#websocket.clients) {
      client.terminate();
    }
    await new Promise<void>((resolve) => this.#websocket.close(() => resolve()));
    await new Promise<void>((resolve) => this.#http.close(() => resolve()));
  }

  #broadcast(payload: unknown): void {
    for (const client of this.#websocket.clients) {
      send(client, payload);
    }
  }

  async #route(request: IncomingMessage, response: ServerResponse): Promise<void> {
    const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);
    const route = `${request.method ?? "GET"} ${url.pathname}`;

    try {
      switch (route) {
        case "GET /api/health":
          return json(response, 200, { ok: true });

        case "GET /api/robot/status": {
          const { state, velocity, battery, navigation, updatedAt } = this.#service.snapshot;
          return json(response, 200, { state, velocity, battery, navigation, updatedAt });
        }

        case "GET /api/robot/pose":
          return json(response, 200, { pose: this.#service.snapshot.pose });

        case "GET /api/robot/sensors":
          return json(response, 200, { scan: this.#service.snapshot.scan });

        case "GET /api/robot/vision": {
          const scene = this.#service.vision;
          // 404 plutot qu'une scene vide : « la vision ne tourne pas » et « la camera
          // ne reconnait rien » sont deux situations differentes, et confondre les deux
          // ferait croire a un robot aveugle qu'il a bien regarde.
          return scene === null
            ? json(response, 404, { error: "la perception visuelle ne publie pas" })
            : json(response, 200, scene);
        }

        case "GET /api/robot/map": {
          const map = this.#service.map;
          // 404 plutot qu'une carte vide : « pas encore de carte » et « carte
          // entierement inconnue » sont deux situations differentes.
          return map === null
            ? json(response, 404, { error: "aucune carte disponible" })
            : json(response, 200, map);
        }

        case "POST /api/robot/stop": {
          const outcome = await this.#service.emergencyStop();
          return json(response, outcome.accepted ? 200 : 502, outcome);
        }

        // Deux etapes, et c'est voulu. Lever l'arret d'urgence ne remet pas le robot en
        // marche : il faut ensuite quitter SAFE_STOP explicitement. Les fusionner
        // ferait d'un seul clic le contraire d'un arret d'urgence.
        case "POST /api/robot/clear-emergency-stop": {
          const outcome = await this.#service.clearEmergencyStop();
          return json(response, outcome.accepted ? 200 : 502, outcome);
        }

        case "POST /api/robot/resume": {
          const outcome = await this.#service.clearSafeStop();
          // 409 : le coeur refuse tant que l'arret d'urgence tient. Le message le dit,
          // et l'operateur doit le lever d'abord.
          return json(response, outcome.accepted ? 200 : 409, outcome);
        }

        case "POST /api/robot/navigation":
          return await this.#navigate(request, response);

        case "POST /api/robot/state":
          return await this.#requestState(request, response);

        default:
          return json(response, 404, { error: `route inconnue : ${route}` });
      }
    } catch (error) {
      return json(response, 500, {
        error: error instanceof Error ? error.message : "erreur interne",
      });
    }
  }

  async #navigate(request: IncomingMessage, response: ServerResponse): Promise<void> {
    const body = await readJson(request);
    if (body === null) {
      return json(response, 400, { error: "corps JSON invalide" });
    }

    const x = finite(body, "x");
    const y = finite(body, "y");
    const theta = body["theta"] === undefined ? 0 : finite(body, "theta");

    if (x === null || y === null || theta === null) {
      return json(response, 400, {
        error: "x et y sont requis et doivent etre des nombres finis ; theta est optionnel",
      });
    }

    const outcome = this.#service.navigateTo({ x, y, theta });

    // 202 et non 200 : le but est accepte, pas atteint. Une navigation dure des minutes.
    return json(response, outcome.accepted ? 202 : 502, outcome);
  }

  async #requestState(request: IncomingMessage, response: ServerResponse): Promise<void> {
    const body = await readJson(request);
    const requested = body?.["state"];

    if (typeof requested !== "string" || !(ROBOT_STATES as readonly string[]).includes(requested)) {
      return json(response, 400, {
        error: `etat inconnu ; attendu l'un de ${ROBOT_STATES.join(", ")}`,
      });
    }

    const outcome = this.#service.requestState(requested as RobotStateName);

    // 202 : la demande est transmise, le coeur reste libre de la refuser.
    return json(response, 202, outcome);
  }
}

function send(socket: WebSocket, payload: unknown): void {
  if (socket.readyState === socket.OPEN) {
    socket.send(JSON.stringify(payload));
  }
}

function json(response: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(payload),
    // Le tableau de bord tourne sur un autre port pendant le developpement.
    "access-control-allow-origin": "*",
  });
  response.end(payload);
}

function finite(body: Record<string, unknown>, key: string): number | null {
  const value = body[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

async function readJson(request: IncomingMessage): Promise<Record<string, unknown> | null> {
  const chunks: Buffer[] = [];
  let size = 0;

  for await (const chunk of request) {
    size += (chunk as Buffer).length;
    if (size > MAX_BODY_BYTES) {
      return null;
    }
    chunks.push(chunk as Buffer);
  }

  if (chunks.length === 0) {
    return null;
  }

  try {
    const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    return typeof parsed === "object" && parsed !== null
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}
