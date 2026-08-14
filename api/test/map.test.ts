import { describe, expect, it } from "vitest";

import { occupancyMapFromMessage } from "../src/robot/snapshot.ts";

/** Construit un `nav_msgs/OccupancyGrid` minimal. */
function grid(width: number, height: number, cells: number[]) {
  return {
    info: {
      width,
      height,
      resolution: 0.05,
      origin: {
        position: { x: -4, y: -3, z: 0 },
        orientation: { x: 0, y: 0, z: 0, w: 1 },
      },
    },
    data: cells,
  };
}

describe("occupancyMapFromMessage", () => {
  it("extrait la geometrie et les cellules", () => {
    const map = occupancyMapFromMessage(grid(2, 2, [0, 100, -1, 0]));

    expect(map).not.toBeNull();
    expect(map?.width).toBe(2);
    expect(map?.height).toBe(2);
    expect(map?.resolution).toBeCloseTo(0.05, 10);
    expect(map?.origin.x).toBeCloseTo(-4, 10);
    expect(map?.cells).toEqual([0, 100, -1, 0]);
  });

  it("refuse une carte dont la taille ne correspond pas au contenu", () => {
    // Une carte decalee est pire qu'une carte absente : elle a l'air juste.
    expect(occupancyMapFromMessage(grid(3, 3, [0, 0, 0]))).toBeNull();
  });

  it("refuse un message tronque", () => {
    expect(occupancyMapFromMessage({})).toBeNull();
    expect(occupancyMapFromMessage({ info: { width: 2, height: 2 } })).toBeNull();
    expect(occupancyMapFromMessage(null)).toBeNull();
  });

  it("accepte une carte entierement inconnue", () => {
    // Au demarrage de la cartographie, toutes les cellules valent -1. C'est une carte
    // valide, simplement vide.
    const map = occupancyMapFromMessage(grid(2, 1, [-1, -1]));

    expect(map?.cells).toEqual([-1, -1]);
  });
});
