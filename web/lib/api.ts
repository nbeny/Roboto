/** Types partages avec l'API, et le peu de code reseau dont le tableau de bord a besoin. */

export const ROBOT_STATES = [
  "BOOTING",
  "IDLE",
  "TELEOPERATION",
  "NAVIGATING",
  "PAUSED",
  "ERROR",
  "SAFE_STOP",
  "CHARGING",
] as const;

export type RobotStateName = (typeof ROBOT_STATES)[number];

export interface Pose2d {
  x: number;
  y: number;
  theta: number;
}

export interface Velocity2d {
  linear: number;
  angular: number;
}

export type BatteryStatus =
  | { instrumented: false }
  | { instrumented: true; percentage: number; voltage: number };

export interface ScanSummary {
  samples: number;
  closest: number | null;
  rangeMin: number;
  rangeMax: number;
}

export type NavigationStatus =
  | { phase: "idle" }
  | { phase: "active"; goal: Pose2d; distanceRemaining: number | null }
  | { phase: "succeeded"; goal: Pose2d }
  | { phase: "failed"; goal: Pose2d; reason: string };

export interface RobotSnapshot {
  state: RobotStateName | null;
  pose: Pose2d | null;
  velocity: Velocity2d | null;
  battery: BatteryStatus;
  scan: ScanSummary | null;
  navigation: NavigationStatus;
  updatedAt: string | null;
}

export interface OccupancyMap {
  width: number;
  height: number;
  resolution: number;
  origin: Pose2d;
  cells: number[];
}

export const API_BASE =
  process.env.NEXT_PUBLIC_API_BASE ?? "http://127.0.0.1:8080";

export function websocketUrl(): string {
  return `${API_BASE.replace(/^http/, "ws")}/ws`;
}

export async function fetchMap(): Promise<OccupancyMap | null> {
  try {
    const response = await fetch(`${API_BASE}/api/robot/map`, { cache: "no-store" });
    // 404 signifie « la cartographie n'a rien publie », pas une panne.
    return response.ok ? ((await response.json()) as OccupancyMap) : null;
  } catch {
    return null;
  }
}

export async function postCommand(
  path: string,
  body?: unknown,
): Promise<{ ok: boolean; message: string }> {
  try {
    const response = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? null : JSON.stringify(body),
    });

    const payload = (await response.json().catch(() => null)) as
      | { message?: string; error?: string }
      | null;

    return {
      ok: response.ok,
      message: payload?.message ?? payload?.error ?? `HTTP ${response.status}`,
    };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : "API injoignable",
    };
  }
}
