<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check } from "@tauri-apps/plugin-updater";
  import { onMount } from "svelte";
  import quickJewelFilters from "$lib/quick-jewel-filters.json";

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
    source?: string;
    affixSide?: string;
    score?: number;
    selectionReason?: string;
    profileIds: string[];
    defaultMin?: number | null;
    defaultMax?: number | null;
  };

  type FilterGroup = {
    id: string;
    label: string;
    filters: FilterCandidate[];
  };

  type AppDiagnostic = {
    code: string;
    message: string;
    detail?: string;
  };

  type PriceCheckProfile = {
    id: string;
    label: string;
    description: string;
    filterIds: string[];
  };

  type CaptureResponse = {
    hotkey: string;
    item: CapturedItem;
    filterGroups: FilterGroup[];
    priceCheckProfiles: PriceCheckProfile[];
    diagnostics: AppDiagnostic[];
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
    diagnostics: AppDiagnostic[];
  };

  type TradeListing = {
    id: string;
    indexed?: string;
    price?: TradePrice;
    accountName?: string;
    canTeleport: boolean;
    item: TradeListingItem;
  };

  type FirefoxBridgeStatus = {
    enabled: boolean;
    port?: number;
    pairingKey: string;
    connected: boolean;
    pending: boolean;
    lastMessage?: string;
  };

  type TeleportToHideoutResponse = {
    success: boolean;
    message: string;
  };

  type FilterValueOverride = {
    id: string;
    min?: number;
    max?: number;
  };

  type FilterRange = {
    min: string;
    max: string;
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

  type QuickJewelStat = {
    id: string;
    label: string;
    min?: number;
  };

  type QuickJewelFilter = {
    id: string;
    label: string;
    baseType: string;
    stats: QuickJewelStat[];
  };

  const quickJewels = quickJewelFilters as QuickJewelFilter[];

  let league = $state("Runes of Aldur");
  let rawText = $state("");
  let capture = $state<CaptureResponse | null>(null);
  let tradeResult = $state<TradeSearchResponse | null>(null);
  let selectedFilterIds = $state<string[]>([]);
  let selectedQuickFilterIds = $state<string[]>([]);
  let filterRanges = $state<Record<string, FilterRange>>({});
  let activePriceProfileId = $state("");
  let activeFilterTab = $state<"item" | "jewels">("jewels");
  let activeJewelId = $state(quickJewels[0]?.id ?? "");
  let quickStatId = $state(quickJewels[0]?.stats[0]?.id ?? "");
  let captureStatus = $state("Ready");
  let captureError = $state("");
  let searchStatus = $state("");
  let searchError = $state("");
  let searching = $state(false);
  let bridgeStatus = $state<FirefoxBridgeStatus | null>(null);
  let bridgeError = $state("");
  let teleportingListingId = $state("");
  let teleportStatuses = $state<Record<string, string>>({});
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
      clearCurrentCapture();
      captureStatus = "Capture failed.";
      captureError = event.payload;
    }).then((unlisten) => unlisteners.push(unlisten));

    void checkForUpdates();
    void refreshFirefoxBridgeStatus();
    const bridgeStatusInterval = window.setInterval(refreshFirefoxBridgeStatus, 3000);

    return () => {
      window.clearInterval(bridgeStatusInterval);
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  });

  function applyCapture(response: CaptureResponse, status: string) {
    capture = response;
    rawText = response.item.rawText;
    activePriceProfileId = response.priceCheckProfiles[0]?.id ?? "";
    selectedFilterIds =
      response.priceCheckProfiles[0]?.filterIds ??
      response.filterGroups
        .flatMap((group) => group.filters)
        .filter((filter) => filter.supported && filter.selectedByDefault)
        .map((filter) => filter.id);
    filterRanges = {
      ...quickFilterRanges(),
      ...rangeDefaultsForFilterGroups(response.filterGroups)
    };
    captureStatus = status;
    captureError = "";
    searchStatus = "";
    searchError = "";
    tradeResult = null;
    teleportStatuses = {};
    activeFilterTab = "item";
  }

  function clearCurrentCapture() {
    capture = null;
    selectedFilterIds = [];
    activePriceProfileId = "";
    tradeResult = null;
    teleportStatuses = {};
    filterRanges = quickFilterRanges();
    searchStatus = "";
    searchError = "";
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
      clearCurrentCapture();
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
      clearCurrentCapture();
      captureStatus = "Paste item text.";
      captureError = "Manual item text is empty.";
      return;
    }

    try {
      const response = await invoke<CaptureResponse>("parse_item_text", { rawText });
      applyCapture(response, "Parsed pasted item.");
    } catch (error) {
      clearCurrentCapture();
      captureStatus = "Parse failed.";
      captureError = readableError(error);
    }
  }

  async function searchTrade() {
    const filterIds = combinedSelectedFilterIds();

    if (filterIds.length === 0) {
      searchError = "Select at least one item or quick filter before searching.";
      return;
    }

    searching = true;
    searchStatus = "Searching trade...";
    searchError = "";

    try {
      const result = await invoke<TradeSearchResponse>("search_trade", {
        request: {
          league,
          rawText: capture?.item.rawText ?? "",
          selectedFilterIds: filterIds,
          selectedFilterValues: selectedFilterValues(filterIds)
        }
      });

      tradeResult = result;
      teleportStatuses = {};
      searchStatus = `Found ${result.total} listings. Showing ${result.fetchedCount}.`;
      localStorage.setItem("poe2TradeLeague", league);
      void refreshFirefoxBridgeStatus();
    } catch (error) {
      searchStatus = "Search unavailable.";
      searchError = readableError(error);
      tradeResult = null;
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

  async function refreshFirefoxBridgeStatus() {
    try {
      bridgeStatus = await invoke<FirefoxBridgeStatus>("firefox_bridge_status");
      bridgeError = "";
    } catch (error) {
      bridgeError = readableError(error);
    }
  }

  function firefoxBridgeLabel() {
    if (bridgeError) {
      return "Disconnected";
    }

    if (!bridgeStatus?.enabled) {
      return "Disconnected";
    }

    if (bridgeStatus.connected) {
      return "Ready";
    }

    return "Paired";
  }

  function firefoxBridgeDetail() {
    if (bridgeError) {
      return bridgeError;
    }

    if (!bridgeStatus?.enabled) {
      return "Bridge unavailable.";
    }

    if (bridgeStatus.connected) {
      return bridgeStatus.lastMessage ?? "Firefox add-on connected.";
    }

    return "Load the Firefox add-on, paste the port/key, and keep a POE tab open.";
  }

  function bridgeSetupText() {
    if (!bridgeStatus?.enabled || !bridgeStatus.port) {
      return "Bridge unavailable";
    }

    return `Port ${bridgeStatus.port} | Key ${bridgeStatus.pairingKey}`;
  }

  async function teleportToHideout(listing: TradeListing) {
    if (!listing.canTeleport || teleportingListingId) {
      return;
    }

    teleportingListingId = listing.id;
    teleportStatuses = {
      ...teleportStatuses,
      [listing.id]: "Sending..."
    };
    searchError = "";

    try {
      const response = await invoke<TeleportToHideoutResponse>("teleport_to_hideout", {
        listingId: listing.id
      });
      teleportStatuses = {
        ...teleportStatuses,
        [listing.id]: response.message || "Sent"
      };
      searchStatus = response.message || "Teleport request sent.";
    } catch (error) {
      const message = readableError(error);
      teleportStatuses = {
        ...teleportStatuses,
        [listing.id]: message
      };
      searchError = message;
    } finally {
      teleportingListingId = "";
      void refreshFirefoxBridgeStatus();
    }
  }

  function teleportButtonLabel(listing: TradeListing) {
    if (teleportingListingId === listing.id) {
      return "Sending...";
    }

    const status = teleportStatuses[listing.id];
    if (status === "Teleport request sent." || status === "sent") {
      return "Sent";
    }

    if (status && status !== "Sending...") {
      return "TP";
    }

    return "TP";
  }

  function isTeleportError(status?: string) {
    return Boolean(
      status &&
        status !== "Sending..." &&
        status !== "Teleport request sent." &&
        status !== "sent"
    );
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
    const filter = findCaptureFilter(id);
    activePriceProfileId = "";

    selectedFilterIds = checked
      ? Array.from(new Set([...selectedFilterIds, id]))
      : selectedFilterIds.filter((selectedId) => selectedId !== id);

    if (checked && filter) {
      ensureFilterRange(id, filter.defaultMin, filter.defaultMax);
    }

    clearSearchResult();
  }

  function applyPriceProfile(profile: PriceCheckProfile) {
    selectedFilterIds = profile.filterIds;
    activePriceProfileId = profile.id;
    for (const id of profile.filterIds) {
      const filter = findCaptureFilter(id);
      if (filter) {
        ensureFilterRange(id, filter.defaultMin, filter.defaultMax);
      }
    }
    clearSearchResult();
  }

  function isSelected(id: string) {
    return selectedFilterIds.includes(id);
  }

  function combinedSelectedFilterIds() {
    return Array.from(new Set([...selectedFilterIds, ...selectedQuickFilterIds]));
  }

  function selectedFilterCount() {
    return combinedSelectedFilterIds().length;
  }

  function activeJewel() {
    return quickJewels.find((jewel) => jewel.id === activeJewelId) ?? quickJewels[0];
  }

  function quickBaseFilterId(jewelId: string) {
    return `quick:jewel:${jewelId}:base`;
  }

  function quickStatFilterId(jewelId: string, statId: string) {
    return `quick:jewel:${jewelId}:stat:${statId}`;
  }

  function selectQuickJewel(jewelId: string) {
    const switching = activeJewelId !== jewelId;
    activeJewelId = jewelId;

    const jewel = activeJewel();
    quickStatId = jewel?.stats[0]?.id ?? "";

    const nextFilters = switching
      ? selectedQuickFilterIds.filter((id) => !id.startsWith("quick:jewel:"))
      : selectedQuickFilterIds;

    selectedQuickFilterIds = Array.from(new Set([...nextFilters, quickBaseFilterId(jewelId)]));
    activeFilterTab = "jewels";
    clearSearchResult();
  }

  function addQuickJewelStat() {
    const jewel = activeJewel();
    if (!jewel || !quickStatId) {
      return;
    }

    selectedQuickFilterIds = Array.from(
      new Set([
        ...selectedQuickFilterIds,
        quickBaseFilterId(jewel.id),
        quickStatFilterId(jewel.id, quickStatId)
      ])
    );
    ensureFilterRange(quickStatFilterId(jewel.id, quickStatId), quickStatDefaultMin(jewel.id, quickStatId));
    clearSearchResult();
  }

  function removeQuickFilter(id: string) {
    const parts = id.split(":");
    const jewelId = parts[2];

    const removedIds =
      parts[3] === "base"
        ? selectedQuickFilterIds.filter((filterId) => filterId.startsWith(`quick:jewel:${jewelId}:`))
        : [id];

    selectedQuickFilterIds =
      parts[3] === "base"
        ? selectedQuickFilterIds.filter((filterId) => !filterId.startsWith(`quick:jewel:${jewelId}:`))
        : selectedQuickFilterIds.filter((filterId) => filterId !== id);
    removeFilterRanges(removedIds);

    clearSearchResult();
  }

  function quickFilterLabel(id: string) {
    const parts = id.split(":");
    const jewel = quickJewels.find((candidate) => candidate.id === parts[2]);

    if (!jewel) {
      return id;
    }

    if (parts[3] === "base") {
      return `${jewel.label} base`;
    }

    const statId = parts.slice(4).join(":");
    const stat = jewel.stats.find((candidate) => candidate.id === statId);
    return stat ? `${jewel.label}: ${stat.label}` : id;
  }

  function isQuickBaseSelected(jewelId: string) {
    return selectedQuickFilterIds.includes(quickBaseFilterId(jewelId));
  }

  function clearSearchResult() {
    tradeResult = null;
    searchStatus = "";
    searchError = "";
  }

  function rangeDefaultsForFilterGroups(groups: FilterGroup[]) {
    const ranges: Record<string, FilterRange> = {};

    for (const filter of groups.flatMap((group) => group.filters)) {
      if (hasRangeDefaults(filter)) {
        ranges[filter.id] = rangeFromDefaults(filter.defaultMin, filter.defaultMax);
      }
    }

    return ranges;
  }

  function quickFilterRanges() {
    return Object.fromEntries(
      Object.entries(filterRanges).filter(([id]) => id.startsWith("quick:jewel:"))
    );
  }

  function hasRangeDefaults(filter: FilterCandidate) {
    return filter.supported && (filter.defaultMin != null || filter.defaultMax != null);
  }

  function rangeFromDefaults(min?: number | null, max?: number | null): FilterRange {
    return {
      min: formatRangeValue(min),
      max: formatRangeValue(max)
    };
  }

  function formatRangeValue(value?: number | null) {
    return value == null || !Number.isFinite(value) ? "" : String(value);
  }

  function ensureFilterRange(id: string, min?: number | null, max?: number | null) {
    if (filterRanges[id]) {
      return;
    }

    filterRanges = {
      ...filterRanges,
      [id]: rangeFromDefaults(min, max)
    };
  }

  function updateFilterRange(id: string, side: keyof FilterRange, value: string) {
    const current = filterRanges[id] ?? rangeFromDefaults();
    filterRanges = {
      ...filterRanges,
      [id]: {
        ...current,
        [side]: value
      }
    };
    clearSearchResult();
  }

  function removeFilterRanges(ids: string[]) {
    const remove = new Set(ids);
    filterRanges = Object.fromEntries(
      Object.entries(filterRanges).filter(([id]) => !remove.has(id))
    );
  }

  function findCaptureFilter(id: string) {
    return capture?.filterGroups
      .flatMap((group) => group.filters)
      .find((filter) => filter.id === id);
  }

  function quickStatDefaultMin(jewelId: string, statId: string) {
    return quickJewels
      .find((jewel) => jewel.id === jewelId)
      ?.stats.find((stat) => stat.id === statId)?.min;
  }

  function selectedFilterValues(filterIds: string[]): FilterValueOverride[] {
    return filterIds
      .filter((id) => filterRanges[id])
      .map((id) => ({
        id,
        min: parseRangeNumber(filterRanges[id].min),
        max: parseRangeNumber(filterRanges[id].max)
      }));
  }

  function parseRangeNumber(value: string) {
    const trimmed = value.trim();
    if (!trimmed) {
      return undefined;
    }

    const parsed = Number(trimmed);
    return Number.isFinite(parsed) ? parsed : undefined;
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

  function formatFilterTag(value?: string) {
    if (!value) {
      return "";
    }

    return value
      .split(/[\s_-]+/)
      .filter(Boolean)
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(" ");
  }
</script>

<svelte:head>
  <title>POE2 Trade Tool</title>
</svelte:head>

<main class="app-shell">
  <header class="topbar">
    <div class="brand-lockup">
      <div class="brand-mark">T</div>
      <div>
        <h1>TradeProject</h1>
        <p>POE2 trade search workspace</p>
      </div>
    </div>

    <nav class="topnav" aria-label="Workspace sections">
      <a href="#capture">Capture</a>
      <a href="#filters">Filters</a>
      <a href="#results">Results</a>
    </nav>

    <div class="topbar-controls">
      <div class="status-pill">
        <span>Hotkey</span>
        <strong>{capture?.hotkey ?? "F8"}</strong>
      </div>

      <label class="league-field">
        <span>League</span>
        <input value={league} oninput={updateLeague} />
      </label>
    </div>
  </header>

  <section class="workspace">
    <aside class="capture-panel" id="capture">
      <div class="composer-intro">
        <span class="eyebrow">Item input</span>
        <h2>What do you want to price?</h2>
        <p>Capture from the game or paste copied item text.</p>
      </div>

      <div class="prompt-chips" aria-label="Trade workflow">
        <span>Capture</span>
        <span>Parse</span>
        <span>Filter</span>
        <span>Search</span>
      </div>

      {#if captureError}
        <div class="notice-card error-card">
          <strong>{captureStatus}</strong>
          <p>{captureError}</p>
        </div>
      {/if}

      <div class="composer-card">
        <label class="manual-input">
          <span>Manual Paste</span>
          <textarea
            bind:value={rawText}
            spellcheck="false"
            placeholder="Paste copied POE2 item text here..."
          ></textarea>
        </label>

        <div class="composer-actions">
          <button class="primary-action" type="button" onclick={captureNow}>Capture Item</button>
          <button class="secondary-action" type="button" onclick={parseManual}>Parse Paste</button>
          <span>{captureStatus}</span>
        </div>
      </div>

      <div class="update-line">
        <span>Updater</span>
        <strong>{updateStatus}</strong>
      </div>
      {#if updateError}
        <div class="notice-card error-card compact">
          <strong>Updater</strong>
          <p>{updateError}</p>
        </div>
      {/if}

      <div class="bridge-card">
        <div class="bridge-heading">
          <span>Firefox TP</span>
          <strong class:ready={bridgeStatus?.connected}>{firefoxBridgeLabel()}</strong>
        </div>
        <p>{firefoxBridgeDetail()}</p>
        <code>{bridgeSetupText()}</code>
      </div>
    </aside>

    <section class="item-panel" id="filters">
      <div class="workspace-heading">
        <div>
          <span class="eyebrow">Generated query</span>
          <h2>{capture?.item.itemName ?? capture?.item.baseType ?? "No item captured"}</h2>
        </div>
        <div class="heading-pills">
          {#if capture?.item.rarity}
            <span>{capture.item.rarity}</span>
          {/if}
          <span>{selectedFilterCount()} selected</span>
        </div>
      </div>

      <div class="filter-tabs" role="tablist" aria-label="Filter modes">
        <button
          class:active-tab={activeFilterTab === "jewels"}
          type="button"
          role="tab"
          aria-selected={activeFilterTab === "jewels"}
          onclick={() => (activeFilterTab = "jewels")}
        >
          Quick Jewels
        </button>
        <button
          class:active-tab={activeFilterTab === "item"}
          type="button"
          role="tab"
          aria-selected={activeFilterTab === "item"}
          disabled={!capture}
          onclick={() => (activeFilterTab = "item")}
        >
          Item Filters
        </button>
      </div>

      {#if activeFilterTab === "jewels"}
        <section class="quick-filter-panel">
          <div class="quick-filter-heading">
            <div>
              <span class="eyebrow">Quick filter</span>
              <h3>{activeJewel()?.label ?? "Jewels"}</h3>
            </div>
            <span>{activeJewel()?.stats.length ?? 0} stats</span>
          </div>

          <div class="jewel-picker" aria-label="Jewel bases">
            {#each quickJewels as jewel}
              <button
                class:jewel-active={activeJewelId === jewel.id || isQuickBaseSelected(jewel.id)}
                type="button"
                onclick={() => selectQuickJewel(jewel.id)}
              >
                <strong>{jewel.label}</strong>
                <span>{jewel.stats.length ? `${jewel.stats.length} stats` : "base only"}</span>
              </button>
            {/each}
          </div>

          <div class="quick-builder">
            <label>
              <span>Stat</span>
              <select bind:value={quickStatId} disabled={!activeJewel()?.stats.length}>
                {#each activeJewel()?.stats ?? [] as stat}
                  <option value={stat.id}>
                    {stat.label}{stat.min ? ` (${stat.min}+ default)` : ""}
                  </option>
                {/each}
              </select>
            </label>
            <button class="secondary-action" type="button" onclick={() => selectQuickJewel(activeJewelId)}>
              Set Base
            </button>
            <button
              class="primary-action"
              type="button"
              onclick={addQuickJewelStat}
              disabled={!quickStatId || searching}
            >
              Add Stat
            </button>
          </div>

          {#if selectedQuickFilterIds.length}
            <div class="quick-chip-list" aria-label="Selected quick filters">
              {#each selectedQuickFilterIds as id}
                <div class="quick-chip">
                  <span>{quickFilterLabel(id)}</span>
                  {#if filterRanges[id]}
                    <div class="range-controls compact-range" aria-label={`${quickFilterLabel(id)} range`}>
                      <label>
                        <span>Min</span>
                        <input
                          type="number"
                          inputmode="decimal"
                          value={filterRanges[id].min}
                          oninput={(event) =>
                            updateFilterRange(id, "min", (event.currentTarget as HTMLInputElement).value)}
                        />
                      </label>
                      <label>
                        <span>Max</span>
                        <input
                          type="number"
                          inputmode="decimal"
                          value={filterRanges[id].max}
                          oninput={(event) =>
                            updateFilterRange(id, "max", (event.currentTarget as HTMLInputElement).value)}
                        />
                      </label>
                    </div>
                  {/if}
                  <button type="button" onclick={() => removeQuickFilter(id)}>Remove</button>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {:else if capture}
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

        {#if capture.priceCheckProfiles.length}
          <section class="profile-panel" aria-label="Price check profiles">
            <div class="profile-heading">
              <div>
                <span class="eyebrow">Smart price check</span>
                <h3>{capture.priceCheckProfiles.find((profile) => profile.id === activePriceProfileId)?.label ?? "Custom Filters"}</h3>
              </div>
              <span>{activePriceProfileId ? "Profile" : "Custom"}</span>
            </div>
            <div class="profile-grid">
              {#each capture.priceCheckProfiles as profile}
                <button
                  class:profile-active={activePriceProfileId === profile.id}
                  type="button"
                  onclick={() => applyPriceProfile(profile)}
                  disabled={searching}
                >
                  <strong>{profile.label}</strong>
                  <span>{profile.description}</span>
                  <small>{profile.filterIds.length} filters</small>
                </button>
              {/each}
            </div>
          </section>
        {/if}

        {#if capture.diagnostics.length}
          <section class="diagnostics-panel">
            <div class="diagnostics-heading">
              <h3>Diagnostics</h3>
              <span>
                {capture.diagnostics.length === 1
                  ? "1 failed id"
                  : `${capture.diagnostics.length} failed ids`}
              </span>
            </div>
            <div class="diagnostic-list">
              {#each capture.diagnostics as diagnostic}
                <article class="diagnostic-row">
                  <code>{diagnostic.code}</code>
                  <strong>{diagnostic.message}</strong>
                  {#if diagnostic.detail}
                    <p>{diagnostic.detail}</p>
                  {/if}
                </article>
              {/each}
            </div>
          </section>
        {/if}

        <div class="filters">
          {#each capture.filterGroups as group}
            <section class="filter-group">
              <div class="filter-group-heading">
                <h3>{group.label}</h3>
                <span>{group.filters.length}</span>
              </div>
              {#each group.filters as filter}
                <div class:unsupported={!filter.supported} class="filter-row">
                  <input
                    type="checkbox"
                    checked={isSelected(filter.id)}
                    disabled={!filter.supported || searching}
                    aria-label={filter.label}
                    onchange={(event) =>
                      toggleFilter(filter.id, (event.currentTarget as HTMLInputElement).checked)}
                  />
                  <div class="filter-content">
                    <span>{filter.label}</span>
                    {#if filter.source || filter.affixSide || filter.score || filter.selectionReason}
                      <div class="filter-meta">
                        {#if filter.affixSide}
                          <strong class="affix-badge">{formatFilterTag(filter.affixSide)}</strong>
                        {/if}
                        {#if filter.source}
                          <strong class="source-badge">{formatFilterTag(filter.source)}</strong>
                        {/if}
                        {#if filter.score}
                          <strong>Score {filter.score}</strong>
                        {/if}
                        {#if filter.selectionReason}
                          <small>{filter.selectionReason}</small>
                        {/if}
                      </div>
                    {/if}
                    {#if filter.supported && isSelected(filter.id) && filterRanges[filter.id]}
                      <div class="range-controls" aria-label={`${filter.label} range`}>
                        <label>
                          <span>Min</span>
                          <input
                            type="number"
                            inputmode="decimal"
                            value={filterRanges[filter.id].min}
                            oninput={(event) =>
                              updateFilterRange(
                                filter.id,
                                "min",
                                (event.currentTarget as HTMLInputElement).value
                              )}
                          />
                        </label>
                        <label>
                          <span>Max</span>
                          <input
                            type="number"
                            inputmode="decimal"
                            value={filterRanges[filter.id].max}
                            oninput={(event) =>
                              updateFilterRange(
                                filter.id,
                                "max",
                                (event.currentTarget as HTMLInputElement).value
                              )}
                          />
                        </label>
                      </div>
                    {/if}
                  </div>
                  {#if !filter.supported}
                    <small>{filter.unsupportedReason}</small>
                  {/if}
                </div>
              {/each}
            </section>
          {/each}
        </div>

      {:else}
        <div class="empty-state">
          <span class="empty-mark">T</span>
          <h3>Start with an item</h3>
          <p>Capture an item or paste copied item text.</p>
        </div>
      {/if}

      <div class="actions">
        <button
          class="primary-action"
          type="button"
          onclick={searchTrade}
          disabled={searching || selectedFilterCount() === 0}
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
        <span>{selectedFilterCount()} selected</span>
      </div>

      {#if searchStatus}
        <p class="status-text">{searchStatus}</p>
      {/if}
      {#if searchError}
        <div class="notice-card error-card search-error">
          <strong>{searchStatus || "Search failed"}</strong>
          <p>{searchError}</p>
        </div>
      {/if}

      {#if tradeResult}
        <section class="results-panel" id="results">
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

          {#if tradeResult.diagnostics.length}
            <section class="diagnostics-panel search-diagnostics">
              <div class="diagnostics-heading">
                <h3>Search Diagnostics</h3>
                <span>{tradeResult.diagnostics.length}</span>
              </div>
              <div class="diagnostic-list">
                {#each tradeResult.diagnostics as diagnostic}
                  <article class="diagnostic-row">
                    <code>{diagnostic.code}</code>
                    <strong>{diagnostic.message}</strong>
                    {#if diagnostic.detail}
                      <p>{diagnostic.detail}</p>
                    {/if}
                  </article>
                {/each}
              </div>
            </section>
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
                      <div class="listing-actions">
                        {#if listing.canTeleport}
                          <button
                            class="tp-button"
                            type="button"
                            onclick={() => teleportToHideout(listing)}
                            disabled={Boolean(teleportingListingId)}
                            title="Teleport to seller hideout"
                          >
                            {teleportButtonLabel(listing)}
                          </button>
                        {/if}
                        <strong>{formatPrice(listing.price)}</strong>
                      </div>
                    </div>

                    <div class="seller-line">
                      <span>{listing.accountName ?? "Unknown seller"}</span>
                      {#if listing.indexed}
                        <span>{listing.indexed}</span>
                      {/if}
                    </div>

                    {#if teleportStatuses[listing.id]}
                      <p class:error={isTeleportError(teleportStatuses[listing.id])} class="teleport-status">
                        {teleportStatuses[listing.id]}
                      </p>
                    {/if}

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
    </section>
  </section>
</main>

<style>
  :root {
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #f4efe6;
    background: #090a0b;
    font-synthesis: none;
    text-rendering: optimizeLegibility;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
    --background: #090a0b;
    --surface: #111315;
    --surface-subtle: #171a1d;
    --surface-muted: #1f2428;
    --ink: #f4efe6;
    --ink-soft: #d7d0c3;
    --muted: #9ba3ad;
    --line: #2a3035;
    --line-strong: #3a4248;
    --field: #0d0f11;
    --primary: #d6b36a;
    --primary-hover: #e4c77e;
    --danger: #f87171;
    --danger-bg: #2a1417;
    --warning: #f0b45b;
    --warning-bg: #261a0d;
    --success: #62d394;
    --blue: #6db7ff;
    --shadow-soft: 0 16px 44px rgba(0, 0, 0, 0.36);
    --shadow-card: 0 10px 26px rgba(0, 0, 0, 0.24);
  }

  :global(body) {
    min-width: 320px;
    min-height: 100vh;
    margin: 0;
    background:
      radial-gradient(circle at 18% 0%, rgba(214, 179, 106, 0.16), transparent 30rem),
      radial-gradient(circle at 82% 12%, rgba(45, 159, 137, 0.11), transparent 28rem),
      linear-gradient(180deg, #111315 0%, var(--background) 22rem);
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  button {
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    transition:
      background 140ms ease,
      border-color 140ms ease,
      color 140ms ease,
      box-shadow 140ms ease;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  button:not(:disabled):hover {
    box-shadow: var(--shadow-card);
  }

  h1,
  h2,
  h3,
  h4,
  p {
    margin: 0;
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
    min-height: 64px;
    padding: 0 22px;
    border-bottom: 1px solid var(--line);
    background: rgba(12, 14, 16, 0.88);
    backdrop-filter: blur(18px);
    position: sticky;
    top: 0;
    z-index: 10;
  }

  .brand-lockup,
  .topbar-controls,
  .actions,
  .seller-line {
    display: flex;
    align-items: center;
  }

  .brand-lockup {
    gap: 12px;
    min-width: 0;
  }

  .brand-mark {
    display: grid;
    width: 32px;
    height: 32px;
    place-items: center;
    border: 1px solid var(--ink);
    border-radius: 8px;
    color: #111315;
    background: var(--primary);
    font-size: 0.84rem;
    font-weight: 800;
  }

  h1 {
    font-size: 0.96rem;
    line-height: 1.2;
  }

  .brand-lockup p,
  .eyebrow,
  .topbar p,
  .item-summary span,
  .update-line span,
  .actions span,
  .filter-group-heading span,
  .diagnostics-heading span {
    color: var(--muted);
    font-size: 0.78rem;
  }

  .eyebrow {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .topnav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .topnav a {
    min-height: 30px;
    display: inline-flex;
    align-items: center;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 0.82rem;
    font-weight: 600;
    text-decoration: none;
  }

  .topnav a:hover {
    color: var(--ink);
    background: var(--surface);
  }

  .topbar-controls {
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
  }

  .status-pill {
    display: grid;
    gap: 2px;
    min-width: 72px;
    padding: 6px 9px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
  }

  .status-pill span,
  .league-field {
    color: var(--muted);
    font-size: 0.76rem;
  }

  .status-pill strong {
    color: var(--ink);
    font-size: 0.9rem;
  }

  .league-field {
    display: grid;
    gap: 4px;
    min-width: 180px;
  }

  .league-field input,
  .manual-input textarea {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--field);
    color: var(--ink);
    outline: none;
  }

  .league-field input:focus,
  .manual-input textarea:focus {
    border-color: var(--ink);
    box-shadow: 0 0 0 3px rgba(214, 179, 106, 0.18);
  }

  .league-field input {
    height: 34px;
    padding: 0 10px;
  }

  .workspace {
    display: grid;
    grid-template-columns: minmax(320px, 410px) minmax(0, 1fr);
    gap: 16px;
    width: min(1520px, calc(100vw - 32px));
    margin: 0 auto;
    padding: 18px 0 30px;
  }

  .capture-panel,
  .item-panel {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow-soft);
  }

  .capture-panel {
    align-self: start;
    display: grid;
    gap: 12px;
    padding: 14px;
    position: sticky;
    top: 82px;
  }

  .item-panel {
    min-height: 520px;
    padding: 16px;
  }

  .composer-intro {
    display: grid;
    gap: 4px;
    padding: 8px 4px 4px;
  }

  .composer-intro h2 {
    font-size: clamp(1.45rem, 2vw, 1.9rem);
    line-height: 1.05;
  }

  .composer-intro p {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .prompt-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .prompt-chips span,
  .heading-pills span,
  .actions span {
    min-height: 26px;
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--surface);
    color: var(--muted);
    padding: 0 9px;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .composer-card {
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow-card);
    overflow: hidden;
  }

  .composer-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px;
    border-top: 1px solid var(--line);
    background: var(--surface-subtle);
  }

  .composer-actions span {
    margin-left: auto;
    color: var(--muted);
    font-size: 0.78rem;
  }

  .primary-action,
  .secondary-action {
    min-height: 36px;
    padding: 0 14px;
    font-weight: 700;
  }

  .primary-action {
    color: #111315;
    background: var(--primary);
  }

  .secondary-action {
    border-color: var(--line);
    color: var(--ink);
    background: var(--surface);
  }

  .primary-action:not(:disabled):hover {
    background: var(--primary-hover);
  }

  .secondary-action:not(:disabled):hover {
    border-color: var(--line-strong);
    background: var(--surface-subtle);
  }

  .manual-input {
    display: grid;
    gap: 6px;
    color: var(--muted);
    font-size: 0.82rem;
  }

  .manual-input span {
    padding: 10px 10px 0;
    font-weight: 700;
  }

  .manual-input textarea {
    min-height: 250px;
    resize: vertical;
    padding: 10px;
    border: 0;
    border-radius: 0;
    background: transparent;
    line-height: 1.35;
    white-space: pre;
  }

  .update-line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
    font-size: 0.82rem;
  }

  .update-line span {
    color: var(--muted);
  }

  .update-line strong {
    color: var(--ink);
    text-align: right;
  }

  .bridge-card {
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
    font-size: 0.8rem;
  }

  .bridge-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .bridge-heading span,
  .bridge-card p {
    color: var(--muted);
  }

  .bridge-heading strong {
    color: #fde68a;
  }

  .bridge-heading strong.ready {
    color: var(--success);
  }

  .bridge-card code {
    overflow-wrap: anywhere;
    color: var(--ink);
    font-size: 0.74rem;
  }

  .workspace-heading {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--line);
  }

  .workspace-heading h2 {
    margin-top: 4px;
    font-size: clamp(1.25rem, 2vw, 1.75rem);
    line-height: 1.1;
  }

  .filter-tabs {
    display: flex;
    gap: 4px;
    width: fit-content;
    max-width: 100%;
    margin: 14px 0;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .filter-tabs button {
    min-height: 34px;
    padding: 0 12px;
    border-color: transparent;
    color: var(--muted);
    background: transparent;
    font-size: 0.82rem;
    font-weight: 800;
  }

  .filter-tabs button.active-tab {
    border-color: var(--line);
    color: var(--ink);
    background: var(--surface);
    box-shadow: var(--shadow-card);
  }

  .quick-filter-panel {
    display: grid;
    gap: 14px;
    margin-top: 4px;
  }

  .quick-filter-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 12px;
    padding: 14px;
    border: 1px solid rgba(214, 179, 106, 0.34);
    border-radius: 8px;
    background:
      linear-gradient(135deg, rgba(214, 179, 106, 0.16), rgba(45, 159, 137, 0.12)),
      var(--surface);
  }

  .quick-filter-heading h3 {
    margin-top: 4px;
    color: var(--ink);
    font-size: 1.1rem;
  }

  .quick-filter-heading > span {
    display: inline-flex;
    min-height: 28px;
    align-items: center;
    padding: 0 10px;
    border: 1px solid rgba(214, 179, 106, 0.32);
    border-radius: 8px;
    color: #f2d89a;
    background: rgba(214, 179, 106, 0.12);
    font-size: 0.78rem;
    font-weight: 800;
  }

  .jewel-picker {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 8px;
  }

  .jewel-picker button {
    display: grid;
    gap: 5px;
    min-height: 68px;
    padding: 11px;
    border-color: var(--line);
    color: var(--ink);
    background: var(--surface);
    text-align: left;
  }

  .jewel-picker button:hover,
  .jewel-picker button.jewel-active {
    border-color: var(--primary);
    background: rgba(214, 179, 106, 0.12);
  }

  .jewel-picker strong,
  .quick-chip span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .jewel-picker span {
    color: var(--muted);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .quick-builder {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) auto auto;
    gap: 10px;
    align-items: end;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .quick-builder label {
    display: grid;
    gap: 6px;
    color: var(--muted);
    font-size: 0.8rem;
    font-weight: 800;
  }

  .quick-builder select {
    width: 100%;
    min-height: 38px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    color: var(--ink);
    background: var(--surface);
    outline: none;
  }

  .quick-builder select:focus {
    border-color: var(--ink);
    box-shadow: 0 0 0 3px rgba(214, 179, 106, 0.18);
  }

  .quick-chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .quick-chip {
    display: flex;
    max-width: 100%;
    min-height: 38px;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 7px 9px;
    border: 1px solid rgba(45, 159, 137, 0.46);
    border-radius: 8px;
    color: #d9fff5;
    background: rgba(45, 159, 137, 0.14);
    font-size: 0.78rem;
    font-weight: 800;
  }

  .quick-chip > button {
    min-height: 28px;
    padding: 0 7px;
    border-color: rgba(45, 159, 137, 0.46);
    color: #8ee6d4;
    background: rgba(10, 16, 18, 0.5);
    font-size: 0.72rem;
    font-weight: 800;
  }

  .heading-pills {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
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
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .item-summary strong {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .profile-panel {
    display: grid;
    gap: 10px;
    margin: 0 0 16px;
    padding: 12px;
    border: 1px solid rgba(214, 179, 106, 0.28);
    border-radius: 8px;
    background:
      linear-gradient(135deg, rgba(214, 179, 106, 0.08), rgba(45, 159, 137, 0.06)),
      var(--surface-subtle);
  }

  .profile-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 12px;
  }

  .profile-heading h3 {
    margin-top: 4px;
    color: var(--ink);
    font-size: 0.98rem;
  }

  .profile-heading > span {
    display: inline-flex;
    min-height: 24px;
    align-items: center;
    padding: 0 9px;
    border: 1px solid rgba(214, 179, 106, 0.3);
    border-radius: 999px;
    color: #f2d89a;
    background: rgba(214, 179, 106, 0.1);
    font-size: 0.72rem;
    font-weight: 800;
  }

  .profile-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .profile-grid button {
    display: grid;
    gap: 5px;
    min-height: 86px;
    padding: 10px;
    border-color: var(--line);
    color: var(--ink);
    background: var(--surface);
    text-align: left;
  }

  .profile-grid button:hover,
  .profile-grid button.profile-active {
    border-color: var(--primary);
    background: rgba(214, 179, 106, 0.12);
  }

  .profile-grid strong,
  .profile-grid span,
  .profile-grid small {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .profile-grid span {
    color: var(--muted);
    font-size: 0.76rem;
    line-height: 1.3;
  }

  .profile-grid small {
    color: #8ee6d4;
    font-size: 0.72rem;
    font-weight: 800;
  }

  .filters {
    display: grid;
    gap: 14px;
  }

  .filter-group {
    display: grid;
    gap: 8px;
  }

  .filter-group-heading,
  .diagnostics-heading,
  .results-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .filter-group h3 {
    color: #f4efe6;
    font-size: 0.86rem;
  }

  .filter-group-heading span,
  .diagnostics-heading span {
    display: inline-grid;
    min-width: 28px;
    min-height: 22px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--surface-subtle);
    color: var(--muted);
    font-weight: 800;
  }

  .filter-row {
    display: grid;
    grid-template-columns: 18px 1fr;
    gap: 8px 10px;
    align-items: start;
    min-height: 44px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink-soft);
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .filter-row:hover {
    border-color: var(--line-strong);
    background: var(--surface-subtle);
  }

  .filter-row > input[type="checkbox"] {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    accent-color: var(--primary);
  }

  .filter-content {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .filter-meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.3;
  }

  .filter-meta strong {
    display: inline-flex;
    min-height: 22px;
    align-items: center;
    padding: 0 7px;
    border: 1px solid rgba(45, 159, 137, 0.36);
    border-radius: 999px;
    color: #8ee6d4;
    background: rgba(45, 159, 137, 0.12);
    font-size: 0.7rem;
  }

  .filter-meta strong.affix-badge {
    border-color: rgba(214, 179, 106, 0.38);
    color: #f2d89a;
    background: rgba(214, 179, 106, 0.12);
  }

  .filter-meta strong.source-badge {
    border-color: rgba(109, 183, 255, 0.34);
    color: #9ccfff;
    background: rgba(109, 183, 255, 0.1);
  }

  .filter-meta small {
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--muted);
  }

  .filter-row span,
  .filter-content span {
    min-width: 0;
    overflow-wrap: anywhere;
    line-height: 1.35;
  }

  .range-controls {
    display: grid;
    grid-template-columns: repeat(2, minmax(86px, 120px));
    gap: 8px;
    max-width: 260px;
  }

  .range-controls label {
    display: grid;
    gap: 4px;
    color: var(--muted);
    font-size: 0.7rem;
    font-weight: 800;
    text-transform: uppercase;
  }

  .range-controls input {
    min-width: 0;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 7px;
    color: var(--ink);
    background: var(--field);
    outline: none;
  }

  .range-controls input:focus {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px rgba(214, 179, 106, 0.18);
  }

  .compact-range {
    grid-template-columns: repeat(2, 82px);
    max-width: none;
  }

  .filter-row > small {
    grid-column: 2;
    color: var(--warning);
    line-height: 1.3;
  }

  .filter-row.unsupported {
    color: var(--muted);
    background: var(--surface-subtle);
  }

  .actions {
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 18px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }

  .status-text {
    margin-top: 12px;
    color: var(--success);
    font-weight: 700;
  }

  .notice-card {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    line-height: 1.35;
  }

  .notice-card strong {
    font-size: 0.84rem;
  }

  .notice-card p {
    overflow-wrap: anywhere;
  }

  .error-card {
    border-color: rgba(248, 113, 113, 0.34);
    color: #fecaca;
    background: var(--danger-bg);
  }

  .compact {
    font-size: 0.78rem;
  }

  .search-error {
    margin-top: 12px;
  }

  .warning {
    padding: 10px 12px;
    border: 1px solid rgba(240, 180, 91, 0.34);
    border-radius: 8px;
    color: #fde68a;
    background: var(--warning-bg);
    line-height: 1.35;
  }

  .diagnostics-panel {
    display: grid;
    gap: 10px;
    margin: 14px 0;
    padding: 12px;
    border: 1px solid rgba(248, 113, 113, 0.34);
    border-radius: 8px;
    background: var(--danger-bg);
  }

  .diagnostics-heading h3 {
    color: #fecaca;
    font-size: 0.88rem;
  }

  .diagnostic-list {
    display: grid;
    gap: 8px;
  }

  .diagnostic-row {
    display: grid;
    gap: 5px;
    padding: 9px 10px;
    border: 1px solid rgba(248, 113, 113, 0.24);
    border-radius: 8px;
    background: var(--surface);
  }

  .diagnostic-row code {
    width: fit-content;
    max-width: 100%;
    overflow-wrap: anywhere;
    padding: 2px 6px;
    border: 1px solid rgba(248, 113, 113, 0.34);
    border-radius: 6px;
    color: #fecaca;
    background: rgba(248, 113, 113, 0.09);
    font-size: 0.76rem;
  }

  .diagnostic-row strong,
  .diagnostic-row p {
    overflow-wrap: anywhere;
  }

  .diagnostic-row p {
    color: var(--muted);
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .search-diagnostics {
    margin: 0;
  }

  .results-panel {
    display: grid;
    gap: 12px;
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
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
    color: var(--muted);
    font-size: 0.82rem;
  }

  .results-heading span {
    max-width: 42%;
    overflow-wrap: anywhere;
    text-align: right;
  }

  .listing-list {
    display: grid;
    gap: 10px;
  }

  .listing-row {
    display: grid;
    grid-template-columns: 78px minmax(0, 1fr);
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow-card);
  }

  .listing-image {
    display: grid;
    width: 78px;
    min-height: 78px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .listing-image img {
    max-width: 66px;
    max-height: 66px;
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

  .listing-actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: end;
    gap: 8px;
  }

  .tp-button {
    min-height: 28px;
    padding: 0 10px;
    border-color: rgba(45, 159, 137, 0.45);
    color: #cfe8dc;
    background: #1f332c;
    font-size: 0.78rem;
    font-weight: 800;
  }

  .tp-button:not(:disabled):hover {
    border-color: rgba(45, 159, 137, 0.8);
    background: #25483e;
  }

  .listing-title h4 {
    font-size: 0.95rem;
    line-height: 1.25;
    overflow-wrap: anywhere;
  }

  .listing-title strong {
    flex: 0 0 auto;
    padding: 4px 8px;
    border-radius: 8px;
    color: #111315;
    background: var(--primary);
    font-size: 0.86rem;
    white-space: nowrap;
  }

  .seller-line {
    flex-wrap: wrap;
    gap: 8px 14px;
  }

  .teleport-status {
    color: var(--success);
    font-size: 0.78rem;
    font-weight: 700;
    line-height: 1.35;
  }

  .teleport-status.error {
    color: #ffb4a9;
  }

  .mod-list {
    display: grid;
    gap: 4px;
    color: var(--ink-soft);
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .mod-list span {
    overflow-wrap: anywhere;
  }

  .pseudo-mods {
    color: var(--blue);
    font-weight: 700;
  }

  .results-empty {
    display: grid;
    min-height: 90px;
    place-items: center;
    border: 1px dashed var(--line);
    border-radius: 8px;
    background: var(--surface-subtle);
  }

  .empty-state {
    display: grid;
    min-height: 420px;
    place-items: center;
    align-content: center;
    gap: 8px;
    border: 1px dashed var(--line);
    border-radius: 8px;
    color: var(--muted);
    background: var(--surface-subtle);
  }

  .empty-state h3 {
    color: var(--ink);
    font-size: 1rem;
  }

  .empty-mark {
    display: grid;
    width: 36px;
    height: 36px;
    place-items: center;
    border: 1px solid var(--ink);
    border-radius: 8px;
    color: var(--surface);
    background: var(--ink);
    font-weight: 800;
  }

  @media (max-width: 820px) {
    .workspace {
      grid-template-columns: 1fr;
      width: min(100vw - 20px, 760px);
      padding-top: 10px;
    }

    .topbar {
      align-items: stretch;
      flex-direction: column;
      padding: 12px;
    }

    .topbar-controls {
      justify-content: stretch;
    }

    .topnav {
      justify-content: stretch;
    }

    .topnav a {
      flex: 1;
      justify-content: center;
    }

    .status-pill,
    .league-field {
      flex: 1 1 150px;
    }

    .capture-panel {
      position: static;
    }

    .item-summary {
      grid-template-columns: repeat(2, minmax(120px, 1fr));
    }

    .quick-builder {
      grid-template-columns: 1fr;
    }

    .profile-grid {
      grid-template-columns: 1fr;
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

    .listing-title strong {
      width: fit-content;
    }
  }

  @media (max-width: 560px) {
    .item-summary {
      grid-template-columns: 1fr;
    }

    .listing-row {
      grid-template-columns: 1fr;
    }

    .listing-image {
      width: 100%;
      min-height: 92px;
    }

    .results-heading {
      align-items: start;
      flex-direction: column;
    }

    .results-heading span {
      max-width: 100%;
      text-align: left;
    }
  }
</style>
