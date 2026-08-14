"""Lance la simulation complete : Gazebo, le robot, le pont et le coeur Rust.

Chaine de commande, de bout en bout :

    teleop --> /cmd_vel --> robot_core --> robot-safety --> ~/cmd_vel_safe
                                                                 |
                                                            ros_gz_bridge
                                                                 |
                                                     gz /roboto/cmd_vel --> roues

Le noeud tourne en `use_sim_time` : sa reference temporelle est le topic `/clock` publie
par Gazebo, et non l'horloge monotone du systeme. Voir
docs/architecture/0006-horloge-monotone-et-non-murale.md.
"""

import os

from launch import LaunchDescription
from launch.actions import DeclareLaunchArgument, ExecuteProcess
from launch.substitutions import Command, LaunchConfiguration
from launch_ros.actions import Node
from launch_ros.parameter_descriptions import ParameterValue

SIMULATION_DIR = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))


def generate_launch_description() -> LaunchDescription:
    world = LaunchConfiguration("world")

    description_file = os.path.join(SIMULATION_DIR, "description", "roboto.urdf.xacro")
    bridge_config = os.path.join(SIMULATION_DIR, "config", "ros_gz_bridge.yaml")

    robot_description = ParameterValue(
        Command(["xacro ", description_file]), value_type=str
    )

    use_sim_time = {"use_sim_time": True}

    return LaunchDescription(
        [
            DeclareLaunchArgument(
                "world",
                default_value=os.path.join(SIMULATION_DIR, "worlds", "indoor.sdf"),
                description="Monde SDF a charger.",
            ),
            # `-s` demarre le serveur seul, sans interface graphique, et `-r` lance la
            # simulation immediatement. Meme sans interface, le LiDAR et la camera
            # passent par le pipeline de rendu : en Harmonic il n'existe pas de LiDAR CPU.
            ExecuteProcess(
                cmd=["gz", "sim", "-s", "-r", "-v", "2", world],
                output="screen",
            ),
            # Publie /robot_description et les TF statiques du modele.
            Node(
                package="robot_state_publisher",
                executable="robot_state_publisher",
                parameters=[{"robot_description": robot_description}, use_sim_time],
                output="screen",
            ),
            # Insere le robot dans le monde a partir de /robot_description.
            Node(
                package="ros_gz_sim",
                executable="create",
                arguments=[
                    "-topic",
                    "/robot_description",
                    "-name",
                    "roboto",
                    "-x",
                    "0.0",
                    "-y",
                    "0.0",
                    "-z",
                    "0.1",
                ],
                output="screen",
            ),
            Node(
                package="ros_gz_bridge",
                executable="parameter_bridge",
                parameters=[{"config_file": bridge_config}, use_sim_time],
                output="screen",
            ),
            Node(
                package="robot_ros2",
                executable="robot_node",
                parameters=[use_sim_time],
                output="screen",
            ),
        ]
    )
