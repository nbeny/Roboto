from setuptools import find_packages, setup

package_name = "robot_vision"

setup(
    name=package_name,
    version="0.1.0",
    packages=find_packages(exclude=["test"]),
    data_files=[
        ("share/ament_index/resource_index/packages", ["resource/" + package_name]),
        ("share/" + package_name, ["package.xml"]),
    ],
    install_requires=["setuptools"],
    zip_safe=True,
    maintainer="Nicolas BENY",
    maintainer_email="ynebocin@gmail.com",
    description="Nœud de perception visuelle de Roboto.",
    license="Apache-2.0",
    entry_points={
        "console_scripts": [
            "vision_node = robot_vision.vision_node:main",
        ],
    },
)
