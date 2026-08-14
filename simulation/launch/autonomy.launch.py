"""Simulation complete, plus cartographie et navigation autonome.

Chaine de commande, de bout en bout :

    Nav2 ──► velocity_smoother ──► /nav/cmd_vel ──► robot_core ──► robot-safety
                                                                        │
                                                                  ~/cmd_vel_safe
                                                                        │
                                                                  ros_gz_bridge
                                                                        │
                                                            gz /roboto/cmd_vel ──► roues

Nav2 publie sur `/nav/cmd_vel` et non sur `/cmd_vel` : c'est le topic d'arrivee qui
determine la source de la commande, donc son autorite. Une consigne arrivant sur ce topic
est traitee comme `CommandSource::Navigation`, et n'est acceptee qu'en etat `NAVIGATING`.
La teleoperation garde `/cmd_vel` et son autorite propre.

Les limites de vitesse de Nav2 sont strictement a l'interieur de l'enveloppe de
`robot-safety` — voir l'en-tete de `config/nav2_params.yaml` pour la raison.
"""

import os

from launch import LaunchDescription
from launch.actions import (
    DeclareLaunchArgument,
    ExecuteProcess,
    IncludeLaunchDescription,
    TimerAction,
)
from launch.conditions import IfCondition
from launch.launch_description_sources import PythonLaunchDescriptionSource
from launch.substitutions import LaunchConfiguration
from launch_ros.actions import Node

SIMULATION_DIR = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))

#: Serveurs Nav2 pilotes par le gestionnaire de cycle de vie, dans l'ordre de demarrage.
NAVIGATION_NODES = [
    "controller_server",
    "smoother_server",
    "planner_server",
    "behavior_server",
    "bt_navigator",
    "waypoint_follower",
    "velocity_smoother",
]


def generate_launch_description() -> LaunchDescription:
    nav2_params = os.path.join(SIMULATION_DIR, "config", "nav2_params.yaml")
    slam_params = os.path.join(SIMULATION_DIR, "config", "slam_toolbox.yaml")

    use_sim_time = {"use_sim_time": True}
    autostart = LaunchConfiguration("autostart")

    # Toutes les consignes de Nav2 — celles du controleur comme celles des manoeuvres de
    # degagement — passent par le lisseur, qui les depose sur `/nav/cmd_vel`. Un seul
    # point de sortie, donc aucun chemin detourne vers les roues.
    nav_cmd_vel = [("cmd_vel", "cmd_vel_nav")]

    return LaunchDescription(
        [
            DeclareLaunchArgument(
                "autostart",
                default_value="true",
                description="Faire passer les serveurs Nav2 a l'etat actif automatiquement.",
            ),
            DeclareLaunchArgument(
                "request_navigating",
                default_value="true",
                description=(
                    "Demander au coeur de passer en NAVIGATING au demarrage. Sans cette "
                    "transition, le coeur refuse les consignes de Nav2 : c'est voulu, "
                    "mais cela se diagnostique mal."
                ),
            ),
            DeclareLaunchArgument(
                "rosbridge",
                default_value="true",
                description="Demarrer la passerelle WebSocket utilisee par l'API.",
            ),
            IncludeLaunchDescription(
                PythonLaunchDescriptionSource(
                    os.path.join(SIMULATION_DIR, "launch", "simulation.launch.py")
                )
            ),
            # --- Cartographie ---------------------------------------------------------
            Node(
                package="slam_toolbox",
                executable="async_slam_toolbox_node",
                name="slam_toolbox",
                parameters=[slam_params],
                output="screen",
            ),
            # `slam_toolbox` est un noeud a cycle de vie : sans gestionnaire pour le
            # configurer puis l'activer, il reste en `unconfigured` et ne publie rien.
            # Un gestionnaire distinct de celui de Nav2, pour que la carte existe avant
            # que les cartes de cout ne la reclament.
            Node(
                package="nav2_lifecycle_manager",
                executable="lifecycle_manager",
                name="lifecycle_manager_slam",
                parameters=[
                    use_sim_time,
                    {"autostart": autostart},
                    {"node_names": ["slam_toolbox"]},
                ],
                output="screen",
            ),
            # --- Navigation -----------------------------------------------------------
            Node(
                package="nav2_controller",
                executable="controller_server",
                name="controller_server",
                parameters=[nav2_params],
                remappings=nav_cmd_vel,
                output="screen",
            ),
            Node(
                package="nav2_smoother",
                executable="smoother_server",
                name="smoother_server",
                parameters=[nav2_params],
                output="screen",
            ),
            Node(
                package="nav2_planner",
                executable="planner_server",
                name="planner_server",
                parameters=[nav2_params],
                output="screen",
            ),
            Node(
                package="nav2_behaviors",
                executable="behavior_server",
                name="behavior_server",
                parameters=[nav2_params],
                remappings=nav_cmd_vel,
                output="screen",
            ),
            Node(
                package="nav2_bt_navigator",
                executable="bt_navigator",
                name="bt_navigator",
                parameters=[nav2_params],
                output="screen",
            ),
            Node(
                package="nav2_waypoint_follower",
                executable="waypoint_follower",
                name="waypoint_follower",
                parameters=[nav2_params],
                output="screen",
            ),
            Node(
                package="nav2_velocity_smoother",
                executable="velocity_smoother",
                name="velocity_smoother",
                parameters=[nav2_params],
                remappings=[
                    ("cmd_vel", "cmd_vel_nav"),
                    ("cmd_vel_smoothed", "/nav/cmd_vel"),
                ],
                output="screen",
            ),
            Node(
                package="nav2_lifecycle_manager",
                executable="lifecycle_manager",
                name="lifecycle_manager_navigation",
                parameters=[
                    use_sim_time,
                    {"autostart": autostart},
                    {"node_names": NAVIGATION_NODES},
                ],
                output="screen",
            ),
            # Passerelle JSON/WebSocket : c'est par la que l'API TypeScript parle a ROS 2.
            # Elle reste optionnelle — la simulation et la navigation fonctionnent sans.
            Node(
                package="rosbridge_server",
                executable="rosbridge_websocket",
                name="rosbridge_websocket",
                parameters=[use_sim_time, {"port": 9090}],
                output="screen",
                condition=IfCondition(LaunchConfiguration("rosbridge")),
            ),
            # --- Autorisation du mouvement ---------------------------------------------
            # Le coeur n'accepte les consignes de navigation qu'en etat NAVIGATING. Sans
            # cette demande, Nav2 planifierait, commanderait, et le robot resterait
            # immobile sans autre trace qu'une ligne de journal en niveau debug.
            #
            # La transition reste explicite dans le coeur : c'est ce lancement qui la
            # demande, parce que demarrer la pile d'autonomie signifie precisement qu'on
            # veut naviguer.
            TimerAction(
                period=12.0,
                actions=[
                    ExecuteProcess(
                        cmd=[
                            "ros2",
                            "topic",
                            "pub",
                            "--once",
                            "/robot_core/request_state",
                            "std_msgs/msg/String",
                            "{data: 'NAVIGATING'}",
                        ],
                        output="screen",
                    )
                ],
            ),
        ]
    )
