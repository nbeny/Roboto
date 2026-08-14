"""Agent conversationnel de Roboto.

L'agent ne parle qu'à l'API HTTP du robot. Il n'a aucun accès à ROS 2, aucun moyen
d'émettre une consigne de vitesse, et passe par `robot-safety` pour tout ce qui bouge.
"""

__all__ = ["__version__"]

__version__ = "0.1.0"
