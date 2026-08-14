"use client";

import { useCallback, useEffect, useRef, useState } from "react";

import { MapView } from "@/components/MapView";
import {
  fetchMap,
  postCommand,
  websocketUrl,
  type OccupancyMap,
  type RobotSnapshot,
} from "@/lib/api";

/** Instantane vide : le tableau de bord ne pretend rien connaitre avant de recevoir. */
const EMPTY: RobotSnapshot = {
  state: null,
  pose: null,
  velocity: null,
  battery: { instrumented: false },
  scan: null,
  navigation: { phase: "idle" },
  updatedAt: null,
};

interface LogLine {
  readonly at: string;
  readonly text: string;
  readonly severity: "info" | "warning" | "error";
}

export default function Dashboard() {
  const [snapshot, setSnapshot] = useState<RobotSnapshot>(EMPTY);
  const [map, setMap] = useState<OccupancyMap | null>(null);
  const [connected, setConnected] = useState(false);
  const [log, setLog] = useState<LogLine[]>([]);

  const append = useCallback((text: string, severity: LogLine["severity"] = "info") => {
    setLog((lines) =>
      [{ at: new Date().toLocaleTimeString("fr-FR"), text, severity }, ...lines].slice(0, 40),
    );
  }, []);

  // --- Liaison WebSocket ------------------------------------------------------
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let closed = false;
    let retry: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      if (closed) return;

      const socket = new WebSocket(websocketUrl());
      socketRef.current = socket;

      socket.onopen = () => setConnected(true);

      socket.onclose = () => {
        setConnected(false);
        // Reconnexion : c'est quand quelque chose redemarre qu'on regarde l'ecran.
        retry = setTimeout(connect, 1_000);
      };

      socket.onmessage = (event) => {
        const frame = JSON.parse(event.data as string) as { event: string; data: unknown };

        switch (frame.event) {
          case "robot.snapshot":
            setSnapshot(frame.data as RobotSnapshot);
            break;
          case "robot.state":
            setSnapshot((s) => ({ ...s, ...(frame.data as Partial<RobotSnapshot>) }));
            break;
          case "robot.pose":
            setSnapshot((s) => ({ ...s, pose: frame.data as RobotSnapshot["pose"] }));
            break;
          case "robot.navigation":
            setSnapshot((s) => ({
              ...s,
              navigation: frame.data as RobotSnapshot["navigation"],
            }));
            break;
          case "robot.alert": {
            const alert = frame.data as { severity: LogLine["severity"]; message: string };
            append(alert.message, alert.severity);
            break;
          }
          default:
            break;
        }
      };
    };

    connect();

    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      socketRef.current?.close();
    };
  }, [append]);

  // --- Carte ------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;

    const refresh = async () => {
      const next = await fetchMap();
      if (!cancelled && next) setMap(next);
    };

    void refresh();
    // La carte change lentement : la relire chaque seconde suffit largement, et evite
    // de transporter vingt mille entiers a chaque cycle de controle.
    const timer = setInterval(() => void refresh(), 2_000);

    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  // --- Commandes --------------------------------------------------------------
  const stop = useCallback(async () => {
    const result = await postCommand("/api/robot/stop");
    append(result.message, result.ok ? "warning" : "error");
  }, [append]);

  const clearEmergency = useCallback(async () => {
    const result = await postCommand("/api/robot/clear-emergency-stop");
    append(result.message, result.ok ? "info" : "error");
  }, [append]);

  const resume = useCallback(async () => {
    const result = await postCommand("/api/robot/resume");
    append(result.message, result.ok ? "info" : "error");
  }, [append]);

  const navigate = useCallback(
    async (state: string) => {
      const result = await postCommand("/api/robot/state", { state });
      append(result.message, result.ok ? "info" : "error");
    },
    [append],
  );

  const sendGoal = useCallback(
    async (x: number, y: number) => {
      const result = await postCommand("/api/robot/navigation", { x, y });
      append(
        result.ok ? `but envoye : ${x.toFixed(2)} ; ${y.toFixed(2)}` : result.message,
        result.ok ? "info" : "error",
      );
    },
    [append],
  );

  const speed = snapshot.velocity
    ? Math.hypot(snapshot.velocity.linear, 0)
    : null;

  return (
    <div className="shell" data-state={snapshot.state ?? "BOOTING"}>
      <header className="masthead">
        <h1 className="wordmark">
          Robo<span>to</span>
        </h1>
        <div className="masthead-meta">
          <span className="link-state" data-live={connected}>
            <span className="link-dot" />
            {connected ? "liaison etablie" : "liaison rompue"}
          </span>
          <span>
            {snapshot.updatedAt
              ? new Date(snapshot.updatedAt).toLocaleTimeString("fr-FR")
              : "en attente"}
          </span>
        </div>
      </header>

      <main className="workspace">
        <section className="panel map-panel">
          <span className="panel-label">carte — repere map</span>
          <MapView
            map={map}
            pose={snapshot.pose}
            navigation={snapshot.navigation}
            onGoal={(x, y) => void sendGoal(x, y)}
          />
        </section>

        <aside className="sidebar">
          <section className="panel state-block">
            <span className="panel-label">etat</span>
            <p className="state-name">{snapshot.state ?? "—"}</p>
            <p className="state-caption">
              {snapshot.state === "SAFE_STOP"
                ? "reprise explicite requise"
                : snapshot.navigation.phase === "active"
                  ? "mission en cours"
                  : "coeur robot-core"}
            </p>
          </section>

          <section className="panel readouts">
            <span className="panel-label">telemetrie</span>

            <Readout label="vitesse" value={speed === null ? null : `${speed.toFixed(2)} m/s`} />
            <Readout
              label="rotation"
              value={
                snapshot.velocity === null
                  ? null
                  : `${snapshot.velocity.angular.toFixed(2)} rad/s`
              }
            />
            <Readout
              label="position"
              value={
                snapshot.pose === null
                  ? null
                  : `${snapshot.pose.x.toFixed(2)} ; ${snapshot.pose.y.toFixed(2)} m`
              }
            />
            <Readout
              label="cap"
              value={
                snapshot.pose === null
                  ? null
                  : `${((snapshot.pose.theta * 180) / Math.PI).toFixed(0)}°`
              }
            />
            <Readout
              label="obstacle"
              value={
                snapshot.scan?.closest == null
                  ? null
                  : `${snapshot.scan.closest.toFixed(2)} m`
              }
              absentLabel={snapshot.scan ? "champ degage" : "pas de lidar"}
            />
            {/*
              Le robot n'a aucune mesure de tension : ni BMS instrumente en simulation,
              ni materiel. Afficher « 87 % » comme le fait la maquette d'origine serait
              inventer une donnee de securite.
            */}
            <Readout label="batterie" value={null} absentLabel="non instrumentee" />
            <Readout
              label="mission"
              value={
                snapshot.navigation.phase === "active"
                  ? snapshot.navigation.distanceRemaining === null
                    ? "en cours"
                    : `${snapshot.navigation.distanceRemaining.toFixed(2)} m restants`
                  : snapshot.navigation.phase === "succeeded"
                    ? "but atteint"
                    : snapshot.navigation.phase === "failed"
                      ? snapshot.navigation.reason
                      : null
              }
              absentLabel="aucune"
            />

            <div className="log">
              {log.map((line, index) => (
                <div className="log-line" data-severity={line.severity} key={index}>
                  <span className="log-time">{line.at}</span>
                  <span>{line.text}</span>
                </div>
              ))}
            </div>
          </section>

          <section className="panel controls">
            <button className="stop" onClick={() => void stop()}>
              Arret
            </button>
            {/*
              Deux boutons, dans cet ordre, et c'est voulu. Lever l'arret d'urgence ne
              remet pas le robot en marche ; il faut ensuite quitter SAFE_STOP. Les
              fusionner ferait d'un seul clic le contraire d'un arret d'urgence.
            */}
            <button className="secondary" onClick={() => void clearEmergency()}>
              1 — lever l&apos;arret d&apos;urgence
            </button>
            <button
              className="secondary"
              onClick={() => void resume()}
              disabled={snapshot.state !== "SAFE_STOP"}
            >
              2 — quitter l&apos;arret de securite
            </button>
            <button className="secondary" onClick={() => void navigate("NAVIGATING")}>
              autoriser la navigation
            </button>
          </section>
        </aside>
      </main>
    </div>
  );
}

/** Une ligne de telemetrie. Ce qui n'est pas mesure s'affiche comme tel. */
function Readout({
  label,
  value,
  absentLabel = "—",
}: {
  label: string;
  value: string | null;
  absentLabel?: string;
}) {
  return (
    <div className="readout">
      <span className="readout-label">{label}</span>
      <span className="readout-value" data-absent={value === null}>
        {value ?? absentLabel}
      </span>
    </div>
  );
}
