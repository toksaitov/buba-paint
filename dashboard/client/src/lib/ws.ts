type WsCallback = (msg: unknown) => void;

const MAX_RETRIES = 3;

export function connectWs(
  botId: string,
  onMessage: WsCallback,
  onGiveUp?: () => void,
): () => void {
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  const token = localStorage.getItem("token");
  const qs = token ? `?token=${encodeURIComponent(token)}` : "";
  const url = `${protocol}://${window.location.host}/ws/bots/${botId}${qs}`;

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  let failures = 0;

  function connect() {
    if (closed) return;

    try {
      ws = new WebSocket(url);
    } catch {
      handleFailure();
      return;
    }

    ws.onopen = () => {
      failures = 0;
    };

    ws.onmessage = (ev) => {
      try {
        onMessage(JSON.parse(ev.data as string));
      } catch {
        /* ignore parse errors */
      }
    };

    ws.onclose = () => {
      if (!closed) handleFailure();
    };

    ws.onerror = () => {
      ws?.close();
    };
  }

  function handleFailure() {
    failures++;
    if (failures > MAX_RETRIES) {
      onGiveUp?.();
      return;
    }
    reconnectTimer = setTimeout(connect, 3000 * failures);
  }

  connect();

  return () => {
    closed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    ws?.close();
  };
}
