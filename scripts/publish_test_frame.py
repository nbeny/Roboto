#!/usr/bin/env python3
"""Publie une image de synthèse portant un marqueur ArUco, comme le ferait une caméra.

Sert à vérifier le nœud de vision sans Gazebo : l'image est fabriquée ici, donc on sait
exactement ce qui doit être détecté. Cela exerce le vrai chemin —
`sensor_msgs/Image` -> conversion -> détection -> `/vision/scene` — et notamment la
conversion maison, qui est la partie la plus facile à casser.

    python3 scripts/publish_test_frame.py --marker 7 --side left
"""

from __future__ import annotations

import argparse
import sys

import cv2
import numpy as np
import rclpy
from rclpy.node import Node
from rclpy.qos import QoSHistoryPolicy, QoSProfile, QoSReliabilityPolicy
from sensor_msgs.msg import Image

WIDTH = 640
HEIGHT = 480


def build_frame(marker_id: int, side: str) -> np.ndarray:
    from roboto_vision.compat import generate_marker

    image = np.full((HEIGHT, WIDTH), 255, dtype=np.uint8)
    size = 160

    centre = {"left": WIDTH // 4, "centre": WIDTH // 2, "right": 3 * WIDTH // 4}[side]
    left = centre - size // 2
    top = HEIGHT // 2 - size // 2

    image[top : top + size, left : left + size] = generate_marker(
        cv2.aruco.DICT_4X4_50, marker_id, size
    )

    return cv2.cvtColor(image, cv2.COLOR_GRAY2RGB)


class FramePublisher(Node):
    def __init__(self, frame: np.ndarray, topic: str) -> None:
        super().__init__("test_frame_publisher")

        qos = QoSProfile(
            reliability=QoSReliabilityPolicy.BEST_EFFORT,
            history=QoSHistoryPolicy.KEEP_LAST,
            depth=1,
        )
        self._publisher = self.create_publisher(Image, topic, qos)
        self._frame = frame
        self.create_timer(0.2, self._publish)

    def _publish(self) -> None:
        message = Image()
        message.header.frame_id = "camera_link"
        message.header.stamp = self.get_clock().now().to_msg()
        message.height = self._frame.shape[0]
        message.width = self._frame.shape[1]
        message.encoding = "rgb8"
        message.is_bigendian = 0
        message.step = self._frame.shape[1] * 3
        message.data = self._frame.tobytes()

        self._publisher.publish(message)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--marker", type=int, default=7)
    parser.add_argument("--side", choices=["left", "centre", "right"], default="left")
    parser.add_argument("--topic", default="/camera/image_raw")
    args = parser.parse_args()

    frame = build_frame(args.marker, args.side)

    rclpy.init()
    node = FramePublisher(frame, args.topic)
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()

    return 0


if __name__ == "__main__":
    sys.exit(main())
