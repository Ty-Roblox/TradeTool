"use strict";

const DEFAULT_PORT = 17652;

const portInput = document.querySelector("#port");
const pairingKeyInput = document.querySelector("#pairingKey");
const statusEl = document.querySelector("#status");

function setStatus(message, ok = true) {
  statusEl.textContent = message;
  statusEl.className = ok ? "ok" : "error";
}

async function loadOptions() {
  const config = await browser.storage.local.get({
    port: DEFAULT_PORT,
    pairingKey: ""
  });
  portInput.value = String(config.port || DEFAULT_PORT);
  pairingKeyInput.value = config.pairingKey || "";
}

async function saveOptions() {
  const port = Number(portInput.value);
  const pairingKey = pairingKeyInput.value.trim();

  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    setStatus("Bridge port must be between 1 and 65535.", false);
    return;
  }

  if (!pairingKey) {
    setStatus("Pairing key is required.", false);
    return;
  }

  await browser.storage.local.set({ port, pairingKey });
  setStatus("Saved.");
}

async function testBridge() {
  await saveOptions();
  const port = Number(portInput.value);
  const pairingKey = pairingKeyInput.value.trim();

  try {
    const response = await fetch(`http://127.0.0.1:${port}/health`, {
      headers: {
        "X-TradeTool-Key": pairingKey
      },
      cache: "no-store"
    });

    if (!response.ok) {
      const text = await response.text();
      setStatus(text || `Bridge returned HTTP ${response.status}.`, false);
      return;
    }

    setStatus("Bridge connected.");
  } catch (error) {
    setStatus(`Bridge test failed: ${error.message}`, false);
  }
}

document.querySelector("#save").addEventListener("click", saveOptions);
document.querySelector("#test").addEventListener("click", testBridge);

loadOptions();
