const { invoke } = window.__TAURI__.core;

const ACTIVE_STATUSES = new Set(["working", "blocked", "idle"]);
const VIEW_MODES = ["compact", "list", "detail"];
const DEFAULT_VIEW_MODE = "compact";
const MAX_VISIBLE_ROWS = {
  list: 6,
  detail: 7,
};

const savedViewMode = window.localStorage.getItem("view-mode");
const savedTheme = window.localStorage.getItem("theme");
const initialTheme = savedTheme === "dark" || savedTheme === "light"
  ? savedTheme
  : window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
const state = {
  agents: [],
  filter: "active",
  viewMode: VIEW_MODES.includes(savedViewMode)
    ? savedViewMode
    : DEFAULT_VIEW_MODE,
  configured: false,
  connectionMode: "local",
  theme: initialTheme,
  settingsOpen: false,
  polling: false,
  timer: null,
  lastWindowSize: "",
};

const elements = {
  body: document.body,
  agentsView: document.getElementById("agents-view"),
  settingsView: document.getElementById("settings-view"),
  settingsButton: document.getElementById("settings-button"),
  modeButton: document.getElementById("mode-button"),
  statusStrip: document.querySelector(".status-strip"),
  settingsTitle: document.getElementById("settings-title"),
  sshHostField: document.getElementById("ssh-host-field"),
  connectionIndicator: document.getElementById("connection-indicator"),
  compactAgent: document.getElementById("compact-agent"),
  compactAgentName: document.getElementById("compact-agent-name"),
  compactAgentExtra: document.getElementById("compact-agent-extra"),
  workingCount: document.getElementById("working-count"),
  blockedCount: document.getElementById("blocked-count"),
  idleCount: document.getElementById("idle-count"),
  summary: document.getElementById("summary"),
  list: document.getElementById("agent-list"),
  lastUpdate: document.getElementById("last-update"),
  sshTarget: document.getElementById("ssh-target"),
  remoteHerdr: document.getElementById("remote-herdr"),
  settingsMessage: document.getElementById("settings-message"),
  themeToggle: document.getElementById("theme-toggle"),
  testButton: document.getElementById("test-button"),
  saveButton: document.getElementById("save-button"),
};

function visibleAgents() {
  if (state.filter === "all") {
    return state.agents;
  }
  return state.agents.filter((agent) => ACTIVE_STATUSES.has(agent.status));
}

function desiredWindowSize() {
  if (state.settingsOpen) {
    return {
      width: 330,
      height: state.connectionMode === "ssh" ? 288 : 238,
    };
  }

  if (state.viewMode === "compact") {
    return { width: 200, height: 36 };
  }

  const count = visibleAgents().length;
  if (state.viewMode === "list") {
    const contentHeight = count === 0
      ? 40
      : Math.min(count, MAX_VISIBLE_ROWS.list) * 33;
    return { width: 260, height: 35 + contentHeight };
  }

  const contentHeight = count === 0
    ? 40
    : Math.min(count, MAX_VISIBLE_ROWS.detail) * 47;
  return { width: 360, height: 35 + 27 + contentHeight + 20 };
}

function resizeWindow() {
  const size = desiredWindowSize();
  const signature = `${size.width}x${size.height}`;
  if (signature === state.lastWindowSize) {
    return;
  }
  state.lastWindowSize = signature;
  invoke("resize_window", size).catch((error) => {
    state.lastWindowSize = "";
    console.error("Could not resize Herdr Glance:", error);
  });
}

function updateModeControl() {
  const currentIndex = VIEW_MODES.indexOf(state.viewMode);
  const nextMode = VIEW_MODES[(currentIndex + 1) % VIEW_MODES.length];
  const label = `Switch to ${nextMode} view`;
  elements.modeButton.title = label;
  elements.modeButton.setAttribute("aria-label", label);
  elements.body.dataset.viewMode = state.viewMode;
}

function setViewMode(mode) {
  if (!VIEW_MODES.includes(mode)) {
    return;
  }
  state.viewMode = mode;
  window.localStorage.setItem("view-mode", mode);
  updateModeControl();
  renderAgents();
  resizeWindow();
}

function cycleViewMode() {
  const currentIndex = VIEW_MODES.indexOf(state.viewMode);
  setViewMode(VIEW_MODES[(currentIndex + 1) % VIEW_MODES.length]);
}

function showSettings(show) {
  state.settingsOpen = show;
  elements.body.classList.toggle("settings-open", show);
  elements.agentsView.hidden = show;
  elements.settingsView.hidden = !show;
  elements.statusStrip.hidden = show;
  elements.settingsTitle.hidden = !show;
  elements.modeButton.hidden = show;
  elements.settingsButton.classList.toggle("active", show);
  elements.settingsButton.setAttribute("aria-expanded", String(show));
  elements.settingsButton.title = show
    ? "Close connection settings"
    : "Connection settings";
  elements.settingsButton.setAttribute("aria-label", elements.settingsButton.title);
  resizeWindow();

  if (show) {
    const firstInput = state.connectionMode === "ssh"
      ? elements.sshTarget
      : elements.remoteHerdr;
    window.setTimeout(() => firstInput.focus(), 0);
  }
}

function setConnectionMode(mode) {
  if (mode !== "local" && mode !== "ssh") {
    return;
  }
  state.connectionMode = mode;
  elements.sshHostField.hidden = mode !== "ssh";
  document.querySelectorAll(".connection-option").forEach((button) => {
    button.classList.toggle("active", button.dataset.connectionMode === mode);
  });
  elements.settingsMessage.textContent = "";
  elements.settingsMessage.classList.remove("error");
  resizeWindow();
}

function setTheme(theme, persist = true) {
  state.theme = theme === "dark" ? "dark" : "light";
  elements.body.dataset.theme = state.theme;
  elements.themeToggle.checked = state.theme === "dark";
  if (persist) {
    window.localStorage.setItem("theme", state.theme);
  }
}

function setConnectionState(kind, message) {
  elements.connectionIndicator.classList.toggle("connected", kind === "connected");
  elements.connectionIndicator.classList.toggle("error", kind === "error");
  elements.connectionIndicator.title = message;
}

function contextLabel(agent) {
  if (agent.tab && agent.tab !== agent.workspace) {
    return `${agent.workspace} / ${agent.tab}`;
  }
  return agent.workspace || agent.pane_id;
}

function renderAgents() {
  const agents = visibleAgents();
  elements.list.replaceChildren();

  if (agents.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "No active agents";
    elements.list.appendChild(empty);
    resizeWindow();
    return;
  }

  for (const agent of agents) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = `agent-row${agent.focused ? " focused" : ""}`;
    row.dataset.status = agent.status;
    row.title = `Focus ${agent.name}`;

    const rail = document.createElement("span");
    rail.className = "status-rail";

    const copy = document.createElement("span");
    copy.className = "agent-copy";
    const name = document.createElement("span");
    name.className = "agent-name";
    name.textContent = agent.name;
    const context = document.createElement("span");
    context.className = "agent-context";
    context.textContent = contextLabel(agent);
    copy.append(name, context);

    const statusDot = document.createElement("span");
    statusDot.className = "agent-status-dot";
    const status = document.createElement("span");
    status.className = "agent-status";
    status.textContent = agent.status;

    row.append(rail, copy, statusDot, status);
    row.addEventListener("click", () => focusAgent(agent, row));
    elements.list.appendChild(row);
  }
  resizeWindow();
}

function renderSummary() {
  const activeAgents = state.agents.filter(
    (agent) => ACTIVE_STATUSES.has(agent.status),
  );
  const compactAgent = activeAgents.find((agent) => agent.focused)
    || activeAgents.find((agent) => agent.status === "blocked")
    || activeAgents.find((agent) => agent.status === "working")
    || activeAgents[0]
    || state.agents[0];
  const working = state.agents.filter(
    (agent) => agent.status === "working",
  ).length;
  const blocked = state.agents.filter(
    (agent) => agent.status === "blocked",
  ).length;
  const idle = state.agents.filter(
    (agent) => agent.status === "idle",
  ).length;
  elements.workingCount.textContent = String(working);
  elements.blockedCount.textContent = String(blocked);
  elements.idleCount.textContent = String(idle);
  elements.compactAgentName.textContent = compactAgent?.name || "No agents";
  elements.compactAgent.dataset.status = compactAgent?.status || "unknown";
  elements.compactAgent.title = compactAgent
    ? `${compactAgent.name} - ${compactAgent.status}`
    : "No agents";
  const additionalAgents = Math.max(0, activeAgents.length - 1);
  elements.compactAgentExtra.textContent = additionalAgents > 0
    ? `+${additionalAgents}`
    : "";
  elements.summary.textContent = `${visibleAgents().length}/${state.agents.length}`;
}

async function focusAgent(agent, row) {
  row.disabled = true;
  try {
    await invoke("focus_agent", { paneId: agent.pane_id });
    elements.lastUpdate.textContent = `Focused ${agent.name}`;
  } catch (error) {
    elements.lastUpdate.textContent = String(error);
  } finally {
    row.disabled = false;
  }
}

async function pollAgents() {
  if (!state.configured || state.polling) {
    return;
  }
  state.polling = true;
  try {
    state.agents = await invoke("list_agents");
    const connectionLabel = state.connectionMode === "ssh"
      ? elements.sshTarget.value.trim()
      : "Local Herdr";
    setConnectionState("connected", connectionLabel);
    renderSummary();
    renderAgents();
    elements.lastUpdate.textContent = `Updated ${new Date().toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })}`;
  } catch (error) {
    state.agents = [];
    setConnectionState("error", String(error));
    elements.lastUpdate.textContent = String(error);
    renderSummary();
    renderAgents();
  } finally {
    state.polling = false;
  }
}

function connectionFromForm() {
  return {
    ssh_target: state.connectionMode === "ssh"
      ? elements.sshTarget.value.trim()
      : "",
    remote_herdr: elements.remoteHerdr.value.trim() || "herdr",
  };
}

async function saveConnection() {
  setSettingsBusy(true);
  try {
    const config = connectionFromForm();
    await invoke("save_connection", { config });
    state.configured = true;
    elements.settingsMessage.textContent = "Saved";
    elements.settingsMessage.classList.remove("error");
    showSettings(false);
    await pollAgents();
  } catch (error) {
    showSettingsError(error);
  } finally {
    setSettingsBusy(false);
  }
}

async function testConnection() {
  setSettingsBusy(true);
  try {
    const config = connectionFromForm();
    const count = await invoke("test_connection", { config });
    elements.settingsMessage.textContent = `Connected. ${count} agents found.`;
    elements.settingsMessage.classList.remove("error");
  } catch (error) {
    showSettingsError(error);
  } finally {
    setSettingsBusy(false);
  }
}

function showSettingsError(error) {
  elements.settingsMessage.textContent = String(error);
  elements.settingsMessage.classList.add("error");
}

function setSettingsBusy(busy) {
  elements.saveButton.disabled = busy;
  elements.testButton.disabled = busy;
}

function bindEvents() {
  elements.modeButton.addEventListener("click", cycleViewMode);
  elements.settingsButton.addEventListener("click", () => {
    showSettings(!state.settingsOpen);
  });
  elements.saveButton.addEventListener("click", saveConnection);
  elements.testButton.addEventListener("click", testConnection);
  elements.themeToggle.addEventListener("change", () => {
    setTheme(elements.themeToggle.checked ? "dark" : "light");
  });

  document.querySelectorAll(".filter").forEach((button) => {
    button.addEventListener("click", () => {
      state.filter = button.dataset.mode;
      document.querySelectorAll(".filter").forEach((candidate) => {
        candidate.classList.toggle("active", candidate === button);
      });
      renderSummary();
      renderAgents();
    });
  });

  document.querySelectorAll(".connection-option").forEach((button) => {
    button.addEventListener("click", () => {
      setConnectionMode(button.dataset.connectionMode);
    });
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && state.settingsOpen) {
      showSettings(false);
    }
  });
}

async function init() {
  setTheme(state.theme, false);
  updateModeControl();
  renderSummary();
  renderAgents();

  try {
    const bootstrap = await invoke("get_connection");
    const config = bootstrap.config;
    elements.sshTarget.value = config.ssh_target || "";
    elements.remoteHerdr.value = config.remote_herdr || "herdr";
    state.configured = bootstrap.configured;
    setConnectionMode(config.ssh_target ? "ssh" : "local");

    if (bootstrap.warning) {
      showSettings(true);
      showSettingsError(bootstrap.warning);
    } else if (!state.configured) {
      setConnectionState("error", "Not configured");
      showSettings(true);
    } else {
      await pollAgents();
    }
  } catch (error) {
    setConnectionState("error", "Startup failed");
    showSettings(true);
    showSettingsError(error);
  }

  state.timer = window.setInterval(pollAgents, 2000);
}

bindEvents();
init();
