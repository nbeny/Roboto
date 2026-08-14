import { afterEach, describe, expect, it } from "vitest";
import { WebSocketServer, type WebSocket } from "ws";

import { RosbridgeClient } from "../src/rosbridge/client.ts";

/**
 * Faux pont rosbridge.
 *
 * Rend l'API testable sans ROS 2 : c'est tout l'interet d'avoir choisi un protocole
 * JSON sur WebSocket plutot qu'un binding natif.
 */
class FakeRosbridge {
  readonly #server: WebSocketServer;
  readonly received: Record<string, unknown>[] = [];
  #socket: WebSocket | null = null;

  private constructor(server: WebSocketServer) {
    this.#server = server;

    server.on("connection", (socket) => {
      this.#socket = socket;
      socket.on("message", (data) => {
        this.received.push(JSON.parse(data.toString()) as Record<string, unknown>);
      });
    });
  }

  static async start(): Promise<FakeRosbridge> {
    const server = new WebSocketServer({ port: 0 });
    await new Promise<void>((resolve) => server.once("listening", resolve));
    return new FakeRosbridge(server);
  }

  get url(): string {
    const address = this.#server.address();
    if (typeof address === "string" || address === null) {
      throw new Error("adresse du faux pont indisponible");
    }
    return `ws://127.0.0.1:${address.port}`;
  }

  get connectionCount(): number {
    return this.#server.clients.size;
  }

  operations(op: string): Record<string, unknown>[] {
    return this.received.filter((frame) => frame["op"] === op);
  }

  send(payload: unknown): void {
    this.#socket?.send(JSON.stringify(payload));
  }

  /** Envoie une trame brute, y compris invalide. */
  sendRaw(raw: string): void {
    this.#socket?.send(raw);
  }

  /** Coupe la connexion courante sans arreter le serveur. */
  dropConnection(): void {
    for (const client of this.#server.clients) {
      client.terminate();
    }
  }

  async stop(): Promise<void> {
    for (const client of this.#server.clients) {
      client.terminate();
    }
    await new Promise<void>((resolve) => this.#server.close(() => resolve()));
  }
}

/** Attend qu'une condition devienne vraie, ou echoue. */
async function eventually(
  condition: () => boolean,
  what: string,
  timeoutMs = 3_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    if (condition()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }

  throw new Error(`condition jamais atteinte : ${what}`);
}

let bridge: FakeRosbridge;
let client: RosbridgeClient;

afterEach(async () => {
  client?.close();
  await bridge?.stop();
});

async function connectedClient(options: { reconnectDelayMs?: number } = {}) {
  bridge = await FakeRosbridge.start();
  client = new RosbridgeClient({
    url: bridge.url,
    reconnectDelayMs: options.reconnectDelayMs ?? 20,
    serviceTimeoutMs: 300,
  });
  client.connect();
  await eventually(() => client.connected, "connexion au pont");
  return { bridge, client };
}

describe("abonnements", () => {
  it("s'abonne et transmet les messages recus", async () => {
    const { bridge, client } = await connectedClient();
    const received: unknown[] = [];

    client.subscribe("/robot_core/state", "std_msgs/msg/String", (message) => {
      received.push(message);
    });
    await eventually(() => bridge.operations("subscribe").length === 1, "abonnement");

    bridge.send({ op: "publish", topic: "/robot_core/state", msg: { data: "IDLE" } });

    await eventually(() => received.length === 1, "message transmis");
    expect(received[0]).toEqual({ data: "IDLE" });
  });

  it("partage un seul abonnement entre plusieurs consommateurs", async () => {
    const { bridge, client } = await connectedClient();
    const first: unknown[] = [];
    const second: unknown[] = [];

    client.subscribe("/scan", "sensor_msgs/msg/LaserScan", (m) => first.push(m));
    client.subscribe("/scan", "sensor_msgs/msg/LaserScan", (m) => second.push(m));
    await eventually(() => bridge.operations("subscribe").length === 1, "abonnement unique");

    bridge.send({ op: "publish", topic: "/scan", msg: { ranges: [] } });

    await eventually(() => first.length === 1 && second.length === 1, "diffusion");
  });

  it("ignore les messages d'un topic auquel on n'est pas abonne", async () => {
    const { bridge, client } = await connectedClient();
    const received: unknown[] = [];
    client.subscribe("/odom", "nav_msgs/msg/Odometry", (m) => received.push(m));

    bridge.send({ op: "publish", topic: "/autre", msg: { data: 1 } });
    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(received).toHaveLength(0);
  });
});

describe("robustesse", () => {
  it("ne tombe pas sur une trame illisible", async () => {
    const { bridge, client } = await connectedClient();

    bridge.sendRaw("{ceci n'est pas du JSON");
    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(client.connected).toBe(true);
  });

  it("se reconnecte et se reabonne apres une coupure", async () => {
    // Le cas qui compte : le pont redemarre, et le tableau de bord doit revivre seul.
    const { bridge, client } = await connectedClient();
    client.subscribe("/robot_core/state", "std_msgs/msg/String", () => {});
    await eventually(() => bridge.operations("subscribe").length === 1, "premier abonnement");

    bridge.dropConnection();
    await eventually(() => !client.connected, "deconnexion detectee");

    await eventually(() => bridge.operations("subscribe").length === 2, "reabonnement");
    expect(client.connected).toBe(true);
  });

  it("cesse de se reconnecter une fois ferme", async () => {
    const { bridge, client } = await connectedClient();

    client.close();
    bridge.dropConnection();
    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(client.connected).toBe(false);
    expect(bridge.connectionCount).toBe(0);
  });
});

describe("publication", () => {
  it("annonce le topic avant d'y publier", async () => {
    const { bridge, client } = await connectedClient();

    client.publish("/cmd_vel", "geometry_msgs/msg/Twist", { linear: { x: 0.2 } });
    await eventually(() => bridge.operations("publish").length === 1, "publication");

    expect(bridge.operations("advertise")).toHaveLength(1);
    expect(bridge.operations("advertise")[0]?.["topic"]).toBe("/cmd_vel");
  });

  it("n'annonce le topic qu'une seule fois", async () => {
    const { bridge, client } = await connectedClient();

    client.publish("/cmd_vel", "geometry_msgs/msg/Twist", {});
    client.publish("/cmd_vel", "geometry_msgs/msg/Twist", {});
    await eventually(() => bridge.operations("publish").length === 2, "deux publications");

    expect(bridge.operations("advertise")).toHaveLength(1);
  });
});

describe("appels de service", () => {
  it("resout avec la reponse du service", async () => {
    const { bridge, client } = await connectedClient();

    const call = client.callService(
      "/robot_core/emergency_stop",
      "std_srvs/srv/Trigger",
      {},
    );
    await eventually(() => bridge.operations("call_service").length === 1, "appel emis");

    const id = bridge.operations("call_service")[0]?.["id"];
    bridge.send({
      op: "service_response",
      id,
      result: true,
      values: { success: true, message: "arret d'urgence engage" },
    });

    await expect(call).resolves.toEqual({
      ok: true,
      values: { success: true, message: "arret d'urgence engage" },
    });
  });

  it("rapporte un service qui repond en echec", async () => {
    const { bridge, client } = await connectedClient();
    const call = client.callService("/robot_core/clear_safe_stop", "std_srvs/srv/Trigger", {});
    await eventually(() => bridge.operations("call_service").length === 1, "appel emis");

    const id = bridge.operations("call_service")[0]?.["id"];
    bridge.send({ op: "service_response", id, result: false, values: null });

    await expect(call).resolves.toMatchObject({ ok: false });
  });

  it("abandonne un appel sans reponse plutot que d'attendre indefiniment", async () => {
    // Une requete HTTP suspendue laisserait l'operateur sans savoir si son ordre est
    // parti.
    const { client } = await connectedClient();

    await expect(
      client.callService("/robot_core/stop", "std_srvs/srv/Trigger", {}),
    ).rejects.toThrow(/aucune reponse/);
  });

  it("refuse un appel quand le pont est injoignable", async () => {
    const { bridge, client } = await connectedClient();
    bridge.dropConnection();
    await eventually(() => !client.connected, "deconnexion");

    await expect(
      client.callService("/robot_core/stop", "std_srvs/srv/Trigger", {}),
    ).rejects.toThrow(/injoignable/);
  });

  it("rejette les appels en cours quand la liaison tombe", async () => {
    const { bridge, client } = await connectedClient();
    const call = client.callService("/robot_core/stop", "std_srvs/srv/Trigger", {});
    await eventually(() => bridge.operations("call_service").length === 1, "appel emis");

    bridge.dropConnection();

    await expect(call).rejects.toThrow(/fermee/);
  });
});
