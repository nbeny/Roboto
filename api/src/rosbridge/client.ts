import { WebSocket } from "ws";

/**
 * Client du protocole rosbridge.
 *
 * rosbridge expose ROS 2 en JSON sur WebSocket. L'API ne depend donc d'aucun binding
 * natif : elle se construit et se teste sans ROS 2 installe, exactement comme le coeur
 * Rust. C'est le meme principe que
 * `docs/architecture/0001-coeur-rust-sans-ros2.md`, applique a l'autre extremite.
 *
 * # Reconnexion
 *
 * Le client se reconnecte et se reabonne seul. Un robot dont le tableau de bord meurt
 * parce que le pont a redemarre n'est pas un tableau de bord : c'est precisement quand
 * quelque chose redemarre qu'on a besoin de le regarder.
 */

export type TopicHandler = (message: unknown) => void;

export interface ServiceResult {
  readonly ok: boolean;
  readonly values: unknown;
}

export interface RosbridgeOptions {
  readonly url: string;
  /** Delai avant une nouvelle tentative de connexion. */
  readonly reconnectDelayMs?: number;
  /** Delai au-dela duquel un appel de service est abandonne. */
  readonly serviceTimeoutMs?: number;
}

interface Subscription {
  readonly type: string;
  readonly handlers: Set<TopicHandler>;
}

interface PendingCall {
  readonly resolve: (result: ServiceResult) => void;
  readonly reject: (error: Error) => void;
  readonly timer: NodeJS.Timeout;
}

export interface ActionOutcome {
  /** Vrai si l'action s'est achevee avec succes. */
  readonly succeeded: boolean;
  /** Code de statut de l'action, tel que rendu par ROS 2. */
  readonly status: number | null;
  readonly values: unknown;
}

export interface ActionGoalHandlers {
  readonly onResult?: (outcome: ActionOutcome) => void;
  readonly onFeedback?: (feedback: unknown) => void;
}

const DEFAULT_RECONNECT_DELAY_MS = 1_000;
const DEFAULT_SERVICE_TIMEOUT_MS = 5_000;

export class RosbridgeClient {
  readonly #url: string;
  readonly #reconnectDelayMs: number;
  readonly #serviceTimeoutMs: number;

  readonly #subscriptions = new Map<string, Subscription>();
  readonly #advertised = new Set<string>();
  readonly #pending = new Map<string, PendingCall>();

  #socket: WebSocket | null = null;
  #reconnectTimer: NodeJS.Timeout | null = null;
  #closed = false;
  #nextCallId = 0;

  #onConnected: (() => void) | null = null;
  #onDisconnected: (() => void) | null = null;

  constructor(options: RosbridgeOptions) {
    this.#url = options.url;
    this.#reconnectDelayMs = options.reconnectDelayMs ?? DEFAULT_RECONNECT_DELAY_MS;
    this.#serviceTimeoutMs = options.serviceTimeoutMs ?? DEFAULT_SERVICE_TIMEOUT_MS;
  }

  get connected(): boolean {
    return this.#socket?.readyState === WebSocket.OPEN;
  }

  onConnected(handler: () => void): void {
    this.#onConnected = handler;
  }

  onDisconnected(handler: () => void): void {
    this.#onDisconnected = handler;
  }

  connect(): void {
    if (this.#closed || this.#socket) {
      return;
    }

    const socket = new WebSocket(this.#url);
    this.#socket = socket;

    socket.on("open", () => {
      // Reabonnement systematique : apres une coupure, le pont a tout oublie.
      for (const [topic, subscription] of this.#subscriptions) {
        this.#send({ op: "subscribe", topic, type: subscription.type });
      }
      for (const topic of this.#advertised) {
        this.#send({ op: "advertise", topic, type: this.#advertisedTypes.get(topic) });
      }

      this.#onConnected?.();
    });

    socket.on("message", (data) => this.#receive(data.toString()));

    socket.on("error", () => {
      // L'evenement `close` suit toujours : la reconnexion est traitee la, en un seul
      // endroit.
    });

    socket.on("close", () => {
      this.#socket = null;
      this.#failPendingCalls("liaison rosbridge fermee");
      this.#onDisconnected?.();
      this.#scheduleReconnect();
    });
  }

  close(): void {
    this.#closed = true;

    if (this.#reconnectTimer) {
      clearTimeout(this.#reconnectTimer);
      this.#reconnectTimer = null;
    }

    this.#failPendingCalls("client ferme");
    this.#socket?.close();
    this.#socket = null;
  }

  subscribe(topic: string, type: string, handler: TopicHandler): void {
    const existing = this.#subscriptions.get(topic);

    if (existing) {
      existing.handlers.add(handler);
      return;
    }

    this.#subscriptions.set(topic, { type, handlers: new Set([handler]) });
    this.#send({ op: "subscribe", topic, type });
  }

  readonly #advertisedTypes = new Map<string, string>();

  publish(topic: string, type: string, message: unknown): void {
    if (!this.#advertised.has(topic)) {
      this.#advertised.add(topic);
      this.#advertisedTypes.set(topic, type);
      this.#send({ op: "advertise", topic, type });
    }

    this.#send({ op: "publish", topic, msg: message });
  }

  /**
   * Appelle un service ROS 2.
   *
   * Rejette apres `serviceTimeoutMs` plutot que d'attendre indefiniment : un appel
   * suspendu bloquerait une requete HTTP, et l'operateur ne saurait pas si son ordre est
   * parti.
   */
  callService(service: string, type: string, args: unknown): Promise<ServiceResult> {
    const id = `call_${this.#nextCallId++}`;

    return new Promise<ServiceResult>((resolve, reject) => {
      if (!this.connected) {
        reject(new Error("rosbridge injoignable"));
        return;
      }

      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`aucune reponse du service ${service} en ${this.#serviceTimeoutMs} ms`));
      }, this.#serviceTimeoutMs);

      this.#pending.set(id, { resolve, reject, timer });
      this.#send({ op: "call_service", id, service, type, args });
    });
  }

  /**
   * Envoie un but d'action ROS 2, sans attendre son aboutissement.
   *
   * Une navigation dure des secondes ou des minutes. Bloquer une requete HTTP jusqu'a
   * son terme serait faux : l'operateur veut savoir que son ordre est **parti**, et
   * suivre la progression ailleurs. Le resultat arrive par `onResult`, la progression
   * par `onFeedback`.
   *
   * Rend l'identifiant du but, ou `null` si le pont est injoignable.
   */
  sendActionGoal(
    action: string,
    actionType: string,
    args: unknown,
    handlers: ActionGoalHandlers = {},
  ): string | null {
    if (!this.connected) {
      return null;
    }

    const id = `goal_${this.#nextCallId++}`;
    this.#actions.set(id, handlers);

    this.#send({
      op: "send_action_goal",
      id,
      action,
      action_type: actionType,
      args,
      feedback: handlers.onFeedback !== undefined,
    });

    return id;
  }

  /** Annule un but d'action en cours. */
  cancelActionGoal(action: string, id: string): void {
    this.#send({ op: "cancel_action_goal", id, action });
  }

  readonly #actions = new Map<string, ActionGoalHandlers>();

  #scheduleReconnect(): void {
    if (this.#closed || this.#reconnectTimer) {
      return;
    }

    this.#reconnectTimer = setTimeout(() => {
      this.#reconnectTimer = null;
      this.connect();
    }, this.#reconnectDelayMs);
  }

  #failPendingCalls(reason: string): void {
    for (const [id, call] of this.#pending) {
      clearTimeout(call.timer);
      call.reject(new Error(reason));
      this.#pending.delete(id);
    }
  }

  #send(payload: unknown): void {
    if (this.#socket?.readyState === WebSocket.OPEN) {
      this.#socket.send(JSON.stringify(payload));
    }
  }

  #receive(raw: string): void {
    let payload: unknown;

    try {
      payload = JSON.parse(raw);
    } catch {
      // Une trame illisible ne doit pas faire tomber l'API.
      return;
    }

    if (typeof payload !== "object" || payload === null) {
      return;
    }

    const frame = payload as Record<string, unknown>;

    switch (frame["op"]) {
      case "publish": {
        const topic = frame["topic"];
        if (typeof topic !== "string") {
          return;
        }
        for (const handler of this.#subscriptions.get(topic)?.handlers ?? []) {
          handler(frame["msg"]);
        }
        return;
      }

      case "service_response": {
        const id = frame["id"];
        if (typeof id !== "string") {
          return;
        }
        const call = this.#pending.get(id);
        if (!call) {
          return;
        }
        this.#pending.delete(id);
        clearTimeout(call.timer);
        call.resolve({ ok: frame["result"] !== false, values: frame["values"] ?? null });
        return;
      }

      case "action_result": {
        const id = frame["id"];
        if (typeof id !== "string") {
          return;
        }
        const handlers = this.#actions.get(id);
        if (!handlers) {
          return;
        }
        this.#actions.delete(id);

        const status = frame["status"];
        handlers.onResult?.({
          succeeded: frame["result"] === true,
          status: typeof status === "number" ? status : null,
          values: frame["values"] ?? null,
        });
        return;
      }

      case "action_feedback": {
        const id = frame["id"];
        if (typeof id !== "string") {
          return;
        }
        this.#actions.get(id)?.onFeedback?.(frame["values"] ?? null);
        return;
      }

      default:
        // `status`, `set_level` et les autres operations du protocole ne concernent pas
        // cette API.
        return;
    }
  }
}
