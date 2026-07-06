"use strict";

const DEFAULT_PORT = 17652;
const POLL_IDLE_MS = 700;
const POLL_ERROR_MS = 2500;

let running = false;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function getConfig() {
  const config = await browser.storage.local.get({
    port: DEFAULT_PORT,
    pairingKey: ""
  });

  return {
    port: Number(config.port) || DEFAULT_PORT,
    pairingKey: String(config.pairingKey || "").trim()
  };
}

async function setBadge(text, color) {
  await browser.browserAction.setBadgeText({ text });
  await browser.browserAction.setBadgeBackgroundColor({ color });
}

async function bridgeFetch(path, init = {}) {
  const config = await getConfig();
  if (!config.pairingKey) {
    throw new Error("Paste the TradeTool pairing key in the add-on options.");
  }

  const headers = new Headers(init.headers || {});
  headers.set("X-TradeTool-Key", config.pairingKey);

  return fetch(`http://127.0.0.1:${config.port}${path}`, {
    ...init,
    headers,
    cache: "no-store"
  });
}

async function findPathOfExileTab() {
  const tabs = await browser.tabs.query({
    url: "https://www.pathofexile.com/*"
  });

  return (
    tabs.find((tab) => tab.url && tab.url.includes("/trade2/")) ||
    tabs.find((tab) => tab.id !== undefined) ||
    null
  );
}

async function sendTeleportInFirefox(token) {
  const tab = await findPathOfExileTab();
  if (!tab || tab.id === undefined) {
    return {
      success: false,
      message: "Open pathofexile.com in Firefox and log in before using TP."
    };
  }

  try {
    return await browser.tabs.sendMessage(tab.id, {
      type: "tradetool:teleport-to-hideout",
      token
    });
  } catch (error) {
    return {
      success: false,
      message: `Could not reach the POE trade tab: ${error.message}`
    };
  }
}

async function postTeleportResult(requestId, result) {
  const response = await bridgeFetch("/result", {
    method: "POST",
    headers: {
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      requestId,
      success: Boolean(result && result.success),
      message: result && result.message ? String(result.message) : null
    })
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `TradeTool bridge returned HTTP ${response.status}.`);
  }
}

async function pollOnce() {
  const response = await bridgeFetch("/next");

  if (response.status === 204) {
    await setBadge("OK", "#247a59");
    return POLL_IDLE_MS;
  }

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `TradeTool bridge returned HTTP ${response.status}.`);
  }

  const request = await response.json();
  await setBadge("TP", "#d6b36a");
  const result = await sendTeleportInFirefox(request.token);
  await postTeleportResult(request.requestId, result);
  await setBadge(result.success ? "SENT" : "ERR", result.success ? "#247a59" : "#a83333");
  return POLL_IDLE_MS;
}

async function pollLoop() {
  if (running) {
    return;
  }

  running = true;

  while (running) {
    try {
      const delay = await pollOnce();
      await sleep(delay);
    } catch (error) {
      await setBadge("OFF", "#7a342e");
      console.warn("TradeTool bridge polling failed:", error);
      await sleep(POLL_ERROR_MS);
    }
  }
}

browser.runtime.onInstalled.addListener(() => {
  browser.runtime.openOptionsPage();
});

browser.browserAction.onClicked.addListener(() => {
  browser.runtime.openOptionsPage();
});

pollLoop();
