"""Commande vocale de Roboto.

Un ordre d'arrêt ne traverse jamais le modèle de langage : voir `urgent` et `pipeline`.
"""

from .urgent import is_emergency, normalise

__all__ = ["is_emergency", "normalise", "__version__"]

__version__ = "0.1.0"
