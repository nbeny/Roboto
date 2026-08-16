import { describe, expect, it } from "vitest";

import { visionSceneFromMessage } from "../src/robot/snapshot.ts";

/** Construit un `std_msgs/String` portant une scene JSON. */
function published(payload: unknown) {
  return { data: JSON.stringify(payload) };
}

const SCENE = {
  detections: [
    { label: "marqueur 7", confidence: 1, bearing: 0.42, x: 10, y: 20, width: 100, height: 100 },
  ],
  width: 640,
  height: 480,
  at: "2026-08-16T10:00:00+00:00",
  description: "La camera voit : marqueur 7 sur la gauche.",
};

describe("visionSceneFromMessage", () => {
  it("extrait les detections et la description", () => {
    const scene = visionSceneFromMessage(published(SCENE));

    expect(scene?.detections).toHaveLength(1);
    expect(scene?.detections[0]?.label).toBe("marqueur 7");
    expect(scene?.detections[0]?.bearing).toBeCloseTo(0.42, 6);
    expect(scene?.description).toContain("marqueur 7");
  });

  it("accepte une scene sans aucune detection", () => {
    // Ne rien reconnaitre est un resultat, pas une panne.
    const scene = visionSceneFromMessage(
      published({ ...SCENE, detections: [], description: "rien" }),
    );

    expect(scene?.detections).toEqual([]);
  });

  it("ignore une charge utile qui n'est pas du JSON", () => {
    // La perception est une information d'appoint : une scene illisible ne doit pas
    // faire tomber la lecture de l'etat du robot.
    expect(visionSceneFromMessage({ data: "{ceci n'est pas du JSON" })).toBeNull();
  });

  it("ignore un message sans champ data", () => {
    expect(visionSceneFromMessage({})).toBeNull();
    expect(visionSceneFromMessage(null)).toBeNull();
  });

  it("ignore une scene sans description exploitable", () => {
    expect(visionSceneFromMessage(published({ detections: [] }))).toBeNull();
  });

  it("ecarte une detection sans etiquette plutot que la scene entiere", () => {
    const scene = visionSceneFromMessage(
      published({ ...SCENE, detections: [{ confidence: 1 }, SCENE.detections[0]] }),
    );

    expect(scene?.detections).toHaveLength(1);
    expect(scene?.detections[0]?.label).toBe("marqueur 7");
  });

  it("accepte un gisement absent", () => {
    const scene = visionSceneFromMessage(
      published({ ...SCENE, detections: [{ label: "marqueur 3", confidence: 0.9 }] }),
    );

    expect(scene?.detections[0]?.bearing).toBeNull();
  });
});
