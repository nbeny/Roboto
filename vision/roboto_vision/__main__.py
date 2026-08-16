"""Analyse une image depuis la ligne de commande.

Sert à vérifier la vision sans robot : on lui donne une photo ou un marqueur imprimé,
elle dit ce qu'elle voit.
"""

from __future__ import annotations

import argparse
import json
import sys

import cv2

from .compat import generate_marker
from .fiducials import FiducialDetector


def main() -> int:
    parser = argparse.ArgumentParser(prog="roboto-vision", description=__doc__)
    parser.add_argument("image", nargs="?", help="chemin d'une image à analyser")
    parser.add_argument(
        "--make-marker",
        type=int,
        metavar="ID",
        help="génère un marqueur ArUco à imprimer, et sort",
    )
    parser.add_argument("--out", default="marqueur.png", help="fichier du marqueur généré")
    parser.add_argument("--json", action="store_true", help="sortie brute plutôt qu'une phrase")
    args = parser.parse_args()

    if args.make_marker is not None:
        cv2.imwrite(
            args.out, generate_marker(cv2.aruco.DICT_4X4_50, args.make_marker, 600)
        )
        print(f"marqueur {args.make_marker} écrit dans {args.out} — à imprimer tel quel")
        return 0

    if not args.image:
        parser.error("donne une image à analyser, ou utilise --make-marker")

    image = cv2.imread(args.image)
    if image is None:
        print(f"image illisible : {args.image}", file=sys.stderr)
        return 1

    summary = FiducialDetector().analyse(image)

    if args.json:
        print(json.dumps(summary.to_dict(), ensure_ascii=False, indent=2))
    else:
        print(summary.describe())

    return 0


if __name__ == "__main__":
    sys.exit(main())
