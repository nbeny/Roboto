"use client";

import { useCallback, useEffect, useRef } from "react";

import type { NavigationStatus, OccupancyMap, Pose2d } from "@/lib/api";

/**
 * Grille d'occupation, robot, et but courant.
 *
 * Le rendu se fait sur un canvas plutot qu'en DOM : une carte fait des dizaines de
 * milliers de cellules, et autant d'elements mettraient un navigateur a genoux.
 *
 * Le clic envoie un but de navigation. C'est le seul geste du tableau de bord qui met le
 * robot en mouvement, et il traverse la meme chaine que tout le reste : Nav2, puis
 * `robot-safety`, qui garde le dernier mot.
 */

interface MapViewProps {
  readonly map: OccupancyMap | null;
  readonly pose: Pose2d | null;
  readonly navigation: NavigationStatus;
  readonly onGoal: (x: number, y: number) => void;
}

/** Palette de la grille : inconnu, libre, occupe. */
const UNKNOWN = "#0c100f";
const FREE = "#161e1b";
const OCCUPIED = "#5d7a70";

export function MapView({ map, pose, navigation, onGoal }: MapViewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !map) {
      return;
    }

    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }

    canvas.width = map.width;
    canvas.height = map.height;

    const image = context.createImageData(map.width, map.height);

    for (let index = 0; index < map.cells.length; index += 1) {
      const occupancy = map.cells[index] ?? -1;

      // La grille ROS a son origine en bas a gauche, le canvas en haut a gauche :
      // sans ce retournement, la carte serait un miroir vertical de la realite.
      const row = map.height - 1 - Math.floor(index / map.width);
      const column = index % map.width;
      const pixel = (row * map.width + column) * 4;

      const colour = occupancy < 0 ? UNKNOWN : occupancy > 50 ? OCCUPIED : FREE;
      image.data[pixel] = parseInt(colour.slice(1, 3), 16);
      image.data[pixel + 1] = parseInt(colour.slice(3, 5), 16);
      image.data[pixel + 2] = parseInt(colour.slice(5, 7), 16);
      image.data[pixel + 3] = 255;
    }

    context.putImageData(image, 0, 0);
  }, [map]);

  useEffect(draw, [draw]);

  /** Convertit une coordonnee du repere `map` en pixel du canvas. */
  const toPixel = useCallback(
    (x: number, y: number): { left: number; top: number } | null => {
      if (!map) {
        return null;
      }
      return {
        left: ((x - map.origin.x) / map.resolution / map.width) * 100,
        top: (1 - (y - map.origin.y) / map.resolution / map.height) * 100,
      };
    },
    [map],
  );

  const handleClick = useCallback(
    (event: React.MouseEvent<HTMLCanvasElement>) => {
      if (!map) {
        return;
      }

      const bounds = event.currentTarget.getBoundingClientRect();
      const fractionX = (event.clientX - bounds.left) / bounds.width;
      const fractionY = (event.clientY - bounds.top) / bounds.height;

      onGoal(
        map.origin.x + fractionX * map.width * map.resolution,
        map.origin.y + (1 - fractionY) * map.height * map.resolution,
      );
    },
    [map, onGoal],
  );

  if (!map) {
    return (
      <p className="map-empty">
        aucune carte
        <br />
        <span style={{ color: "var(--ink-faint)", fontSize: "10px" }}>
          la cartographie n&apos;a pas encore publie
        </span>
      </p>
    );
  }

  const robot = pose ? toPixel(pose.x, pose.y) : null;
  const goal =
    navigation.phase === "active" || navigation.phase === "succeeded"
      ? toPixel(navigation.goal.x, navigation.goal.y)
      : null;

  return (
    <div style={{ position: "relative", width: "100%", height: "100%" }}>
      <canvas ref={canvasRef} className="map-canvas" onClick={handleClick} />

      {goal && (
        <svg
          viewBox="0 0 24 24"
          aria-hidden
          style={{
            position: "absolute",
            left: `${goal.left}%`,
            top: `${goal.top}%`,
            width: 22,
            height: 22,
            transform: "translate(-50%, -50%)",
            pointerEvents: "none",
          }}
        >
          <circle cx="12" cy="12" r="8" fill="none" stroke="var(--accent)" strokeWidth="1.5" />
          <path d="M12 1v6M12 17v6M1 12h6M17 12h6" stroke="var(--accent)" strokeWidth="1.5" />
        </svg>
      )}

      {robot && (
        <svg
          viewBox="0 0 24 24"
          aria-label="position du robot"
          style={{
            position: "absolute",
            left: `${robot.left}%`,
            top: `${robot.top}%`,
            width: 26,
            height: 26,
            // Le repere ROS tourne dans le sens trigonometrique, l'ecran dans l'autre.
            transform: `translate(-50%, -50%) rotate(${-(pose?.theta ?? 0) * (180 / Math.PI)}deg)`,
            pointerEvents: "none",
          }}
        >
          <circle cx="12" cy="12" r="10" fill="var(--accent-glow)" />
          {/* Une fleche, pas un point : l'orientation est une information. */}
          <path d="M12 3 L18 19 L12 15 L6 19 Z" fill="var(--accent)" />
        </svg>
      )}

      <span className="map-hint">clic — envoyer un but</span>
    </div>
  );
}
