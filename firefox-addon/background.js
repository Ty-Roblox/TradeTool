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
    tabs.find((tab) => tab.active && tab.url && tab.url.includes("/trade2/")) ||
    tabs.find((tab) => tab.url && tab.url.includes("/trade2/")) ||
    tabs.find((tab) => tab.active && tab.id !== undefined) ||
    tabs.find((tab) => tab.id !== undefined) ||
    null
  );
}

function pageTeleportScriptSource(requestId, request) {
  return `
    (() => {
      const requestId = ${JSON.stringify(requestId)};
      const request = ${JSON.stringify(request)};

      async function readJsonResponse(response) {
        const text = await response.text();
        let payload = null;
        try {
          payload = text ? JSON.parse(text) : null;
        } catch (_) {
          payload = null;
        }

        return { text, payload };
      }

      async function resolveToken() {
        if (request.token) {
          return request.token;
        }

        if (!request.fetchUrl || !request.listingId) {
          throw new Error("TradeTool did not provide a hideout token or fetch URL for this listing.");
        }

        const response = await fetch(request.fetchUrl, {
          method: "GET",
          credentials: "include",
          headers: {
            "Accept": "application/json",
            "X-Requested-With": "XMLHttpRequest"
          }
        });

        const { text, payload } = await readJsonResponse(response);

        if (response.status === 401 || response.status === 403) {
          const error = new Error("Firefox is not logged into pathofexile.com or the session expired.");
          error.status = response.status;
          throw error;
        }

        if (!response.ok) {
          throw new Error(text || "POE trade listing fetch returned HTTP " + response.status + ".");
        }

        const listing = payload && Array.isArray(payload.result)
          ? payload.result.find((entry) => entry && entry.id === request.listingId)
          : null;
        const token = listing && listing.listing ? listing.listing.hideout_token : null;

        if (!token) {
          throw new Error("POE did not return an instant-buyout hideout token for this listing.");
        }

        return token;
      }

      async function sendWhisper(token) {
        const response = await fetch("/api/trade2/whisper", {
          method: "POST",
          credentials: "include",
          headers: {
            "Accept": "*/*",
            "Content-Type": "application/json",
            "X-Requested-With": "XMLHttpRequest"
          },
          body: JSON.stringify({ token })
        });

        const { text, payload } = await readJsonResponse(response);
        return { response, text, payload };
      }

      resolveToken()
        .then(sendWhisper)
        .then(({ response, text, payload }) => {
          window.postMessage({
            source: "tradetool-poe2-page",
            requestId,
            ok: response.ok,
            status: response.status,
            payload,
            text
          }, window.location.origin);
        })
        .catch((error) => {
          window.postMessage({
            source: "tradetool-poe2-page",
            requestId,
            ok: false,
            status: 0,
            message: error && error.message ? error.message : String(error)
          }, window.location.origin);
        });
    })();
  `;
}

function teleportExecutionSource(request) {
  const requestId = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const pageScript = pageTeleportScriptSource(requestId, request);

  return `
    (() => new Promise((resolve) => {
      const requestId = ${JSON.stringify(requestId)};

      const timeout = setTimeout(() => {
        window.removeEventListener("message", onMessage);
        resolve({
          success: false,
          message: "Timed out waiting for pathofexile.com to answer the TP request."
        });
      }, 15000);

      function onMessage(event) {
        if (event.source !== window || event.origin !== window.location.origin) {
          return;
        }

        const data = event.data;
        if (
          !data ||
          data.source !== "tradetool-poe2-page" ||
          data.requestId !== requestId
        ) {
          return;
        }

        clearTimeout(timeout);
        window.removeEventListener("message", onMessage);

        if (data.ok && data.payload && data.payload.success === true) {
          resolve({
            success: true,
            message: "Teleport request sent."
          });
          return;
        }

        if (data.status === 401 || data.status === 403) {
          resolve({
            success: false,
            message: "Firefox is not logged into pathofexile.com or the session expired."
          });
          return;
        }

        resolve({
          success: false,
          message:
            data.message ||
            (data.text ? "POE trade returned: " + data.text : "POE trade returned HTTP " + data.status + ".")
        });
      }

      window.addEventListener("message", onMessage);

      const script = document.createElement("script");
      script.textContent = ${JSON.stringify(pageScript)};
      (document.documentElement || document.head || document.body).appendChild(script);
      script.remove();
    }))()
  `;
}

async function sendTeleportInFirefox(request) {
  const tab = await findPathOfExileTab();
  if (!tab || tab.id === undefined) {
    return {
      success: false,
      message: "Open pathofexile.com in Firefox and log in before using TP."
    };
  }

  try {
    const results = await browser.tabs.executeScript(tab.id, {
      code: teleportExecutionSource({
        listingId: request.listingId,
        token: request.token || null,
        fetchUrl: request.fetchUrl || null
      }),
      runAt: "document_idle"
    });

    return results && results[0]
      ? results[0]
      : {
          success: false,
          message: "Firefox did not return a TP result."
        };
  } catch (error) {
    try {
      return await browser.tabs.sendMessage(tab.id, {
        type: "tradetool:teleport-to-hideout",
        listingId: request.listingId,
        token: request.token || null,
        fetchUrl: request.fetchUrl || null
      });
    } catch (_) {
      return {
        success: false,
        message: `Could not run the POE trade request in Firefox: ${error.message}`
      };
    }
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
  const result = await sendTeleportInFirefox(request);
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
