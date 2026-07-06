"use strict";

function pageScriptSource(requestId, request) {
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

function injectTeleportRequest(token) {
  const requestId = `${Date.now()}-${Math.random().toString(16).slice(2)}`;

  return new Promise((resolve) => {
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
          (data.text ? `POE trade returned: ${data.text}` : `POE trade returned HTTP ${data.status}.`)
      });
    }

    window.addEventListener("message", onMessage);

    const script = document.createElement("script");
    script.textContent = pageScriptSource(requestId, token);
    (document.documentElement || document.head || document.body).appendChild(script);
    script.remove();
  });
}

browser.runtime.onMessage.addListener((message) => {
  if (!message || message.type !== "tradetool:teleport-to-hideout") {
    return undefined;
  }

  if (!message.token && !message.fetchUrl) {
    return Promise.resolve({
      success: false,
      message: "TradeTool did not provide a hideout token or fetch URL for this listing."
    });
  }

  return injectTeleportRequest({
    listingId: message.listingId || "",
    token: message.token || null,
    fetchUrl: message.fetchUrl || null
  });
});
