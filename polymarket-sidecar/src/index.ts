import { createServer } from "./server.js";
import { loadConfig } from "./config.js";

const config = loadConfig();
const server = createServer(undefined, config);

server.listen(config.port, config.host, () => {
  process.stdout.write(
    `buba-polymarket-sidecar listening on http://${config.host}:${config.port}\n`,
  );
});
