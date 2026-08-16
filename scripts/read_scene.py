#!/usr/bin/env python3
"""Lit une description de scène et l'affiche sous une forme exploitable par un script.

Écrit parce qu'un essai qui cherche un mot dans la sortie YAML de `ros2 topic echo`
vérifie la mise en forme d'un afficheur, pas la donnée. Ici, le JSON est analysé, et le
script s'exprime en valeurs : étiquette, gisement, côté.

    python3 scripts/read_scene.py --timeout 30
    # marqueur 7|0.2626|gauche
"""

from __future__ import annotations

import argparse
import json
import sys

import rclpy
from rclpy.node import Node
from rclpy.qos import QoSDurabilityPolicy, QoSHistoryPolicy, QoSProfile, QoSReliabilityPolicy
from std_msgs.msg import String


class SceneReader(Node):
    def __init__(self, topic: str) -> None:
        super().__init__("scene_reader")
        self.scene: dict | None = None

        qos = QoSProfile(
            reliability=QoSReliabilityPolicy.RELIABLE,
            durability=QoSDurabilityPolicy.TRANSIENT_LOCAL,
            history=QoSHistoryPolicy.KEEP_LAST,
            depth=1,
        )
        self.create_subscription(String, topic, self._on_scene, qos)

    def _on_scene(self, message: String) -> None:
        try:
            self.scene = json.loads(message.data)
        except json.JSONDecodeError:
            self.scene = None


def side_of(bearing: float | None) -> str:
    if bearing is None:
        return "indetermine"
    if bearing > 0.09:
        return "gauche"
    if bearing < -0.09:
        return "droite"
    return "face"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--topic", default="/vision/scene")
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument(
        "--expect", help="n'accepter que si cette étiquette est présente"
    )
    args = parser.parse_args()

    rclpy.init()
    node = SceneReader(args.topic)

    deadline = node.get_clock().now().nanoseconds + int(args.timeout * 1e9)
    result: dict | None = None

    try:
        while node.get_clock().now().nanoseconds < deadline:
            rclpy.spin_once(node, timeout_sec=0.5)

            scene = node.scene
            if not scene or not scene.get("detections"):
                continue
            if args.expect and not any(
                args.expect in d.get("label", "") for d in scene["detections"]
            ):
                continue

            result = scene
            break
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()

    if result is None:
        print("aucune scène exploitable", file=sys.stderr)
        return 1

    for found in result["detections"]:
        bearing = found.get("bearing")
        print(f"{found.get('label')}|{bearing}|{side_of(bearing)}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
