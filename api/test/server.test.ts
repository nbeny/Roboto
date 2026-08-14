import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { WebSocket, WebSocketServer } from "ws";

import { ApiServer } from "../src/http/server.ts";
import { RobotService } from "../src/robot/service.ts";
import { RosbridgeClient } from "../src/rosbridge/client.ts";

/** Faux pont, reduit a ce dont ces essais ont besoin. */
class FakeRosbridge {
  readonly #server: WebSocketServer;
  readonly received: Record<string, unknown>[] = [];
  #socket: WebSocket | null = null;
  /** Reponse a donner aux appels de service. */
  serviceReply: { result: boolean; values: unknown } = {
    result: true,
    values: { success: true, message: "" },
  };

  private constructor(server: WebSocketServer) {
    this.#server = server;

    server.on("connection", (socket) => {
      this.#socket = socket;
      socket.on("message", (data) => {
        const frame = JSON.parse(data.toString()) as Record<string, unknown>;
        this.received.push(frame);

        if (frame["op"] === "call_service") {
          socket.send(
            JSON.stringify({
              op: "service_response",
              id: frame["id"],
              result: this.serviceReply.result,
              values: this.serviceReply.values,
            }),
          );
        }
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
      throw new Error("adresse indisponible");
    }
    return `ws://127.0.0.1:${address.port}`;
  }

  operations(op: string): Record<string, unknown>[] {
    return this.received.filter((frame) => frame["op"] === op);
  }

  publish(topic: string, msg: unknown): void {
    this.#socket?.send(JSON.stringify({ op: "publish", topic, msg }));
  }

  async stop(): Promise<void> {
    for (const client of this.#server.clients) {
      client.terminate();
    }
    await new Promise<void>((resolve) => this.#server.close(() => resolve()));
  }
}

async function eventually(condition: () => boolean, what: string, timeoutMs = 3_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (condition()) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`condition jamais atteinte : ${what}`);
}

let bridge: FakeRosbridge;
let client: RosbridgeClient;
let server: ApiServer;
let baseUrl: string;

beforeEach(async () => {
  bridge = await FakeRosbridge.start();
  client = new RosbridgeClient({ url: bridge.url, reconnectDelayMs: 50, serviceTimeoutMs: 500 });
  const service = new RobotService({ client });
  server = new ApiServer({ service, port: 0 });

  service.start();
  client.connect();
  await eventually(() => client.connected, "connexion au pont");

  const port = await server.listen();
  baseUrl = `http://127.0.0.1:${port}`;
});

afterEach(async () => {
  client.close();
  await server.close();
  await bridge.stop();
});

describe("lecture de l'etat", () => {
  it("rend un etat vide tant que rien n'est arrive", async () => {
    const response = await fetch(`${baseUrl}/api/robot/status`);

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toMatchObject({
      state: null,
      velocity: null,
      battery: { instrumented: false },
    });
  });

  it("annonce la batterie comme non instrumentee plutot que d'inventer un pourcentage", async () => {
    const body = (await (await fetch(`${baseUrl}/api/robot/status`)).json()) as {
      battery: unknown;
    };

    expect(body.battery).toEqual({ instrumented: false });
  });

  it("reflete l'etat publie par le coeur", async () => {
    bridge.publish("/robot_core/state", { data: "NAVIGATING" });
    await eventually(async () => true, "propagation");
    await new Promise((resolve) => setTimeout(resolve, 100));

    const body = (await (await fetch(`${baseUrl}/api/robot/status`)).json()) as {
      state: string | null;
    };

    expect(body.state).toBe("NAVIGATING");
  });

  it("ignore un etat que l'API ne connait pas", async () => {
    bridge.publish("/robot_core/state", { data: "DOCKING" });
    await new Promise((resolve) => setTimeout(resolve, 100));

    const body = (await (await fetch(`${baseUrl}/api/robot/status`)).json()) as {
      state: string | null;
    };

    expect(body.state).toBeNull();
  });

  it("rend la pose et les capteurs sur leurs routes", async () => {
    bridge.publish("/odom", {
      pose: { pose: { position: { x: 1.5, y: -0.5 }, orientation: { x: 0, y: 0, z: 0, w: 1 } } },
    });
    bridge.publish("/scan", { ranges: [3.0, 1.5], range_min: 0.05, range_max: 12 });
    await new Promise((resolve) => setTimeout(resolve, 100));

    const pose = (await (await fetch(`${baseUrl}/api/robot/pose`)).json()) as {
      pose: { x: number } | null;
    };
    const sensors = (await (await fetch(`${baseUrl}/api/robot/sensors`)).json()) as {
      scan: { closest: number } | null;
    };

    expect(pose.pose?.x).toBeCloseTo(1.5, 6);
    expect(sensors.scan?.closest).toBeCloseTo(1.5, 6);
  });
});

describe("commandes", () => {
  it("l'arret appelle le service d'arret d'urgence du coeur", async () => {
    const response = await fetch(`${baseUrl}/api/robot/stop`, { method: "POST" });

    expect(response.status).toBe(200);
    const calls = bridge.operations("call_service");
    expect(calls).toHaveLength(1);
    expect(calls[0]?.["service"]).toBe("/robot_core/emergency_stop");
  });

  it("rapporte un refus du coeur au lieu de le masquer", async () => {
    // Le coeur refuse de quitter SAFE_STOP tant que l'arret d'urgence tient.
    bridge.serviceReply = {
      result: true,
      values: { success: false, message: "arret d'urgence engage" },
    };

    const response = await fetch(`${baseUrl}/api/robot/resume`, { method: "POST" });

    expect(response.status).toBe(409);
    await expect(response.json()).resolves.toMatchObject({
      accepted: false,
      message: "arret d'urgence engage",
    });
  });

  it("accepte un but de navigation sans attendre l'arrivee", async () => {
    const response = await fetch(`${baseUrl}/api/robot/navigation`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ x: 1.5, y: -1.0 }),
    });

    // 202 et non 200 : le but est accepte, pas atteint.
    expect(response.status).toBe(202);
    await eventually(() => bridge.operations("send_action_goal").length === 1, "but transmis");

    const goal = bridge.operations("send_action_goal")[0];
    expect(goal?.["action"]).toBe("/navigate_to_pose");
  });

  it("refuse un but sans coordonnees exploitables", async () => {
    for (const body of [{}, { x: 1 }, { x: "gauche", y: 0 }, { x: 1, y: Number.NaN }]) {
      const response = await fetch(`${baseUrl}/api/robot/navigation`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });

      expect(response.status, JSON.stringify(body)).toBe(400);
    }
  });

  it("refuse un corps qui n'est pas du JSON", async () => {
    const response = await fetch(`${baseUrl}/api/robot/navigation`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{ceci n'est pas du JSON",
    });

    expect(response.status).toBe(400);
  });

  it("transmet une demande de transition sans prejuger de la reponse du coeur", async () => {
    const response = await fetch(`${baseUrl}/api/robot/state`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ state: "NAVIGATING" }),
    });

    // 202 : transmis, pas accorde. C'est le coeur qui decide.
    expect(response.status).toBe(202);
    await eventually(() => bridge.operations("publish").length === 1, "publication");
    expect(bridge.operations("publish")[0]?.["topic"]).toBe("/robot_core/request_state");
  });

  it("refuse un etat inconnu", async () => {
    const response = await fetch(`${baseUrl}/api/robot/state`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ state: "FLYING" }),
    });

    expect(response.status).toBe(400);
  });

  it("rend 404 sur une route inconnue", async () => {
    expect((await fetch(`${baseUrl}/api/robot/teapot`)).status).toBe(404);
  });
});

describe("WebSocket", () => {
  /** Ouvre un client WebSocket et collecte les evenements recus. */
  async function connectDashboard(): Promise<{ events: Record<string, unknown>[]; close: () => void }> {
    const socket = new WebSocket(`${baseUrl.replace("http", "ws")}/ws`);
    const events: Record<string, unknown>[] = [];

    socket.on("message", (data) => {
      events.push(JSON.parse(data.toString()) as Record<string, unknown>);
    });

    await new Promise<void>((resolve) => socket.once("open", resolve));

    return { events, close: () => socket.close() };
  }

  it("envoie l'instantane courant des la connexion", async () => {
    // Un robot immobile ne publie rien de nouveau pendant des minutes : un client qui
    // arrive doit voir l'etat, pas attendre le prochain changement.
    const dashboard = await connectDashboard();

    await eventually(() => dashboard.events.length >= 1, "instantane initial");
    expect(dashboard.events[0]?.["event"]).toBe("robot.snapshot");

    dashboard.close();
  });

  it("diffuse un changement d'etat", async () => {
    const dashboard = await connectDashboard();
    await eventually(() => dashboard.events.length >= 1, "instantane initial");

    bridge.publish("/robot_core/state", { data: "TELEOPERATION" });

    await eventually(
      () => dashboard.events.some((e) => e["event"] === "robot.state"),
      "evenement d'etat",
    );

    dashboard.close();
  });

  it("leve une alerte a l'entree en arret de securite", async () => {
    const dashboard = await connectDashboard();
    await eventually(() => dashboard.events.length >= 1, "instantane initial");

    bridge.publish("/robot_core/state", { data: "SAFE_STOP" });

    await eventually(
      () => dashboard.events.some((e) => e["event"] === "robot.alert"),
      "alerte",
    );

    const alert = dashboard.events.find((e) => e["event"] === "robot.alert");
    expect(alert?.["data"]).toMatchObject({ severity: "warning", state: "SAFE_STOP" });

    dashboard.close();
  });

  it("ne diffuse pas un changement d'etat qui n'en est pas un", async () => {
    const dashboard = await connectDashboard();
    await eventually(() => dashboard.events.length >= 1, "instantane initial");

    bridge.publish("/robot_core/state", { data: "IDLE" });
    await eventually(
      () => dashboard.events.some((e) => e["event"] === "robot.state"),
      "premier etat",
    );
    const afterFirst = dashboard.events.length;

    bridge.publish("/robot_core/state", { data: "IDLE" });
    await new Promise((resolve) => setTimeout(resolve, 150));

    expect(dashboard.events.length).toBe(afterFirst);

    dashboard.close();
  });
});
