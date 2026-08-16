"""Lit la caméra, publie ce qu'elle reconnaît.

Ce nœud **ne commande rien**. Il n'a aucun éditeur de vitesse, aucun client de service
de mouvement. Il décrit ce qu'il voit sur `/vision/scene` ; ce que le robot en fait est
décidé ailleurs, et borné par `robot-safety`.

La description part en JSON sur un `std_msgs/String`, comme `~/request_state` avant
elle : générer un paquet d'interfaces sur mesure pour une charge utile qui finit de
toute façon en JSON dans une réponse HTTP coûterait plus qu'il ne rapporte.
"""

from __future__ import annotations

import json
import os
import sys

import numpy as np
import rclpy
from rclpy.node import Node
from rclpy.qos import QoSDurabilityPolicy, QoSHistoryPolicy, QoSProfile, QoSReliabilityPolicy
from sensor_msgs.msg import Image
from std_msgs.msg import String


def _import_vision():
    """Importe `roboto_vision`, en aidant à le trouver s'il n'est pas installé.

    Le paquet vit hors de l'espace colcon, à la racine du dépôt. Plutôt que d'exiger un
    `pip install` avant toute simulation, on accepte un chemin explicite — et on dit
    clairement quoi faire si rien ne marche.
    """
    extra = os.environ.get("ROBOTO_VISION_PATH")
    if extra and extra not in sys.path:
        sys.path.insert(0, extra)

    try:
        from roboto_vision.fiducials import FiducialDetector
    except ImportError as error:
        raise SystemExit(
            "Le paquet `roboto_vision` est introuvable. Installe-le "
            "(`pip install -e vision/`) ou indique son chemin par la variable "
            f"ROBOTO_VISION_PATH. Détail : {error}"
        ) from error

    return FiducialDetector


class VisionNode(Node):
    """Abonné caméra, éditeur de descriptions de scène."""

    def __init__(self) -> None:
        super().__init__("vision_node")

        self.declare_parameter("image_topic", "/camera/image_raw")
        self.declare_parameter("scene_topic", "/vision/scene")
        self.declare_parameter("horizontal_fov", 1.047)
        # 2 Hz et non 15 : la détection tourne sur le calculateur du robot, à côté de
        # Nav2 et de SLAM. Analyser chaque trame consommerait un cœur entier pour une
        # information qui ne change pas si vite.
        self.declare_parameter("analysis_rate", 2.0)

        image_topic = self.get_parameter("image_topic").value
        scene_topic = self.get_parameter("scene_topic").value
        rate = float(self.get_parameter("analysis_rate").value)

        detector_class = _import_vision()
        self._detector = detector_class(
            horizontal_fov=float(self.get_parameter("horizontal_fov").value)
        )

        # Le flux d'images est volumineux et périssable : on garde la dernière trame et
        # on jette le reste, plutôt que d'accumuler du retard.
        camera_qos = QoSProfile(
            reliability=QoSReliabilityPolicy.BEST_EFFORT,
            history=QoSHistoryPolicy.KEEP_LAST,
            depth=1,
        )

        # La scène, elle, est petite et se garde : un abonné qui arrive doit trouver la
        # dernière description sans attendre la suivante.
        scene_qos = QoSProfile(
            reliability=QoSReliabilityPolicy.RELIABLE,
            durability=QoSDurabilityPolicy.TRANSIENT_LOCAL,
            history=QoSHistoryPolicy.KEEP_LAST,
            depth=1,
        )

        self._publisher = self.create_publisher(String, scene_topic, scene_qos)
        self._subscription = self.create_subscription(
            Image, image_topic, self._on_image, camera_qos
        )

        self._latest: Image | None = None
        self._timer = self.create_timer(1.0 / max(rate, 0.1), self._analyse)

        self.get_logger().info(
            f"vision : {image_topic} -> {scene_topic}, {rate:.1f} Hz"
        )

    def _on_image(self, message: Image) -> None:
        # On mémorise sans analyser : le travail se fait à cadence maîtrisée dans le
        # minuteur, pas au rythme de la caméra.
        self._latest = message

    def _analyse(self) -> None:
        message = self._latest
        if message is None:
            return

        frame = _to_bgr(message)
        if frame is None:
            self.get_logger().warn(
                f"encodage d'image non pris en charge : {message.encoding}",
                throttle_duration_sec=10.0,
            )
            return

        summary = self._detector.analyse(frame)

        payload = summary.to_dict()
        payload["frame_id"] = message.header.frame_id

        self._publisher.publish(String(data=json.dumps(payload, ensure_ascii=False)))


def _to_bgr(message: Image) -> np.ndarray | None:
    """Convertit un `sensor_msgs/Image` en tableau BGR, sans `cv_bridge`.

    `cv_bridge` n'est pas installé dans l'image de simulation, et la conversion tient en
    quelques lignes pour les encodages qui nous concernent. En ajouter la dépendance
    pour cela serait disproportionné.
    """
    if message.height == 0 or message.width == 0 or not message.data:
        return None

    try:
        flat = np.frombuffer(bytes(message.data), dtype=np.uint8)
    except ValueError:
        return None

    encoding = message.encoding.lower()

    if encoding in ("rgb8", "bgr8"):
        channels = 3
    elif encoding in ("rgba8", "bgra8"):
        channels = 4
    elif encoding == "mono8":
        channels = 1
    else:
        return None

    expected = message.height * message.width * channels
    if flat.size < expected:
        return None

    frame = flat[:expected].reshape(message.height, message.width, channels)

    if encoding.startswith("rgb"):
        # OpenCV travaille en BGR : sans cette inversion, tout est correct sauf les
        # couleurs, ce qui ne se voit pas sur un marqueur noir et blanc mais fausserait
        # n'importe quel détecteur couleur ajouté plus tard.
        frame = frame[:, :, [2, 1, 0]] if channels == 3 else frame[:, :, [2, 1, 0, 3]]

    return np.ascontiguousarray(frame)


def main(args: list[str] | None = None) -> None:
    rclpy.init(args=args)
    node = VisionNode()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
