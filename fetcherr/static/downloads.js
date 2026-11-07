// Live download progress: consumes the JSON feed of GET api/ws/downloads and patches
// the server-rendered rows. Formatting mirrors src/ui/format.rs.
(() => {
  const rows = document.getElementById("download-rows");
  const status = document.getElementById("live-status");
  const UNITS = ["B", "KB", "MB", "GB", "TB"];
  const STATE_CLASS = {
    downloading: "active", fetching_metadata: "active",
    seeding: "done", completed: "done", error: "error",
  };
  const STATE_LABEL = {
    queued: "Queued", fetching_metadata: "Metadata", downloading: "Downloading", stalled: "Stalled",
    paused: "Paused", checking: "Checking", moving: "Moving", seeding: "Seeding",
    completed: "Completed", error: "Error", unknown: "Unknown",
  };

  const bytes = (value) => {
    let size = value, unit = 0;
    while (size >= 1024 && unit < UNITS.length - 1) { size /= 1024; unit++; }
    return unit === 0 ? `${value} B` : `${size.toFixed(1)} ${UNITS[unit]}`;
  };
  const speed = (value) => (value === 0 ? "—" : `${bytes(value)}/s`);
  const eta = (seconds) => {
    if (seconds === null || seconds === undefined) return "∞";
    if (seconds === 0) return "—";
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
    return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
  };

  const refreshRows = () => htmx.trigger(rows, "refresh");

  const patchRow = (download) => {
    const row = rows.querySelector(`tr[data-id="${download.id}"]`);
    if (!row) return false;
    const field = (name) => row.querySelector(`[data-field="${name}"]`);
    const stateClass = STATE_CLASS[download.state] ?? "idle";
    const percent = (download.progress * 100).toFixed(1);
    field("state").className = `pill ${stateClass}`;
    field("state").textContent = STATE_LABEL[download.state] ?? download.state;
    field("bar").className = `progress-bar ${stateClass}`;
    field("bar").style.width = `${percent}%`;
    field("percent").textContent = `${percent}%`;
    field("size").textContent = bytes(download.size);
    field("speed").textContent = speed(download.dl_speed);
    field("eta").textContent = eta(download.eta_secs);
    field("peers").textContent = `${download.seeds} / ${download.peers}`;
    return true;
  };

  const handle = (event) => {
    if (event.type === "update") {
      const allKnown = event.downloads.map(patchRow).every(Boolean);
      if (!allKnown) refreshRows();
    } else if (event.type === "removed") {
      event.ids.forEach((id) => rows.querySelector(`tr[data-id="${id}"]`)?.remove());
      if (!rows.querySelector("tr")) refreshRows();
    }
  };

  const connect = (attempt = 0) => {
    const url = new URL("api/ws/downloads", document.baseURI);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(url);
    socket.onopen = () => { attempt = 0; status.textContent = "live"; status.classList.add("on"); };
    socket.onmessage = (message) => handle(JSON.parse(message.data));
    socket.onclose = () => {
      status.textContent = "reconnecting…";
      status.classList.remove("on");
      setTimeout(() => connect(attempt + 1), Math.min(1000 * 2 ** attempt, 15000));
    };
  };

  connect();
})();
