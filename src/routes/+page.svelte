<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check } from "@tauri-apps/plugin-updater";
  import { onMount } from "svelte";

  type ItemProperty = {
    name: string;
    value: string;
  };

  type ItemModifier = {
    index: number;
    text: string;
  };

  type CapturedItem = {
    rawText: string;
    itemClass?: string;
    rarity?: string;
    itemName?: string;
    baseType?: string;
    itemLevel?: number;
    quality?: number;
    sockets?: string;
    properties: ItemProperty[];
    explicitMods: ItemModifier[];
  };

  type FilterCandidate = {
    id: string;
    label: string;
    selectedByDefault: boolean;
    supported: boolean;
    unsupportedReason?: string;
  };

  type FilterGroup = {
    id: string;
    label: string;
    filters: FilterCandidate[];
  };

  type CaptureResponse = {
    hotkey: string;
    item: CapturedItem;
    filterGroups: FilterGroup[];
  };

  type TradeSearchResponse = {
    url: string;
    searchId: string;
    total: number;
    resultIds: string[];
    fetchedCount: number;
    listings: TradeListing[];
    fetchUrl?: string;
    warning?: string;
  };

  type TradeListing = {
    id: string;
    indexed?: string;
    price?: TradePrice;
    accountName?: string;
    item: TradeListingItem;
  };

  type TradePrice = {
    priceType?: string;
    amount: number;
    currency: string;
  };

  type TradeListingItem = {
    icon?: string;
    name?: string;
    typeLine?: string;
    baseType?: string;
    rarity?: string;
    itemLevel?: number;
    explicitMods: string[];
    pseudoMods: string[];
  };

  let league = $state("Runes of Aldur");
  let rawText = $state("");
  let capture = $state<CaptureResponse | null>(null);
  let tradeResult = $state<TradeSearchResponse | null>(null);
  let selectedFilterIds = $state<string[]>([]);
  let captureStatus = $state("Ready");
  let captureError = $state("");
  let searchStatus = $state("");
  let searchError = $state("");
  let searching = $state(false);
  let updateStatus = $state("Checking for updates...");
  let updateError = $state("");

  onMount(() => {
    const storedLeague = localStorage.getItem("poe2TradeLeague");
    if (storedLeague) {
      league = storedLeague;
    }

    const unlisteners: UnlistenFn[] = [];

    void listen<CaptureResponse>("item_captured", (event) => {
      applyCapture(event.payload, "Captured from hotkey.");
    }).then((unlisten) => unlisteners.push(unlisten));

    void listen<string>("capture_error", (event) => {
      captureStatus = "Capture failed.";
      captureError = event.payload;
    }).then((unlisten) => unlisteners.push(unlisten));

    void checkForUpdates();

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  });

  function applyCapture(response: CaptureResponse, status: string) {
    capture = response;
    rawText = response.item.rawText;
    selectedFilterIds = response.filterGroups
      .flatMap((group) => group.filters)
      .filter((filter) => filter.supported && filter.selectedByDefault)
      .map((filter) => filter.id);
    captureStatus = status;
    captureError = "";
    searchStatus = "";
    searchError = "";
    tradeResult = null;
  }

  async function captureNow() {
    captureStatus = "Capturing...";
    captureError = "";
    searchStatus = "";
    searchError = "";

    try {
      const response = await invoke<CaptureResponse>("capture_item_now");
      applyCapture(response, "Captured from cursor.");
    } catch (error) {
      captureStatus = "Capture failed.";
      captureError = readableError(error);
    }
  }

  async function parseManual() {
    captureStatus = "Parsing...";
    captureError = "";
    searchStatus = "";
    searchError = "";

    if (!rawText.trim()) {
      captureStatus = "Paste item text.";
      captureError = "Manual item text is empty.";
      return;
    }

    try {
      const response = await invoke<CaptureResponse>("parse_item_text", { rawText });
      applyCapture(response, "Parsed pasted item.");
    } catch (error) {
      captureStatus = "Parse failed.";
      captureError = readableError(error);
    }
  }

  async function searchTrade() {
    if (!capture) {
      searchError = "Capture or paste an item before searching.";
      return;
    }

    searching = true;
    searchStatus = "Searching trade...";
    searchError = "";

    try {
      const result = await invoke<TradeSearchResponse>("search_trade", {
        request: {
          league,
          rawText: capture.item.rawText,
          selectedFilterIds
        }
      });

      tradeResult = result;
      searchStatus = `Found ${result.total} listings. Showing ${result.fetchedCount}.`;
      localStorage.setItem("poe2TradeLeague", league);
    } catch (error) {
      searchStatus = "Search unavailable.";
      searchError = readableError(error);
    } finally {
      searching = false;
    }
  }

  async function openOfficialSearch() {
    if (!tradeResult) {
      return;
    }

    try {
      await invoke("open_trade_url", { url: tradeResult.url });
      searchStatus = "Opened official trade search.";
    } catch (error) {
      searchError = readableError(error);
    }
  }

  async function checkForUpdates() {
    try {
      const update = await check();

      if (!update) {
        updateStatus = "Up to date.";
        return;
      }

      updateStatus = `Downloading ${update.version}...`;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            updateStatus = "Update download started.";
            break;
          case "Progress":
            updateStatus = "Downloading update...";
            break;
          case "Finished":
            updateStatus = "Update downloaded.";
            break;
        }
      });

      updateStatus = "Restarting...";
      await relaunch();
    } catch (error) {
      updateStatus = "Updater unavailable.";
      updateError = readableError(error);
    }
  }

  function toggleFilter(id: string, checked: boolean) {
    selectedFilterIds = checked
      ? Array.from(new Set([...selectedFilterIds, id]))
      : selectedFilterIds.filter((selectedId) => selectedId !== id);
  }

  function isSelected(id: string) {
    return selectedFilterIds.includes(id);
  }

  function updateLeague(event: Event) {
    league = (event.currentTarget as HTMLInputElement).value;
    localStorage.setItem("poe2TradeLeague", league);
  }

  function readableError(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  function formatPrice(price?: TradePrice) {
    if (!price) {
      return "No price";
    }

    const amount = Number.isInteger(price.amount)
      ? String(price.amount)
      : price.amount.toFixed(2).replace(/\.?0+$/, "");

    return `${amount} ${price.currency}`;
  }

  function listingTitle(listing: TradeListing) {
    return listing.item.name || listing.item.typeLine || listing.item.baseType || "Unknown item";
  }

  function listingSubtitle(listing: TradeListing) {
    const parts = [
      listing.item.typeLine,
      listing.item.rarity,
      listing.item.itemLevel ? `ilvl ${listing.item.itemLevel}` : ""
    ].filter(Boolean);

    return parts.join(" / ");
  }
</script>

<svelte:head>
  <title>POE2 Trade Tool</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div>
      <h1>POE2 Trade Tool</h1>
      <p>{capture?.hotkey ?? "F8"} capture</p>
    </div>

    <label class="league-field">
      <span>League</span>
      <input value={league} oninput={updateLeague} />
    </label>
  </header>

  <section class="workspace">
    <aside class="capture-panel">
      <div class="section-heading">
        <h2>Capture</h2>
        <span>{captureStatus}</span>
      </div>

      <button class="primary-action" type="button" onclick={captureNow}>Capture Item</button>

      {#if captureError}
        <p class="error">{captureError}</p>
      {/if}

      <label class="manual-input">
        <span>Manual Paste</span>
        <textarea bind:value={rawText} spellcheck="false"></textarea>
      </label>

      <button class="secondary-action" type="button" onclick={parseManual}>Parse Paste</button>

      <div class="update-line">
        <span>Updater</span>
        <strong>{updateStatus}</strong>
      </div>
      {#if updateError}
        <p class="error compact">{updateError}</p>
      {/if}
    </aside>

    <section class="item-panel">
      <div class="section-heading">
        <h2>{capture?.item.itemName ?? capture?.item.baseType ?? "No item captured"}</h2>
        {#if capture?.item.rarity}
          <span>{capture.item.rarity}</span>
        {/if}
      </div>

      {#if capture}
        <div class="item-summary">
          <div>
            <span>Base</span>
            <strong>{capture.item.baseType ?? "Unknown"}</strong>
          </div>
          <div>
            <span>Class</span>
            <strong>{capture.item.itemClass ?? "Unknown"}</strong>
          </div>
          <div>
            <span>Level</span>
            <strong>{capture.item.itemLevel ?? "-"}</strong>
          </div>
          <div>
            <span>Quality</span>
            <strong>{capture.item.quality ? `${capture.item.quality}%` : "-"}</strong>
          </div>
        </div>

        <div class="filters">
          {#each capture.filterGroups as group}
            <section class="filter-group">
              <h3>{group.label}</h3>
              {#each group.filters as filter}
                <label class:unsupported={!filter.supported} class="filter-row">
                  <input
                    type="checkbox"
                    checked={isSelected(filter.id)}
                    disabled={!filter.supported || searching}
                    onchange={(event) =>
                      toggleFilter(filter.id, (event.currentTarget as HTMLInputElement).checked)}
                  />
                  <span>{filter.label}</span>
                  {#if !filter.supported}
                    <small>{filter.unsupportedReason}</small>
                  {/if}
                </label>
              {/each}
            </section>
          {/each}
        </div>

        <div class="actions">
          <button
            class="primary-action"
            type="button"
            onclick={searchTrade}
            disabled={searching || selectedFilterIds.length === 0}
          >
            {searching ? "Searching..." : "Search Trade"}
          </button>
          {#if tradeResult}
            <button class="secondary-action" type="button" onclick={openOfficialSearch}>
              Open Official Search
            </button>
          {/if}
          {#if tradeResult?.warning}
            <button class="secondary-action" type="button" onclick={searchTrade} disabled={searching}>
              Retry
            </button>
          {/if}
          <span>{selectedFilterIds.length} selected</span>
        </div>

        {#if searchStatus}
          <p class="status-text">{searchStatus}</p>
        {/if}
        {#if searchError}
          <p class="error">{searchError}</p>
        {/if}

        {#if tradeResult}
          <section class="results-panel">
            <div class="results-heading">
              <div>
                <h3>{tradeResult.total} matches</h3>
                <p>{tradeResult.fetchedCount} listings loaded from the first page</p>
              </div>
              <span>{tradeResult.searchId}</span>
            </div>

            {#if tradeResult.warning}
              <p class="warning">{tradeResult.warning}</p>
            {/if}

            {#if tradeResult.listings.length}
              <div class="listing-list">
                {#each tradeResult.listings as listing}
                  <article class="listing-row">
                    <div class="listing-image">
                      {#if listing.item.icon}
                        <img src={listing.item.icon} alt="" loading="lazy" />
                      {:else}
                        <span></span>
                      {/if}
                    </div>

                    <div class="listing-body">
                      <div class="listing-title">
                        <div>
                          <h4>{listingTitle(listing)}</h4>
                          <p>{listingSubtitle(listing)}</p>
                        </div>
                        <strong>{formatPrice(listing.price)}</strong>
                      </div>

                      <div class="seller-line">
                        <span>{listing.accountName ?? "Unknown seller"}</span>
                        {#if listing.indexed}
                          <span>{listing.indexed}</span>
                        {/if}
                      </div>

                      {#if listing.item.pseudoMods.length}
                        <div class="mod-list pseudo-mods">
                          {#each listing.item.pseudoMods as mod}
                            <span>{mod}</span>
                          {/each}
                        </div>
                      {/if}

                      {#if listing.item.explicitMods.length}
                        <div class="mod-list">
                          {#each listing.item.explicitMods as mod}
                            <span>{mod}</span>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  </article>
                {/each}
              </div>
            {:else}
              <div class="results-empty">
                <p>No listings were loaded in-app.</p>
              </div>
            {/if}
          </section>
        {/if}
      {:else}
        <div class="empty-state">
          <p>Capture an item or paste copied item text.</p>
        </div>
      {/if}
    </section>
  </section>
</main>

<style>
  :root {
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #171d19;
    background: #f1f3ef;
    font-synthesis: none;
    text-rendering: optimizeLegibility;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(body) {
    min-width: 320px;
    min-height: 100vh;
    margin: 0;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  button {
    border: 0;
    border-radius: 8px;
    cursor: pointer;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .app-shell {
    min-height: 100vh;
    display: grid;
    grid-template-rows: auto 1fr;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 24px;
    border-bottom: 1px solid #d4dbd2;
    background: #ffffff;
  }

  h1,
  h2,
  h3,
  p {
    margin: 0;
  }

  h1 {
    font-size: 1.25rem;
    line-height: 1.2;
  }

  .topbar p,
  .section-heading span,
  .item-summary span,
  .update-line span,
  .actions span {
    color: #637069;
    font-size: 0.82rem;
  }

  .league-field {
    display: grid;
    gap: 4px;
    min-width: 180px;
    color: #637069;
    font-size: 0.78rem;
  }

  .league-field input,
  .manual-input textarea {
    border: 1px solid #cad5ce;
    border-radius: 8px;
    background: #ffffff;
    color: #171d19;
  }

  .league-field input {
    height: 34px;
    padding: 0 10px;
  }

  .workspace {
    display: grid;
    grid-template-columns: minmax(280px, 360px) 1fr;
    gap: 20px;
    padding: 20px;
  }

  .capture-panel,
  .item-panel {
    border: 1px solid #d2dbd5;
    border-radius: 8px;
    background: #ffffff;
  }

  .capture-panel {
    align-self: start;
    display: grid;
    gap: 14px;
    padding: 16px;
  }

  .item-panel {
    min-height: 520px;
    padding: 18px;
  }

  .section-heading {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 10px;
    border-bottom: 1px solid #e0e6e1;
  }

  .section-heading h2 {
    font-size: 1rem;
  }

  .primary-action,
  .secondary-action {
    min-height: 40px;
    padding: 0 14px;
    font-weight: 700;
  }

  .primary-action {
    color: #ffffff;
    background: #28684f;
  }

  .secondary-action {
    color: #20392f;
    background: #ddebe3;
  }

  .manual-input {
    display: grid;
    gap: 6px;
    color: #637069;
    font-size: 0.82rem;
  }

  .manual-input textarea {
    min-height: 220px;
    resize: vertical;
    padding: 10px;
    line-height: 1.35;
    white-space: pre;
  }

  .update-line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding-top: 8px;
    border-top: 1px solid #e0e6e1;
    font-size: 0.82rem;
  }

  .item-summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(120px, 1fr));
    gap: 10px;
    margin: 16px 0;
  }

  .item-summary div {
    display: grid;
    gap: 4px;
    padding: 10px;
    border: 1px solid #d8e0da;
    border-radius: 8px;
    background: #f7faf6;
  }

  .filters {
    display: grid;
    gap: 14px;
  }

  .filter-group {
    display: grid;
    gap: 8px;
  }

  .filter-group h3 {
    color: #33453b;
    font-size: 0.86rem;
  }

  .filter-row {
    display: grid;
    grid-template-columns: 18px 1fr;
    gap: 8px 10px;
    align-items: start;
    padding: 8px 10px;
    border: 1px solid #dde5df;
    border-radius: 8px;
    background: #fbfcfa;
  }

  .filter-row input {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
  }

  .filter-row small {
    grid-column: 2;
    color: #8a5a37;
    line-height: 1.3;
  }

  .filter-row.unsupported {
    color: #68716c;
    background: #f2f5f1;
  }

  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 18px;
  }

  .status-text {
    margin-top: 12px;
    color: #28684f;
  }

  .error {
    color: #9e2f2f;
    line-height: 1.35;
  }

  .error.compact {
    font-size: 0.78rem;
  }

  .warning {
    padding: 10px 12px;
    border: 1px solid #e0c59e;
    border-radius: 8px;
    color: #714b1c;
    background: #fff8e9;
    line-height: 1.35;
  }

  .results-panel {
    display: grid;
    gap: 12px;
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid #e0e6e1;
  }

  .results-heading {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
  }

  .results-heading h3,
  .listing-title h4 {
    margin: 0;
  }

  .results-heading p,
  .results-heading span,
  .listing-title p,
  .seller-line,
  .results-empty {
    color: #637069;
    font-size: 0.82rem;
  }

  .listing-list {
    display: grid;
    gap: 10px;
  }

  .listing-row {
    display: grid;
    grid-template-columns: 74px 1fr;
    gap: 12px;
    padding: 12px;
    border: 1px solid #dde5df;
    border-radius: 8px;
    background: #fbfcfa;
  }

  .listing-image {
    display: grid;
    width: 74px;
    min-height: 74px;
    place-items: center;
    border: 1px solid #d8e0da;
    border-radius: 8px;
    background: #eef3ef;
  }

  .listing-image img {
    max-width: 64px;
    max-height: 64px;
    object-fit: contain;
  }

  .listing-body {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .listing-title {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
  }

  .listing-title h4 {
    font-size: 0.95rem;
    line-height: 1.25;
  }

  .listing-title strong {
    flex: 0 0 auto;
    padding: 4px 8px;
    border-radius: 8px;
    color: #fff;
    background: #2d5f7a;
    font-size: 0.86rem;
  }

  .seller-line {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
  }

  .mod-list {
    display: grid;
    gap: 4px;
    color: #27342f;
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .mod-list span {
    overflow-wrap: anywhere;
  }

  .pseudo-mods {
    color: #2d5f7a;
    font-weight: 700;
  }

  .results-empty {
    display: grid;
    min-height: 90px;
    place-items: center;
    border: 1px dashed #cad5ce;
    border-radius: 8px;
  }

  .empty-state {
    display: grid;
    min-height: 420px;
    place-items: center;
    color: #637069;
  }

  @media (max-width: 820px) {
    .topbar,
    .workspace {
      grid-template-columns: 1fr;
    }

    .topbar {
      align-items: stretch;
    }

    .item-summary {
      grid-template-columns: repeat(2, minmax(120px, 1fr));
    }

    .listing-row {
      grid-template-columns: 58px 1fr;
    }

    .listing-image {
      width: 58px;
      min-height: 58px;
    }

    .listing-image img {
      max-width: 50px;
      max-height: 50px;
    }

    .listing-title {
      display: grid;
    }
  }
</style>
