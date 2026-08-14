import { describe, expect, it } from "vitest";

import {
  emptySnapshot,
  parseRobotState,
  poseFromOdometry,
  scanSummary,
  velocityFromTwist,
  yawFromQuaternion,
} from "../src/robot/snapshot.ts";

/** Construit un `nav_msgs/Odometry` minimal mais realiste. */
function odometry(x: number, y: number, quaternion: Record<string, number>) {
  return {
    pose: { pose: { position: { x, y, z: 0 }, orientation: quaternion } },
    twist: { twist: { linear: { x: 0, y: 0, z: 0 }, angular: { x: 0, y: 0, z: 0 } } },
  };
}

/** Construit un `sensor_msgs/LaserScan` minimal. */
function laserScan(ranges: number[], rangeMin = 0.05, rangeMax = 12) {
  return { ranges, range_min: rangeMin, range_max: rangeMax };
}

describe("etat initial", () => {
  it("ne pretend rien connaitre avant d'avoir recu quoi que ce soit", () => {
    const snapshot = emptySnapshot();

    expect(snapshot.state).toBeNull();
    expect(snapshot.pose).toBeNull();
    expect(snapshot.velocity).toBeNull();
    expect(snapshot.scan).toBeNull();
    expect(snapshot.updatedAt).toBeNull();
  });

  it("annonce la batterie comme non instrumentee plutot que d'inventer un pourcentage", () => {
    expect(emptySnapshot().battery).toEqual({ instrumented: false });
  });
});

describe("parseRobotState", () => {
  it("accepte les huit etats du coeur", () => {
    for (const state of [
      "BOOTING",
      "IDLE",
      "TELEOPERATION",
      "NAVIGATING",
      "PAUSED",
      "ERROR",
      "SAFE_STOP",
      "CHARGING",
    ]) {
      expect(parseRobotState(state)).toBe(state);
    }
  });

  it("refuse un etat inconnu au lieu de le laisser passer", () => {
    // Un coeur plus recent que l'API pourrait annoncer un etat que le dashboard ne sait
    // pas interpreter. Mieux vaut n'afficher rien que quelque chose de faux.
    expect(parseRobotState("DOCKING")).toBeNull();
    expect(parseRobotState("idle")).toBeNull();
    expect(parseRobotState("")).toBeNull();
  });

  it("refuse ce qui n'est pas une chaine", () => {
    expect(parseRobotState(42)).toBeNull();
    expect(parseRobotState(null)).toBeNull();
    expect(parseRobotState(undefined)).toBeNull();
    expect(parseRobotState({ data: "IDLE" })).toBeNull();
  });
});

describe("yawFromQuaternion", () => {
  it("rend un cap nul pour l'identite", () => {
    expect(yawFromQuaternion({ x: 0, y: 0, z: 0, w: 1 })).toBeCloseTo(0, 10);
  });

  it("rend un quart de tour", () => {
    const half = Math.SQRT1_2;
    expect(yawFromQuaternion({ x: 0, y: 0, z: half, w: half })).toBeCloseTo(Math.PI / 2, 6);
  });

  it("rend un quart de tour negatif", () => {
    const half = Math.SQRT1_2;
    expect(yawFromQuaternion({ x: 0, y: 0, z: -half, w: half })).toBeCloseTo(-Math.PI / 2, 6);
  });

  it("refuse un quaternion incomplet", () => {
    expect(yawFromQuaternion({ x: 0, y: 0, z: 0 })).toBeNull();
    expect(yawFromQuaternion(null)).toBeNull();
    expect(yawFromQuaternion("0,0,0,1")).toBeNull();
  });

  it("refuse des composantes non finies", () => {
    expect(yawFromQuaternion({ x: 0, y: 0, z: Number.NaN, w: 1 })).toBeNull();
  });
});

describe("poseFromOdometry", () => {
  it("extrait la position et le cap", () => {
    const pose = poseFromOdometry(odometry(1.5, -0.75, { x: 0, y: 0, z: 0, w: 1 }));

    expect(pose).not.toBeNull();
    expect(pose?.x).toBeCloseTo(1.5, 10);
    expect(pose?.y).toBeCloseTo(-0.75, 10);
    expect(pose?.theta).toBeCloseTo(0, 10);
  });

  it("refuse un message tronque", () => {
    expect(poseFromOdometry({})).toBeNull();
    expect(poseFromOdometry({ pose: {} })).toBeNull();
    expect(poseFromOdometry({ pose: { pose: { position: { x: 1 } } } })).toBeNull();
  });

  it("refuse une position non finie", () => {
    expect(poseFromOdometry(odometry(Number.NaN, 0, { x: 0, y: 0, z: 0, w: 1 }))).toBeNull();
  });
});

describe("velocityFromTwist", () => {
  it("ne retient que les deux degres de liberte d'une base differentielle", () => {
    const velocity = velocityFromTwist({
      linear: { x: 0.35, y: 9, z: 9 },
      angular: { x: 9, y: 9, z: -0.4 },
    });

    expect(velocity).toEqual({ linear: 0.35, angular: -0.4 });
  });

  it("refuse un message tronque ou non fini", () => {
    expect(velocityFromTwist({ linear: { x: 1 } })).toBeNull();
    expect(velocityFromTwist({ linear: { x: Number.NaN }, angular: { z: 0 } })).toBeNull();
    expect(velocityFromTwist(null)).toBeNull();
  });
});

describe("scanSummary", () => {
  it("compte les mesures et trouve la plus proche", () => {
    const summary = scanSummary(laserScan([2.5, 1.2, 3.8]));

    expect(summary?.samples).toBe(3);
    expect(summary?.closest).toBeCloseTo(1.2, 10);
  });

  it("ignore les mesures infinies", () => {
    // Un LiDAR annonce l'absence d'echo par `inf`. Les compter comme des distances
    // donnerait une portee libre absurde ; les traiter comme des obstacles serait pire.
    const summary = scanSummary(laserScan([Number.POSITIVE_INFINITY, 2.0, Number.POSITIVE_INFINITY]));

    expect(summary?.samples).toBe(3);
    expect(summary?.closest).toBeCloseTo(2.0, 10);
  });

  it("ignore les mesures hors de la portee annoncee", () => {
    // En deca de la zone aveugle, la mesure ne veut rien dire.
    const summary = scanSummary(laserScan([0.01, 2.0, 50], 0.05, 12));

    expect(summary?.closest).toBeCloseTo(2.0, 10);
  });

  it("rend un obstacle absent quand rien n'est visible", () => {
    const summary = scanSummary(laserScan([Number.POSITIVE_INFINITY, Number.NaN]));

    expect(summary?.samples).toBe(2);
    expect(summary?.closest).toBeNull();
  });

  it("supporte un balayage vide", () => {
    const summary = scanSummary(laserScan([]));

    expect(summary?.samples).toBe(0);
    expect(summary?.closest).toBeNull();
  });

  it("refuse un message qui n'est pas un balayage", () => {
    expect(scanSummary({ ranges: "beaucoup" })).toBeNull();
    expect(scanSummary({})).toBeNull();
    expect(scanSummary(null)).toBeNull();
  });
});
