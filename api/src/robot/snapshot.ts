/**
 * Modele de ce que le dashboard affiche, et traduction depuis les messages ROS 2.
 *
 * Toutes les fonctions de ce module prennent `unknown` et valident. Ce n'est pas de la
 * paranoia : elles sont a la frontiere du systeme, et ce qui arrive par la vient d'un
 * pont JSON dont le contenu depend de la version de ROS 2, du firmware, et de ce qui
 * tourne sur le reseau. Une donnee inattendue doit produire `null`, jamais une valeur
 * inventee ni une exception qui fait tomber l'API.
 */

/** Les huit etats du coeur, dans l'ordre de `robot_core::RobotState`. */
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
  /** Metres. */
  readonly x: number;
  /** Metres. */
  readonly y: number;
  /** Radians, sens trigonometrique. */
  readonly theta: number;
}

export interface Velocity2d {
  /** m/s, positif vers l'avant. */
  readonly linear: number;
  /** rad/s, positif dans le sens trigonometrique. */
  readonly angular: number;
}

/**
 * Etat de la batterie.
 *
 * Le robot n'a pas encore de mesure de tension : il n'y a ni BMS instrumente en
 * simulation, ni materiel. Le type le dit explicitement plutot que de rendre un
 * pourcentage invente. Un tableau de bord qui affiche « 87 % » sans rien mesurer est
 * pire qu'un tableau de bord qui affiche « non instrumente ».
 */
export type BatteryStatus =
  | { readonly instrumented: false }
  | { readonly instrumented: true; readonly percentage: number; readonly voltage: number };

export interface ScanSummary {
  /** Nombre de mesures dans le balayage. */
  readonly samples: number;
  /** Distance a l'obstacle le plus proche, ou `null` si le champ est degage. */
  readonly closest: number | null;
  /** Portee minimale annoncee par le capteur, en metres. */
  readonly rangeMin: number;
  /** Portee maximale annoncee par le capteur, en metres. */
  readonly rangeMax: number;
}

/**
 * Grille d'occupation produite par `slam_toolbox`.
 *
 * Volontairement tenue a l'ecart de [`RobotSnapshot`] : une carte de 159 x 119 cellules
 * represente une vingtaine de milliers d'entiers, et la diffuser a chaque changement
 * d'etat noierait le WebSocket. Elle se recupere par requete, quand le tableau de bord en
 * a besoin.
 */
export interface OccupancyMap {
  readonly width: number;
  readonly height: number;
  /** Taille d'une cellule, en metres. */
  readonly resolution: number;
  /** Coin inferieur gauche de la carte, dans le repere `map`. */
  readonly origin: Pose2d;
  /** Occupation par cellule : -1 inconnu, 0 libre, 100 occupe. */
  readonly cells: readonly number[];
}

/** Traduit un `nav_msgs/OccupancyGrid`. */
export function occupancyMapFromMessage(raw: unknown): OccupancyMap | null {
  const info = child(raw, "info");
  const width = finiteField(info, "width");
  const height = finiteField(info, "height");
  const resolution = finiteField(info, "resolution");
  const originPose = child(info, "origin");
  const position = child(originPose, "position");
  const cells = child(raw, "data");

  if (width === null || height === null || resolution === null || !Array.isArray(cells)) {
    return null;
  }

  const x = finiteField(position, "x");
  const y = finiteField(position, "y");
  const theta = yawFromQuaternion(child(originPose, "orientation"));

  if (x === null || y === null || theta === null) {
    return null;
  }

  // Une carte dont la taille annoncee ne correspond pas a son contenu est inexploitable :
  // la dessiner produirait une image decalee, ce qui est pire qu'une absence de carte.
  if (cells.length !== width * height) {
    return null;
  }

  return { width, height, resolution, origin: { x, y, theta }, cells: cells as number[] };
}

/**
 * Ce que la camera reconnait, tel que publie par le noeud de vision.
 *
 * Tenu a l'ecart de [`RobotSnapshot`] pour la meme raison que la carte : la scene change
 * a 2 Hz et n'interesse pas chaque cycle de controle. Elle se recupere par requete.
 */
export interface VisionDetection {
  readonly label: string;
  readonly confidence: number;
  /** Gisement en radians, positif vers la gauche. `null` si non calculable. */
  readonly bearing: number | null;
}

export interface VisionScene {
  readonly detections: readonly VisionDetection[];
  /** Phrase prete a lire, produite par la couche de vision. */
  readonly description: string;
  readonly at: string;
}

/** Traduit la charge utile JSON publiee sur `/vision/scene`. */
export function visionSceneFromMessage(raw: unknown): VisionScene | null {
  const text = child(raw, "data");
  if (typeof text !== "string") {
    return null;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    // Une scene illisible est ignoree plutot que propagee : la perception est une
    // information d'appoint, elle ne doit pas faire tomber la lecture de l'etat.
    return null;
  }

  const detections = child(parsed, "detections");
  const description = child(parsed, "description");
  const at = child(parsed, "at");

  if (!Array.isArray(detections) || typeof description !== "string") {
    return null;
  }

  return {
    detections: detections.flatMap((entry) => {
      const label = child(entry, "label");
      const confidence = finiteField(entry, "confidence");
      const bearing = finiteField(entry, "bearing");
      return typeof label === "string" && confidence !== null
        ? [{ label, confidence, bearing }]
        : [];
    }),
    description,
    at: typeof at === "string" ? at : new Date().toISOString(),
  };
}

/** Ou en est la mission de navigation courante. */
export type NavigationStatus =
  | { readonly phase: "idle" }
  | {
      readonly phase: "active";
      readonly goal: Pose2d;
      /** Distance restante annoncee par Nav2, si elle est disponible. */
      readonly distanceRemaining: number | null;
    }
  | { readonly phase: "succeeded"; readonly goal: Pose2d }
  | { readonly phase: "failed"; readonly goal: Pose2d; readonly reason: string };

export interface RobotSnapshot {
  readonly state: RobotStateName | null;
  readonly pose: Pose2d | null;
  readonly velocity: Velocity2d | null;
  readonly battery: BatteryStatus;
  readonly scan: ScanSummary | null;
  readonly navigation: NavigationStatus;
  /** Instant de la derniere donnee recue, au format ISO 8601. */
  readonly updatedAt: string | null;
}

/** Instantane d'un robot dont on n'a encore rien recu. */
export function emptySnapshot(): RobotSnapshot {
  return {
    state: null,
    pose: null,
    velocity: null,
    battery: { instrumented: false },
    scan: null,
    navigation: { phase: "idle" },
    updatedAt: null,
  };
}

/**
 * Valide un nom d'etat venu du topic `~/state`.
 *
 * Un nom inconnu rend `null` : une version du coeur plus recente que l'API ne doit pas
 * faire afficher un etat que le dashboard ne sait pas interpreter.
 */
export function parseRobotState(raw: unknown): RobotStateName | null {
  if (typeof raw !== "string") {
    return null;
  }

  return (ROBOT_STATES as readonly string[]).includes(raw) ? (raw as RobotStateName) : null;
}

/** Lit un champ numerique fini, ou `null`. */
function finiteField(source: unknown, key: string): number | null {
  if (typeof source !== "object" || source === null) {
    return null;
  }

  const value = (source as Record<string, unknown>)[key];

  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** Descend d'un niveau dans un objet, ou `null`. */
function child(source: unknown, key: string): unknown {
  if (typeof source !== "object" || source === null) {
    return null;
  }

  return (source as Record<string, unknown>)[key] ?? null;
}

/** Extrait le cap d'un quaternion, en radians. */
export function yawFromQuaternion(raw: unknown): number | null {
  const x = finiteField(raw, "x");
  const y = finiteField(raw, "y");
  const z = finiteField(raw, "z");
  const w = finiteField(raw, "w");

  if (x === null || y === null || z === null || w === null) {
    return null;
  }

  return Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z));
}

/** Traduit un `nav_msgs/Odometry` en pose planaire. */
export function poseFromOdometry(raw: unknown): Pose2d | null {
  const pose = child(child(raw, "pose"), "pose");
  const position = child(pose, "position");

  const x = finiteField(position, "x");
  const y = finiteField(position, "y");
  const theta = yawFromQuaternion(child(pose, "orientation"));

  if (x === null || y === null || theta === null) {
    return null;
  }

  return { x, y, theta };
}

/** Traduit un `geometry_msgs/Twist` en consigne planaire. */
export function velocityFromTwist(raw: unknown): Velocity2d | null {
  // Une base a entrainement differentiel n'a que deux degres de liberte : les quatre
  // autres composantes sont ignorees, pas rejetees. Un emetteur qui les remplirait se
  // tromperait de type de robot, mais ce n'est pas a l'API d'en juger.
  const linear = finiteField(child(raw, "linear"), "x");
  const angular = finiteField(child(raw, "angular"), "z");

  if (linear === null || angular === null) {
    return null;
  }

  return { linear, angular };
}

/** Resume un `sensor_msgs/LaserScan`. */
export function scanSummary(raw: unknown): ScanSummary | null {
  const ranges = child(raw, "ranges");
  const rangeMin = finiteField(raw, "range_min");
  const rangeMax = finiteField(raw, "range_max");

  if (!Array.isArray(ranges) || rangeMin === null || rangeMax === null) {
    return null;
  }

  // Un LiDAR annonce l'absence d'echo par `inf`, et parfois par `NaN`. Une mesure sous la
  // zone aveugle ou au-dela de la portee ne veut rien dire non plus. Les unes comme les
  // autres sont ecartees du calcul plutot que traitees comme des distances.
  let closest: number | null = null;
  for (const measure of ranges) {
    if (typeof measure !== "number" || !Number.isFinite(measure)) {
      continue;
    }
    if (measure < rangeMin || measure > rangeMax) {
      continue;
    }
    if (closest === null || measure < closest) {
      closest = measure;
    }
  }

  return { samples: ranges.length, closest, rangeMin, rangeMax };
}
