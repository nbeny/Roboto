import { ApiServer } from "./http/server.ts";
import { RobotService } from "./robot/service.ts";
import { RosbridgeClient } from "./rosbridge/client.ts";

/**
 * Point d'entree de l'API.
 *
 * Trois objets, cables ensemble : un client rosbridge qui parle a ROS 2, un service qui
 * met en cache l'etat du robot, un serveur qui expose le tout au tableau de bord.
 */

const rosbridgeUrl = process.env["ROSBRIDGE_URL"] ?? "ws://127.0.0.1:9090";
const host = process.env["API_HOST"] ?? "0.0.0.0";
const port = Number.parseInt(process.env["API_PORT"] ?? "8080", 10);

const client = new RosbridgeClient({ url: rosbridgeUrl });
const service = new RobotService({ client });
const server = new ApiServer({ service, host, port });

client.onConnected(() => {
  console.log(`[api] rosbridge connecte sur ${rosbridgeUrl}`);
});

client.onDisconnected(() => {
  // Pas une erreur fatale : le client se reconnecte seul. C'est precisement quand le
  // pont redemarre qu'on a besoin du tableau de bord.
  console.warn("[api] rosbridge injoignable, nouvelle tentative en cours");
});

service.start();
client.connect();

const listeningPort = await server.listen();
console.log(`[api] REST et WebSocket sur http://${host}:${listeningPort}`);

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    console.log(`[api] arret sur ${signal}`);
    client.close();
    void server.close().then(() => process.exit(0));
  });
}
