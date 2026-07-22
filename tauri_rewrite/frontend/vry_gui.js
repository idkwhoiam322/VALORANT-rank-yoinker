// Tauri IPC bridge
function pickTauriInvoke() {
    let t = window.__TAURI__;
    if (t && t.core && t.core.invoke) return t.core.invoke;
    if (t && t.invoke) return t.invoke;
    return null;
}
function pickTauriListen() {
    let t = window.__TAURI__;
    if (t && t.event && t.event.listen) return t.event.listen;
    if (t && t.listen) return t.listen;
    return null;
}
let _tauriInvoke = pickTauriInvoke();
let _tauriListen = pickTauriListen();
let tauriInvoke = _tauriInvoke || (() => Promise.reject(new Error('IPC unavailable')));
let tauriListen = _tauriListen || (() => Promise.resolve(() => {}));
if (!_tauriInvoke && !_tauriListen) {
    console.warn('[VRY] Tauri IPC global not detected; the app appears to be running outside the Tauri runtime. Backend-dependent features will be unavailable.');
}

(function () {
    if (!_tauriInvoke) return;
    ["log", "warn", "error", "debug", "info"].forEach(function (level) {
        let original = console[level];
        console[level] = function () {
            let msg = Array.prototype.map.call(arguments, function (a) {
                return typeof a === "object" ? JSON.stringify(a) : String(a);
            }).join(" ");
            original.apply(console, arguments);
            _tauriInvoke("log_frontend", { msg: msg, level: level }).catch(function () {});
        };
    });
})();

window.addEventListener("error", function (e) {
    console.error("[VRY] Uncaught error:", e.message || e.error, e.filename || "", e.lineno || 0);
});
window.addEventListener("unhandledrejection", function (e) {
    console.error("[VRY] Unhandled promise rejection:", e.reason);
});

(function () {
    "use strict";

    let COPY_HINT = " (Right-click to copy)";
    let stripHint = function (t) { return t ? t.replace(COPY_HINT, "") : ""; };
    let chromaColor = function (name) { let m = name && name.match(/\(Variant \d+ (.+)\)$/); return m ? m[1] : ""; };

    // Reject any URL that is not https: or data: so payload-derived values can
    // never inject javascript:/other schemes into DOM attributes (img src,
    // background-image, etc.). Also reject raw quote/backslash/backtick/angle/
    // control characters, which a valid URL would carry percent-encoded; those
    // bytes would otherwise break out of a CSS url("-") or attribute context.
    // Returns "" when the input is unsafe/empty.
    let safeHttps = function (url) {
        if (!url || typeof url !== "string") return "";
        if (/["\\<>`\r\n]/.test(url)) return "";
        try {
            let u = new URL(url, window.location.href);
            if (u.protocol === "https:" || u.protocol === "data:") return url;
        } catch (e) {
            console.error("[VRY] safeHttps URL parse error:", e, url);
        }
        return "";
    };

    let RANK_NAMES_FULL = [
        "Unranked", "Unranked", "Unranked",
        "Iron 1", "Iron 2", "Iron 3",
        "Bronze 1", "Bronze 2", "Bronze 3",
        "Silver 1", "Silver 2", "Silver 3",
        "Gold 1", "Gold 2", "Gold 3",
        "Platinum 1", "Platinum 2", "Platinum 3",
        "Diamond 1", "Diamond 2", "Diamond 3",
        "Ascendant 1", "Ascendant 2", "Ascendant 3",
        "Immortal 1", "Immortal 2", "Immortal 3",
        "Radiant"
    ];

    let RANK_NAMES_SHORT = [
        "UnR", "UnR", "UnR",
        "Iron 1", "Iron 2", "Iron 3",
        "Bron 1", "Bron 2", "Bron 3",
        "Silv 1", "Silv 2", "Silv 3",
        "Gold 1", "Gold 2", "Gold 3",
        "Plat 1", "Plat 2", "Plat 3",
        "Dia 1", "Dia 2", "Dia 3",
        "Asc 1", "Asc 2", "Asc 3",
        "Imm 1", "Imm 2", "Imm 3",
        "Rad"
    ];

    let RANK_COLORS = [
        [46, 46, 46], [46, 46, 46], [46, 46, 46],
        [72, 69, 62], [72, 69, 62], [72, 69, 62],
        [187, 143, 90], [187, 143, 90], [187, 143, 90],
        [174, 178, 178], [174, 178, 178], [174, 178, 178],
        [197, 186, 63], [197, 186, 63], [197, 186, 63],
        [24, 167, 185], [24, 167, 185], [24, 167, 185],
        [216, 100, 199], [216, 100, 199], [216, 100, 199],
        [24, 148, 82], [24, 148, 82], [24, 148, 82],
        [221, 68, 68], [221, 68, 68], [221, 68, 68],
        [255, 253, 205]
    ];

    let NA = "N/A";
    let PREVIEW_WEAPONS = ["Vandal", "Phantom", "Melee"];

    let state = {
        payload: null,
        players: [],
        selectedPuuid: null,
        // Identity (puuid) of the current match/account. Used to decide whether a
        // selected player's disappearance is a real match change (drop selection)
        // or a transient per-tick absence (keep selection). Undefined until the
        // first payload arrives.
        matchPuuid: undefined,
        lastRenderKey: null,
        // Backend session id (bumped on every restart/re-auth). Heartbeats and
        // state_change carrying a different sessionId are stale and dropped.
        epoch: null,
        rankIcons: null,
        lastGameState: null,
        // State of the last heartbeat we rendered. Used by renderStateTransition
        // to decide the from->to transition without depending on state_change
        // event ordering (see renderStateTransition for the race explanation).
        prevGameState: null,
        playerButtons: new Map(),
        dirty: { meta: true, players: true, playedWith: true, details: true, json: true },
    };

    // Dedicated buffer for the loading-overlay log tail. Keeping the lines in
    // an array (instead of re-parsing DOM textContent on every event) avoids
    // the O(n^2) split/join growth from re-parsing DOM textContent on every event.
    let logLines = [];

    // Handle for the pending screenshot toast-revert timer.
    // Tracked so rapid re-clicks clear the previous timer instead of stacking.
    let screenshotToastTimer = null;

    let els = {
        blueGrid: document.getElementById("blueGrid"),
        redGrid: document.getElementById("redGrid"),
        detailsPanel: document.getElementById("detailsPanel"),
        emptyState: document.getElementById("emptyState"),
        emptyStateTitle: document.getElementById("emptyStateTitle"),
        emptyStateText: document.getElementById("emptyStateText"),
        selectedAgent: document.getElementById("selectedAgent"),
        selectedTeam: document.getElementById("selectedTeam"),
        selectedName: document.getElementById("selectedName"),
        trnLink: document.getElementById("trnLink"),
        vtlLink: document.getElementById("vtlLink"),
        topTrnLink: document.getElementById("topTrnLink"),
        topVtlLink: document.getElementById("topVtlLink"),
        copyStatsBtn: document.getElementById("copyStatsBtn"),
        screenshotPlayerRow: document.getElementById("screenshotPlayerRow"),
        screenshotPlayerBtn: document.getElementById("screenshotPlayerBtn"),
        selectedCardTitle: document.getElementById("selectedCardTitle"),
        selectedModalName: document.getElementById("selectedModalName"),
        selectedLevel: document.getElementById("selectedLevel"),
        playerCardPreview: document.getElementById("playerCardPreview"),
        expressionGrid: document.getElementById("expressionGrid"),
        weaponGroups: document.getElementById("weaponGroups"),
        closeDetailsButton: document.getElementById("closeDetailsButton"),
        statBar: document.getElementById("statBar"),
        statusPill: document.getElementById("statusPill"),
        statusText: document.getElementById("statusText"),
        loadingOverlay: document.getElementById("loadingOverlay"),
        loadingRefreshButton: document.getElementById("loadingRefreshButton"),
        loadingLogTail: document.getElementById("loadingLogTail"),
        matchMeta: document.getElementById("matchMeta"),
        refreshButton: document.getElementById("refreshButton"),
        confirmModal: document.getElementById("confirmModal"),
        modalCancel: document.getElementById("modalCancel"),
        modalConfirm: document.getElementById("modalConfirm"),
        toast: document.getElementById("toast"),
        jsonPre: document.getElementById("jsonPre"),
        jsonCopyBtn: document.getElementById("jsonCopyBtn"),
        defHeader: document.getElementById("defHeader"),
        atkHeader: document.getElementById("atkHeader"),
        defSection: document.getElementById("defSection"),
        atkSection: document.getElementById("atkSection"),
        teamDivider: document.getElementById("teamDivider"),
        screenshotButton: document.getElementById("screenshotButton"),
        playedWithEmpty: document.getElementById("playedWithEmpty"),
        playedWithPanel: document.getElementById("playedWithPanel"),
        playedWithTable: document.getElementById("playedWithTable"),
        playedWithBody: document.getElementById("playedWithBody"),
        logPanel: document.getElementById("logPanel"),
        logPre: document.getElementById("logPre"),
        logCopyBtn: document.getElementById("logCopyBtn"),
        hbPanel: document.getElementById("hbPanel"),
        hbPre: document.getElementById("hbPre"),
        hbCopyBtn: document.getElementById("hbCopyBtn"),
        hbRefreshBtn: document.getElementById("hbRefreshBtn"),
        teamsLayout: document.querySelector(".teams-layout"),
        metaUpdatedChip: null,
    };

    let WEAPON_COLUMNS = [
        { className: "weapon-column-sidearms", groups: [{ title: "Sidearms", slug: "sidearms", weapons: ["Classic", "Shorty", "Frenzy", "Ghost", "Bandit", "Sheriff"] }] },
        { className: "weapon-column-smgs", groups: [
            { title: "SMGs", slug: "smgs", weapons: ["Stinger", "Spectre"] },
            { title: "Shotguns", slug: "shotguns", weapons: ["Bucky", "Judge"] },
        ] },
        { className: "weapon-column-rifles", groups: [
            { title: "Rifles", slug: "rifles", weapons: ["Bulldog", "Guardian", "Phantom", "Vandal"] },
            { title: "Melee", slug: "melee", weapons: ["Melee"] },
        ] },
        { className: "weapon-column-heavy", groups: [
            { title: "Sniper Rifles", slug: "sniper-rifles", weapons: ["Marshal", "Outlaw", "Operator"] },
            { title: "Machine Guns", slug: "machine-guns", weapons: ["Ares", "Odin"] },
        ] },
    ];

    // ---- helpers ----
    function isEmpty(v) { return v === null || v === undefined || v === "" || (typeof v === "number" && Number.isNaN(v)); }
    function txt(v, fb) { return isEmpty(v) ? (fb === undefined ? NA : fb) : String(v); }

    // Centralized state writer. All top-level mutations of the
    // global `state` object now flow through here so writes are explicit and
    // auditable; it pairs with the dirty-flag change detection added for 4.9/1.4.
    function setState(partial) { Object.assign(state, partial); }

    function rankName(idx, short) {
        if (isEmpty(idx)) return NA;
        let n = Number(idx);
        let t = short ? RANK_NAMES_SHORT : RANK_NAMES_FULL;
        if (Number.isNaN(n) || n < 0 || n >= t.length) return NA;
        return t[n];
    }

    function rankColor(idx) {
        let n = Number(idx);
        if (isEmpty(idx) || Number.isNaN(n) || n < 0 || n >= RANK_COLORS.length) return null;
        let rgb = RANK_COLORS[n];
        return "rgb(" + rgb[0] + ", " + rgb[1] + ", " + rgb[2] + ")";
    }

    function winRateDisplay(v) {
        if (isEmpty(v)) return NA;
        let s = String(v);
        if (s.indexOf("%") !== -1) return s;
        let m = s.match(/^(-?[\d.]+)(\s*\(.*)$/);
        return m ? (m[1] + "%" + m[2]) : s;
    }

    function stripWeaponName(skinName, weaponName) {
        if (!skinName) return skinName;
        return skinName.replace(new RegExp("\\b" + weaponName + "\\b", "ig"), "").replace(/\s+/g, " ").trim() || skinName;
    }

    function formatTimeAgo(seconds) {
        let s = Math.max(0, Math.floor(Number(seconds) || 0));
        if (s < 60) return s + (s === 1 ? " second" : " seconds");
        if (s < 3600) { let m = Math.floor(s / 60); return m + (m === 1 ? " minute" : " minutes"); }
        if (s < 86400) { let h = Math.floor(s / 3600); return h + (h === 1 ? " hour" : " hours"); }
        let d = Math.floor(s / 86400);
        return d + (d === 1 ? " day" : " days");
    }

    function formatEncounterTimes(entry) {
        let parts = [];
        if (entry.ally_count) parts.push(entry.ally_count + " as ally");
        if (entry.enemy_count) parts.push(entry.enemy_count + " as enemy");
        let breakdown = parts.length ? " (" + parts.join(", ") + ")" : "";
        return txt(entry.times, "0") + breakdown;
    }

    function formatEncounterRecord(entry) {
        let parts = [];
        if (entry.ally_count) {
            let r = entry.ally_wins + "W-" + entry.ally_losses + "L";
            if (entry.ally_unknown) r += " (" + entry.ally_unknown + " unknown)";
            parts.push("Ally " + r);
        }
        if (entry.enemy_count) {
            let r2 = entry.enemy_wins + "W-" + entry.enemy_losses + "L";
            if (entry.enemy_unknown) r2 += " (" + entry.enemy_unknown + " unknown)";
            parts.push("Enemy " + r2);
        }
        return parts.length ? parts.join(" / ") : NA;
    }

    function setStatChip(container, label, value, isNA, chipClass, textColor, iconUrl) {
        let chip = document.createElement("div");
        chip.className = chipClass || "stat-chip";
        let l = document.createElement("span");
        l.className = "stat-label";
        l.textContent = label;
        let v = document.createElement("span");
        v.className = "stat-value" + (isNA ? " na" : "");
        if (iconUrl) {
            let img = document.createElement("img");
            img.className = "rank-icon";
            img.src = safeHttps(iconUrl);
            img.loading = "lazy";
            v.append(img);
            v.append(" ");
        }
        v.append(document.createTextNode(value));
        if (textColor && !isNA) v.style.color = textColor;
        chip.append(l, v);
        container.append(chip);
    }

    function buildStatChips(player, chipClass, shortName, compSuffix) {
        let frag = document.createDocumentFragment();
        let comp = compSuffix ? " (Comp)" : "";
        setStatChip(frag, "Win Rate" + comp, winRateDisplay(player.winPercentage), isEmpty(player.winPercentage), chipClass);
        setStatChip(frag, "Rank", rankName(player.rank, shortName), isEmpty(player.rank), chipClass, rankColor(player.rank), state.rankIcons && state.rankIcons[player.rank]);
        setStatChip(frag, "RR", txt(player.rr), isEmpty(player.rr), chipClass);
        setStatChip(frag, "Leaderboard", isEmpty(player.leaderboard) ? NA : (Number(player.leaderboard) <= 0 ? NA : "#" + player.leaderboard), isEmpty(player.leaderboard), chipClass);
        setStatChip(frag, "Peak Rank", (function(rn){return rn!==NA&&player.peakRankAct?rn+String(player.peakRankAct).trim():rn})(rankName(player.peakRank, shortName)), isEmpty(player.peakRank), chipClass, rankColor(player.peakRank), state.rankIcons && state.rankIcons[player.peakRank]);
        setStatChip(frag, "Last Act", rankName(player.previousRank, shortName), isEmpty(player.previousRank), chipClass, rankColor(player.previousRank), state.rankIcons && state.rankIcons[player.previousRank]);
        setStatChip(frag, "Level", txt(player.level), isEmpty(player.level), chipClass);
        setStatChip(frag, "Last Active" + comp, txt(player.lastActive), isEmpty(player.lastActive), chipClass);
        return frag;
    }

    function buildCardStats(player) {
        let grid = document.createElement("div");
        grid.className = "card-stats";
        grid.append(buildStatChips(player, "card-stat", true));
        return grid;
    }

    function showToast(message) {
        els.toast.textContent = message;
        els.toast.classList.add("show");
        clearTimeout(showToast._t);
        showToast._t = setTimeout(function () { els.toast.classList.remove("show"); }, 3200);
    }

    // Revert the screenshot success/error toast state after `ms`, clearing any
    // previously scheduled revert first so timers don't pile up.
    function revertScreenshotToast(ms) {
        if (screenshotToastTimer) clearTimeout(screenshotToastTimer);
        screenshotToastTimer = setTimeout(function () {
            els.toast.classList.remove("is-success");
            els.toast.classList.remove("is-error");
            screenshotToastTimer = null;
        }, ms);
    }

    async function captureToClipboard(target, buttonEl, opts) {
        if (!target) { showToast("No target element."); return; }
        buttonEl.disabled = true;
        let cleanup = [];
        function doCleanup() { cleanup.forEach(function (fn) { fn(); }); }

        if (opts.overlayStyle) {
            let s = document.createElement("style");
            s.id = opts.styleId || "tmp-scr";
            s.textContent = opts.overlayStyle;
            document.head.appendChild(s);
            cleanup.push(function () { let el = document.getElementById(s.id); if (el) el.remove(); });
        }
        if (opts.hideDetails) {
            let panel = els.detailsPanel;
            if (!panel.hidden) { panel.hidden = true; cleanup.push(function () { panel.hidden = false; }); }
        }
        if (opts.showBadge) document.body.classList.add("show-you-badge");

        try {
            let canvas = await html2canvas(target, { scale: 2, useCORS: true, backgroundColor: "#0f1115" });
            doCleanup();
            if (opts.showBadge) document.body.classList.remove("show-you-badge");
            let blob = await new Promise(function (resolve) { canvas.toBlob(resolve, "image/png"); });
            if (!blob) {
                if (opts.styleToast) { els.toast.classList.add("is-error"); revertScreenshotToast(600); }
                console.error("[VRY] Screenshot capture produced no blob");
                showToast("Screenshot failed."); return;
            }
            if (navigator.clipboard && navigator.clipboard.write) {
                try {
                    await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
                    if (opts.styleToast) { els.toast.classList.add("is-success"); revertScreenshotToast(600); }
                    showToast("Screenshot copied!");
                } catch (e) {
                    if (opts.styleToast) { els.toast.classList.add("is-error"); revertScreenshotToast(600); }
                    console.error("[VRY] Clipboard write failed:", e);
                    showToast("Screenshot copy failed.");
                }
            } else {
                if (opts.styleToast) { els.toast.classList.add("is-error"); revertScreenshotToast(600); }
                console.error("[VRY] Clipboard API unavailable");
                showToast("Clipboard API unavailable.");
            }
        } catch (e) {
            if (opts.styleToast) { els.toast.classList.add("is-error"); revertScreenshotToast(600); }
            console.error("[VRY] Screenshot capture failed:", e);
            showToast("Screenshot failed.");
        } finally {
            // Always restore UI state, even if html2canvas throws synchronously
            // or an early-return path above is taken.
            doCleanup();
            if (opts.showBadge) document.body.classList.remove("show-you-badge");
            buttonEl.disabled = false;
        }
    }

    async function takeScreenshot() {
        if (typeof html2canvas === "undefined") { showToast("Screenshot library not loaded yet."); return; }
        captureToClipboard(els.teamsLayout, els.screenshotButton, {
            overlayStyle: "body::before { display: none !important; }.player-button { background: rgba(10, 14, 24, 0.92) !important; }.player-button::after { display: none !important; }.player-button.self-card,.player-button.is-blue,.player-button.is-red { background: rgba(10, 14, 24, 0.92) !important; box-shadow: 0 18px 40px rgba(0,0,0,0.5) !important; }.player-button:hover,.player-button:focus-visible,.player-button.is-selected { transform: none !important; }* { animation: none !important; }",
            styleId: "tmp-scr",
            hideDetails: true,
            showBadge: true,
            styleToast: true
        });
    }

    async function takeDetailScreenshot() {
        if (typeof html2canvas === "undefined") { showToast("Screenshot library not loaded yet."); return; }
        let target = els.detailsPanel;
        if (!target || target.hidden) { showToast("No player loadout open."); return; }
        captureToClipboard(target, els.screenshotPlayerBtn, {
            overlayStyle: "* { animation: none !important; }",
            styleId: "tmp-scr-det"
        });
    }
    function setStatus(text, cls) {
        els.statusPill.className = "pill status-pill " + cls;
        els.statusText.textContent = text;
        updateLoadingOverlay(cls);
    }

    function resetState() {
        // Preserve the current session epoch across resetState so the stale-event
        // guard remains active; epoch is only set by the backend_ready/cache_cleared
        // handler. Clear lastGameState/prevGameState: a late heartbeat from the old
        // session can no longer seed these because the epoch guard is still active.
        setState({ payload: null, players: [], lastRenderKey: null, selectedPuuid: null, matchPuuid: undefined, lastGameState: null, prevGameState: null });
        markAllDirty();
        render();
    }

    // ---- Tauri IPC setup ----
    function setupTauriListeners() {
        tauriListen("heartbeat", function (event) {
            try {
                setStatus("Connected", "live");
                // Drop heartbeats from a previous backend session. After a restart the
                // new session mints a fresh sessionId; a late heartbeat from the old
                // session (still in flight) must not re-render stale data over the
                // freshly-initialized UI.
                let evSession = event.payload && typeof event.payload.sessionId === "number" ? event.payload.sessionId : null;
                // When an epoch is active we drop any event that lacks a sessionId or
                // carries a mismatched one — including events from a previous backend
                // session still in flight. The old guard also required evSession !== null
                // up front, which let any event without a numeric sessionId bypass the
                // drop entirely and repaint stale data.
                if (state.epoch !== null && evSession !== state.epoch) return;
                if (event.payload && event.payload.players) {
                    // Snapshot the previous *genuinely different* game state so a later
                    // `state_change` event can compute the from->to transition
                    // independently of delivery order (see renderStateTransition). Only
                    // capture it when the state actually changes: during a PREGAME->
                    // INGAME handoff the backend can emit several consecutive fallback
                    // heartbeats with state "INGAME" before the real state_change fires
                    // (Riot moves the match server-side while WS/poll still reports
                    // PREGAME). Updating prevGameState on every heartbeat would let the
                    // second fallback tick overwrite it with "INGAME" and re-trigger the
                    // UI wipe. Keeping the last *different* state closes the race for any
                    // number of repeated same-state heartbeats in between.
                    if (event.payload.state !== state.lastGameState) {
                        setState({ prevGameState: state.lastGameState });
                    }
                    setState({ lastGameState: event.payload.state });
                    setPayload(event.payload);
                }
            } catch (e) { console.error("[VRY] heartbeat handler error:", e); }
        });

        // Rank icons are emitted once at startup and cached
        // here; they are no longer carried on every heartbeat payload.
        tauriListen("rank_icons", function (event) {
            try {
                setState({ rankIcons: event.payload || null });
                state.dirty.players = true;
                state.dirty.details = true;
                render();
            } catch (e) { console.error("[VRY] rank_icons handler error:", e); }
        });

        tauriListen("state_change", function (event) {
            try {
                // Drop transitions from a previous backend session (see heartbeat guard).
                let evSession = event.payload && typeof event.payload.sessionId === "number" ? event.payload.sessionId : null;
                if (state.epoch !== null && evSession !== state.epoch) return;
                if (event.payload && event.payload.state) {
                    renderStateTransition(event.payload.state);
                }
            } catch (e) { console.error("[VRY] state_change handler error:", e); }
        });

        tauriListen("backend_ready", function (event) {
            try {
                setStatus("Connected", "live");
                // Mint a new epoch for this backend session. Any heartbeats/state_change
                // carrying an older sessionId are dropped by their guards above, which
                // prevents stale post-restart events from desyncing the freshly cleared UI.
                let newEpoch = event.payload && typeof event.payload.sessionId === "number" ? event.payload.sessionId : null;
                setState({ epoch: newEpoch });
            } catch (e) { console.error("[VRY] backend_ready handler error:", e); }
        });

        tauriListen("riot_client_launching", function () {
            try { setStatus("Launching Riot Client…", "loading"); } catch (e) { console.error("[VRY] riot_client_launching handler error:", e); }
        });

        tauriListen("riot_client_waiting", function () {
            try { setStatus("Waiting for Riot Client…", "loading"); } catch (e) { console.error("[VRY] riot_client_waiting handler error:", e); }
        });

        tauriListen("cache_cleared", function (event) {
            try {
                // Re-arm the epoch from the payload BEFORE clearing UI state so the
                // stale-event guard is active. resetState() preserves the existing epoch
                // (it only clears via the backend_ready/cache_cleared handler), so a late
                // heartbeat from the previous session is dropped instead of repainting
                // stale data over the cleared UI.
                let newEpoch = event.payload && typeof event.payload.sessionId === "number" ? event.payload.sessionId : null;
                if (newEpoch !== null) setState({ epoch: newEpoch });
                resetState();
            } catch (e) { console.error("[VRY] cache_cleared handler error:", e); }
        });

        tauriListen("log_update", function (event) {
            try {
                let line = event.payload || "";
                logLines.push(line);
                if (logLines.length > 200) logLines.splice(0, logLines.length - 200);
                if (els.loadingLogTail) {
                    els.loadingLogTail.textContent = logLines.join("\n");
                    els.loadingLogTail.scrollTop = els.loadingLogTail.scrollHeight;
                }
                if (els.logPanel && els.logPanel.open) {
                    renderLogTail();
                }
            } catch (e) { console.error("[VRY] log_update handler error:", e); }
        });

        tauriListen("auth_error", function (event) {
            try {
                let message = (event.payload && event.payload.message) || "Please sign in to Riot Client and click Refresh.";
                showAuthErrorModal(message);
            } catch (e) { console.error("[VRY] auth_error handler error:", e); }
        });
    }

    // ---- loading overlay ----
    function updateLoadingOverlay(cls) {
        els.loadingOverlay.hidden = cls === "live";
    }

    // ---- state / rendering ----
    function bumpTimestampOnly(payload) {
        if (!payload || !payload.time) return;
        // Re-query the chip live: renderMeta() rebuilds matchMeta via
        // replaceChildren() on every meta render, so the cached els.metaUpdatedChip
        // reference goes stale (detached) and a no-op version tick would silently
        // fail to refresh the "Updated HH:MM:SS" label.
        let chip = document.getElementById("metaUpdatedChip");
        if (chip) chip.textContent = "Updated " + new Date(payload.time * 1000).toLocaleTimeString();
    }

    function setPayload(payload) {
        try {
            let version = payload && payload.version;
            let unchanged = version !== undefined && version === state.lastRenderKey;
            let newMatchPuuid = payload && (payload.matchId || null);
            setState({ payload: payload });
            if (unchanged) { bumpTimestampOnly(payload); return; }
            setState({ lastRenderKey: version });
            setState({ players: normalizePlayers(payload) });
            // Only drop the current selection when the match identity actually
            // changes (keyed off payload.matchId, the real per-match id — NOT the
            // account puuid, which is constant). A player can be absent from a single
            // heartbeat (e.g. during the PREGAME->INGAME handoff before loadouts are
            // fetched) without it being a new match, so clearing on transient absence
            // would flicker the details panel closed and never auto-restore it.
            if (state.selectedPuuid && state.matchPuuid !== undefined && newMatchPuuid !== state.matchPuuid) { setState({ selectedPuuid: null }); }
            setState({ matchPuuid: newMatchPuuid });
            markAllDirty();
            render();
        } catch (e) {
            console.error("[VRY] setPayload error:", e);
        }
    }

    function normalizePlayers(payload) {
        let rawPlayers = (payload && payload.players) || {};
        let myPuuid = payload && payload.puuid;
        let players = [];
        for (let puuid in rawPlayers) {
            if (!Object.prototype.hasOwnProperty.call(rawPlayers, puuid)) continue;
            let p = rawPlayers[puuid] || {};
            p.puuid = p.puuid || puuid;
            p.isSelf = !!(myPuuid && p.puuid === myPuuid);
            p._weaponMap = {};
            let w = p.weapons || {};
            for (let key in w) {
                if (!Object.prototype.hasOwnProperty.call(w, key)) continue;
                let entry = w[key];
                if (entry && entry.weapon) p._weaponMap[entry.weapon] = entry;
            }
            players.push(p);
        }
        return players.filter(function (p) { return p.name || p.agent || p.weapons; }).sort(function (a, b) {
            // Self first
            if (a.isSelf) return -1;
            if (b.isSelf) return 1;
            // Party members next (non-zero partyNumber), grouped together
            let pa = Number(a.partyNumber) || 0;
            let pb = Number(b.partyNumber) || 0;
            if (pa !== pb) return pb - pa;
            // Team ordering (Blue before Red)
            let t = teamRank(a.team) - teamRank(b.team);
            if (t !== 0) return t;
            // By rank descending (higher rank first)
            let ra = Number(a.rank) || 0;
            let rb = Number(b.rank) || 0;
            if (ra !== rb) return rb - ra;
            // Alphabetically by name as final tiebreaker
            return String(a.name || "").localeCompare(String(b.name || ""));
        });
    }

    function teamRank(team) { if (team === "Blue") return 0; if (team === "Red") return 1; return 2; }
    function teamClass(team) { if (team === "Blue") return "is-blue"; if (team === "Red") return "is-red"; return ""; }

    function markAllDirty() {
        state.dirty.meta = true;
        state.dirty.players = true;
        state.dirty.playedWith = true;
        state.dirty.details = true;
        state.dirty.json = true;
    }

    // Coalesce the heavy grid rendering (renderMeta + renderPlayers) into a
    // single requestAnimationFrame so the browser batches layout/paint once
    // per frame instead of forcing a synchronous reflow on every
    // replaceChildren()/append() pair. Rapid successive renders (e.g. a
    // heartbeat immediately followed by a selection) collapse into one paint.
    let gridRenderScheduled = false;
    function scheduleGridRender() {
        if (gridRenderScheduled) return;
        gridRenderScheduled = true;
        requestAnimationFrame(function () {
            gridRenderScheduled = false;
            if (state.dirty.meta) { renderMeta(); state.dirty.meta = false; }
            if (state.dirty.players) { renderPlayers(); state.dirty.players = false; }
        });
    }

    function render() {
        scheduleGridRender();
        renderTopLinks();
        if (state.dirty.playedWith) { renderPlayedWith(); state.dirty.playedWith = false; }
        if (state.dirty.details) { renderDetails(); state.dirty.details = false; }
        if (state.dirty.json) { renderJson(); state.dirty.json = false; }
    }

    function renderTopLinks() {
        let self = state.players.filter(function (p) { return p.isSelf; })[0];
        let hasName = self && self.name && self.name.indexOf("#") !== -1;
        [els.topTrnLink, els.topVtlLink].forEach(function (link) {
            if (!link) return;
            if (!hasName) {
                link.href = "#";
                link.removeAttribute("title");
                link.disabled = true;
                return;
            }
            let href = link === els.topTrnLink
                ? "https://tracker.gg/valorant/profile/riot/" + encodeURIComponent(self.name) + "/overview"
                : "https://vtl.lol/id/" + encodeURIComponent(self.name.replace("#", "_"));
            link.href = href;
            link.title = href + COPY_HINT;
            link.disabled = false;
        });
    }

    // ---- targeted rendering for click interactions ----
    function selectPlayer(puuid) {
        setState({ selectedPuuid: puuid });
        updateSelection();
        state.dirty.details = true;
        render();
    }

    function deselectPlayer() {
        setState({ selectedPuuid: null });
        state.dirty.details = true;
        render();
        updateSelection();
    }

    function updateSelection() {
        // Use the cached button map built during renderPlayers() instead of
        // re-querying the DOM (.player-button) on every selection change.
        state.playerButtons.forEach(function (btn) {
            btn.classList.toggle("is-selected", btn.dataset.puuid === state.selectedPuuid);
        });
    }

    // ---- event delegation ----
    function handlePlayerGridClick(e) {
        try {
            let button = e.target.closest(".player-button");
            if (!button) return;
            selectPlayer(button.dataset.puuid);
        } catch (err) {
            console.error("[VRY] handlePlayerGridClick error:", err);
        }
    }

    // ---- event delegation ----
    // A single document-level contextmenu listener replaces the per-element
    // bindContextCopy() listeners (and the grid-level handler) that were
    // attached to ~40 freshly-created elements on every render. Those elements
    // are destroyed on each heartbeat, so the old listeners churned GC. See
    // so the old listeners churned GC.
    function handleDelegatedContextMenu(e) {
        try {
            // Always suppress the native webview context menu (reload, save image as,
            // print, back/forward, …) everywhere. Only the in-app copy action below
            // remains; right-clicks on non-copyable elements now do nothing.
            e.preventDefault();
            if (e.stopPropagation) e.stopPropagation();
            let node = e.target;
            if (node && node.nodeType === 3) node = node.parentNode; // text node
            let el = null;
            while (node && node !== document && node.nodeType === 1) {
                if (node.title && node.title.indexOf(COPY_HINT) !== -1) { el = node; break; }
                node = node.parentNode;
            }
            if (!el) return;
            let text = stripHint(el.title || el.textContent);
            copyToClipboard(text).then(function () { showToast("Copied: " + text); }).catch(function (e) { console.error("[VRY] Context menu copy failed:", e); showToast("Failed to copy."); });
        } catch (err) {
            console.error("[VRY] handleDelegatedContextMenu error:", err);
        }
    }
    document.addEventListener("contextmenu", handleDelegatedContextMenu);

    // Avatar <img> error fallback, delegated in the capture phase (error events
    // do not bubble) so we no longer attach an onerror listener per avatar image.
    // Avatar <img> error fallback, delegated in the capture phase (error events
    // do not bubble) so we no longer attach an onerror listener per avatar image.
    function handleGridImageError(e) {
        try {
            if (e.target && e.target.tagName === "IMG" && e.target.classList && e.target.classList.contains("agent-avatar")) {
                let ph = makeAvatarPlaceholder();
                if (e.target.parentNode) e.target.parentNode.replaceChild(ph, e.target);
            }
        } catch (err) {
            console.error("[VRY] handleGridImageError:", err);
        }
    }

    let STATE_LABELS = { INGAME: "In-Game", PREGAME: "Agent Select", MENUS: "In-Menus", DISCONNECTED: "Disconnected" };
    let STATE_CLASSES = { INGAME: "state-ingame", PREGAME: "state-pregame", MENUS: "state-menus", DISCONNECTED: "state-disconnected" };

    function showLoadingChip(label) {
        els.matchMeta.replaceChildren();
        let chip = document.createElement("span");
        chip.className = "meta-chip";
        let spinner = document.createElement("span");
        spinner.className = "meta-spinner";
        chip.append(spinner, document.createTextNode("Loading " + label + " Data\u2026"));
        els.matchMeta.append(chip);
    }

    function renderStateTransition(newState) {
        // Decide whether to keep the current player UI or wipe it based on the
        // actual from->to transition. We use `prevGameState` (the state of the
        // last heartbeat we rendered) rather than `lastGameState`, because the
        // backend's `state_change` event can be delivered AFTER the first INGAME
        // heartbeat during the pregame->ingame handoff (e.g. when the pregame
        // endpoint 404s and the backend falls back to an INGAME context lookup,
        // it emits the INGAME heartbeat before the state_change event). In that
        // race, `lastGameState` is already "INGAME" when state_change arrives, so
        // comparing `lastGameState` would wrongly treat it as a non-handoff
        // transition and wipe the player grid — losing all loaded data. Using
        // `prevGameState` keeps the UI intact across pregame->ingame.
        let isPregameToIngame = (state.prevGameState === "PREGAME" && newState === "INGAME");
        let label = STATE_LABELS[newState] || newState || "Unknown";
        // Keep the existing player UI for any transition that lands on the same
        // already-rendered state. This covers the PREGAME->INGAME handoff AND a
        // reconnect/refresh while already INGAME. In both cases the backend's
        // single INGAME heartbeat may already have rendered the top-left meta
        // chips (In-Game • mode • map • server • time) before this state_change
        // is delivered. Only show a loading chip if we have no data yet; if the
        // chips are already on screen, leave them untouched — otherwise a late
        // state_change would wipe the rendered chips and nothing would repaint
        // them (INGAME steady-state suppresses further heartbeats).
        let keepUi = isPregameToIngame || (newState === state.lastGameState && state.payload);
        if (keepUi) {
            // Ensure the next heartbeat triggers a re-render.
            setState({ lastRenderKey: null });
            if (!state.payload) showLoadingChip(label);
            return;
        }
        // Original clearing behavior for all other transitions
        setState({ payload: null, lastRenderKey: null, players: [] });
        showLoadingChip(label);
        // Clear player grids and show loading state across full width
        els.blueGrid.replaceChildren();
        els.redGrid.replaceChildren();
        state.playerButtons = new Map();
        els.defHeader.textContent = label;
        els.defSection.hidden = false;
        els.atkSection.hidden = true;
        els.teamDivider.hidden = true;
        els.teamsLayout.classList.add("is-unified");
        let loadMsg = document.createElement("div");
        loadMsg.className = "loading-grid-message";
        let loadSpinner = document.createElement("span");
        loadSpinner.className = "meta-spinner";
        loadMsg.append(loadSpinner, document.createTextNode(" Waiting for " + label + " data\u2026"));
        els.blueGrid.append(loadMsg);
    }

    function renderMeta() {
        let p = state.payload;
        els.matchMeta.replaceChildren();
        if (!p) {
            let w = document.createElement("span");
            w.className = "meta-chip updated";
            let s = document.createElement("span");
            s.className = "meta-spinner";
            w.append(s, document.createTextNode("Waiting for data\u2026"));
            els.matchMeta.append(w);
            return;
        }
        let segments = [];
        segments.push({ text: STATE_LABELS[p.state] || txt(p.state, "Unknown"), cls: STATE_CLASSES[p.state] || "" });
        if (p.mode) segments.push({ text: p.mode, cls: "mode" });
        let mapVal = p.map;
        if (Array.isArray(mapVal)) mapVal = mapVal[0];
        if (mapVal) segments.push({ text: mapVal, cls: "map" });
        if (p.server) segments.push({ text: p.server, cls: "server" });
        if (p.time) { let d = new Date(p.time * 1000); segments.push({ text: "Updated " + d.toLocaleTimeString(), cls: "updated", id: "metaUpdatedChip" }); }
        segments.forEach(function (seg, i) {
            if (i > 0) { let sep = document.createElement("span"); sep.className = "meta-sep"; sep.textContent = "\u2022"; els.matchMeta.append(sep); }
            appendMetaChip(seg.text, seg.cls, seg.id);
        });
    }

    function appendMetaChip(text, cls, id) {
        let chip = document.createElement("span");
        chip.className = "meta-chip" + (cls ? " " + cls : "");
        if (id) {
            chip.id = id;
            if (id === "metaUpdatedChip") els.metaUpdatedChip = chip;
        }
        chip.textContent = text;
        els.matchMeta.append(chip);
    }

    function renderPlayers() {
        els.blueGrid.replaceChildren();
        els.redGrid.replaceChildren();
        state.playerButtons = new Map();
        renderTeamHeaders();
        if (state.players.length === 0) {
            let msg = document.createElement("div");
            msg.className = "loading-grid-message";
            msg.textContent = "Waiting for player data\u2026";
            els.blueGrid.append(msg);
            return;
        }
        let selfTeam = myTeam(state.payload);
        let blueFrag = document.createDocumentFragment();
        let redFrag = document.createDocumentFragment();
        state.players.forEach(function (player) {
            let button = document.createElement("button");
            button.type = "button";
            button.className = "player-button " + teamClass(player.team);
            button.classList.toggle("self-card", player.isSelf);
            button.classList.toggle("is-selected", player.puuid === state.selectedPuuid);
            button.title = txt(player.name, "Unknown Player") + COPY_HINT;
            button.dataset.puuid = player.puuid;
            state.playerButtons.set(player.puuid, button);
            let avatar = buildAgentAvatar(player.agentImgLink, player.agent, player.agentSelectionState);
            let identity = document.createElement("div");
            identity.className = "player-main";
            let name = document.createElement("span");
            name.className = "player-name";
            // Censor Self Player Name
            name.textContent = txt(player.isSelf ? "You" : player.name, "Unknown Player");
            let youBadge = null;
            if (player.isSelf) { youBadge = document.createElement("span"); youBadge.className = "self-badge"; youBadge.textContent = "You"; }
            let agent = document.createElement("span");
            agent.className = "agent-name";
            agent.textContent = txt(player.agent, "Agent " + NA);
            let metaRow = document.createElement("span");
            metaRow.className = "player-meta-row";
            let rankBadge = document.createElement("span");
            rankBadge.className = "player-meta";
            let rankIconUrl = state.rankIcons && state.rankIcons[player.rank];
            if (rankIconUrl) { let ri = document.createElement("img"); ri.className = "rank-icon"; ri.src = safeHttps(rankIconUrl); ri.loading = "lazy"; rankBadge.append(ri); rankBadge.append(" "); }
            rankBadge.append(document.createTextNode(rankName(player.rank, true)));
            let bc = rankColor(player.rank);
            if (bc) rankBadge.style.color = bc;
            metaRow.append(rankBadge);
            let action = document.createElement("span");
            action.className = "player-action";
            action.textContent = "View loadout & stats";
            identity.append(name);
            if (youBadge) identity.append(youBadge);
            identity.append(agent, metaRow, action);
            button.append(avatar, identity, buildCardStats(player), buildPreviewRow(player));
            let isSelfTeam = !selfTeam || player.team === selfTeam;
            (isSelfTeam ? blueFrag : redFrag).append(button);
        });
        els.blueGrid.append(blueFrag);
        els.redGrid.append(redFrag);
    }

    function myTeam(payload) {
        if (!payload || !payload.players) return null;
        for (let k in payload.players) { if (payload.players[k].isSelf) return payload.players[k].team; }
        return null;
    }

    function teamSide(team) {
        if (team === "Blue") return "DEF";
        if (team === "Red") return "ATK";
        return "";
    }

    function renderTeamHeaders() {
        let payload = state.payload;
        if (!payload || payload.state === "MENUS" || payload.state === "DISCONNECTED") {
            els.defHeader.textContent = "PARTY";
            els.defHeader.classList.toggle("is-my-team", true);
            els.defSection.hidden = false;
            els.atkSection.hidden = true;
            els.teamDivider.hidden = true;
        els.teamsLayout.classList.add("is-unified");
            return;
        }
        let selfTeam = myTeam(payload);
        let hasBlue = state.players.some(function (p) { return p.team === "Blue"; });
        let hasRed = state.players.some(function (p) { return p.team === "Red"; });
        let singleTeam = (hasBlue && !hasRed) || (!hasBlue && hasRed);
        els.teamsLayout.classList.toggle("is-unified", singleTeam);
        if (singleTeam) {
            els.teamDivider.hidden = true;
            let onlyTeam = hasBlue ? "Blue" : "Red";
            let isMyTeam = selfTeam === onlyTeam;
            els.atkSection.hidden = true;
            els.defSection.hidden = false;
            els.defHeader.textContent = (isMyTeam ? "ALLY" : "ENEMY") + " (" + teamSide(onlyTeam) + ")";
            els.defHeader.classList.toggle("is-my-team", isMyTeam);
        } else {
            els.defSection.hidden = false; els.atkSection.hidden = false; els.teamDivider.hidden = false;
            // LEFT side (defSection/blueGrid) = self's team, RIGHT side (atkSection/redGrid) = other team
            let leftTeam = selfTeam || "Blue";
            let rightTeam = (leftTeam === "Blue") ? "Red" : "Blue";
            els.defHeader.textContent = (leftTeam === selfTeam ? "ALLY" : "ENEMY") + " (" + teamSide(leftTeam) + ")";
            els.atkHeader.textContent = (rightTeam === selfTeam ? "ALLY" : "ENEMY") + " (" + teamSide(rightTeam) + ")";
            els.defHeader.classList.toggle("is-my-team", leftTeam === selfTeam);
            els.atkHeader.classList.toggle("is-my-team", rightTeam === selfTeam);
        }
    }

    function buildPreviewRow(player) {
        let row = document.createElement("div");
        row.className = "preview-row";
        PREVIEW_WEAPONS.forEach(function (weaponName) {
            let weapon = getWeapon(player, weaponName);
            let slot = document.createElement("span");
            slot.className = "preview-slot";
            if (weapon && weapon.skinDisplayIcon) { let img = document.createElement("img"); img.src = safeHttps(weapon.skinDisplayIcon); img.alt = weapon.skinDisplayName || weapon.weapon || "Weapon"; slot.append(img); }
            let copy = document.createElement("span");
            copy.className = "preview-copy";
            let label = document.createElement("span");
            label.className = "preview-label";
            label.textContent = weaponName;
            let name = document.createElement("span");
            name.className = "preview-name";
            let skinLabel = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
            if (weapon && weapon.skinDisplayName && weaponName !== "Melee") { skinLabel = stripWeaponName(skinLabel, weaponName); }
            name.textContent = skinLabel;
            slot.title = skinLabel + COPY_HINT;
            copy.append(label, name);
            slot.append(copy);
            row.append(slot);
        });
        return row;
    }

    function renderDetails() {
        let selected = state.players.find(function (p) { return p.puuid === state.selectedPuuid; });
        let hasSelection = Boolean(selected);
        els.detailsPanel.hidden = !hasSelection;
        els.emptyState.hidden = hasSelection || state.players.length > 0;
        if (!hasSelection && state.players.length === 0) { els.emptyStateTitle.textContent = "No match data"; els.emptyStateText.textContent = "Waiting for VRY backend data."; }
        else if (!hasSelection) { els.emptyStateTitle.textContent = "No player selected"; els.emptyStateText.textContent = "Click a player card to view loadout and stats."; }
        if (!selected) return;
        let agentNotSelected = selected.agentSelectionState === "";
        els.selectedAgent.src = safeHttps(selected.agentImgLink);
        els.selectedAgent.hidden = !selected.agentImgLink || agentNotSelected;
        els.selectedAgent.alt = selected.agent || "";
        els.selectedAgent.classList.toggle("is-selecting", selected.agentSelectionState === "selected");
        // Censor Self Player Name
        els.selectedName.textContent = txt(selected.isSelf ? "You" : selected.name, "Unknown Player");
        els.selectedName.title = txt(selected.name, "Unknown Player") + COPY_HINT;
        let hasName = selected.name && selected.name.indexOf("#") !== -1;
        if (hasName) {
            let trnHref = "https://tracker.gg/valorant/profile/riot/" + encodeURIComponent(selected.name) + "/overview";
            let vtlHref = "https://vtl.lol/id/" + encodeURIComponent(selected.name.replace("#", "_"));
            els.trnLink.href = trnHref; els.trnLink.title = trnHref + COPY_HINT; els.trnLink.rel = "noopener noreferrer"; els.trnLink.hidden = false;
            els.vtlLink.href = vtlHref; els.vtlLink.title = vtlHref + COPY_HINT; els.vtlLink.rel = "noopener noreferrer"; els.vtlLink.hidden = false;
        } else { els.trnLink.hidden = true; els.vtlLink.hidden = true; }
        els.copyStatsBtn.hidden = false;
        els.screenshotPlayerRow.hidden = false;
        els.selectedCardTitle.textContent = selected.title || "";
        els.selectedCardTitle.hidden = !selected.title;
        els.selectedCardTitle.title = (selected.title || "Title") + COPY_HINT;
        els.selectedModalName.textContent = txt(selected.agent, "Agent " + NA);
        els.selectedLevel.textContent = isEmpty(selected.level) ? "Level " + NA : ("Level " + selected.level);
        els.selectedTeam.textContent = selected.team ? ((myTeam(state.payload) === selected.team) ? "ALLY" : "ENEMY") : "Unknown";
        els.selectedTeam.className = "team-pill " + teamClass(selected.team);
        if (selected.playerCard) {
            els.playerCardPreview.style.backgroundImage = "url(\"" + safeHttps(selected.playerCard) + "\")";
            els.playerCardPreview.title = (selected.playerCardName || "Player Card") + COPY_HINT;
        } else {
            els.playerCardPreview.style.backgroundImage = "";
            els.playerCardPreview.title = "Player Card";
        }
        renderStatBar(selected);
        renderExpressions(selected);
        renderWeapons(selected);

    }

    function renderStatBar(player) {
        els.statBar.replaceChildren();
        els.statBar.append(buildStatChips(player, "stat-chip", false, true));
    }

    function renderExpressions(player) {
        els.expressionGrid.replaceChildren();
        let expressions = [];
        let sprayKeys = player.sprays || {};
        for (let idx in sprayKeys) {
            if (!Object.prototype.hasOwnProperty.call(sprayKeys, idx)) continue;
            let e = sprayKeys[idx] || {};
            expressions.push(Object.assign({ index: Number(idx) }, e));
        }
        expressions.sort(function (a, b) { return a.index - b.index; });
        let slots = expressions.slice(0, 4);
        while (slots.length < 4) slots.push(null);
        let frag = document.createDocumentFragment();
        slots.forEach(function (expression, idx) {
            let tile = document.createElement("div");
            tile.className = "expression-tile expression-slot-" + idx;
            tile.title = (expression ? (expression.displayName || "Expression") : ("Empty slot " + (idx + 1))) + COPY_HINT;
            tile.setAttribute("aria-label", tile.title);
            if (expression && expression.type === "flex") tile.classList.add("is-flex");
            let art = document.createElement("div");
            art.className = "expression-art";
            let iconSrc = expression && (expression.fullTransparentIcon || expression.displayIcon);
            if (iconSrc) { let img = document.createElement("img"); img.src = safeHttps(iconSrc); img.alt = (expression && expression.displayName) || "Expression"; art.append(img); }
            let copy = document.createElement("div");
            copy.className = "expression-copy";
            let name = document.createElement("strong");
            name.textContent = expression ? (expression.displayName || NA) : ("Slot " + (idx + 1));
            let type = document.createElement("span");
            type.className = "expression-type";
            type.textContent = expression ? (expression.type || "expression") : "empty";
            copy.append(name, type);
            tile.append(art, copy);
            frag.append(tile);
        });
        els.expressionGrid.append(frag);
    }

    function renderWeapons(player) {
        els.weaponGroups.replaceChildren();
        let frag = document.createDocumentFragment();
        WEAPON_COLUMNS.forEach(function (column) {
            let columnNode = document.createElement("div");
            columnNode.className = "weapon-column " + column.className;
            let colFrag = document.createDocumentFragment();
            column.groups.forEach(function (group) {
                let section = document.createElement("section");
                section.className = "weapon-group weapon-group-" + group.slug;
                let heading = document.createElement("h3");
                heading.textContent = group.title;
                let grid = document.createElement("div");
                grid.className = "weapon-grid";
                let gridFrag = document.createDocumentFragment();
                group.weapons.forEach(function (weaponName) { gridFrag.append(buildWeaponTile(player, weaponName)); });
                grid.append(gridFrag);
                section.append(heading, grid);
                colFrag.append(section);
            });
            columnNode.append(colFrag);
            frag.append(columnNode);
        });
        els.weaponGroups.append(frag);
    }

    function buildWeaponTile(player, weaponName) {
        let weapon = getWeapon(player, weaponName);
        let tile = document.createElement("div");
        tile.className = "weapon-tile";
        tile.classList.toggle("is-empty", !weapon);
        let vc = chromaColor(weapon && weapon.chromaDisplayName);
        vc = vc ? " (" + vc + ")" : "";
        tile.title = (weapon ? (weaponName + ": " + (weapon.skinDisplayName || weapon.weapon || "Unknown skin") + vc) : (weaponName + ": " + NA)) + COPY_HINT;
        if (weapon && weapon.contentTierColor) {
            tile.style.setProperty("--tier-color", weapon.contentTierColor);
            tile.style.borderColor = weapon.contentTierColor;
        }
        if (weapon && weapon.contentTierName && weapon.contentTierIcon) {
            let badge = document.createElement("img");
            badge.className = "weapon-tier-badge";
            badge.src = safeHttps(weapon.contentTierIcon);
            badge.alt = weapon.contentTierName;
            badge.title = weapon.contentTierName;
            badge.loading = "lazy";
            tile.append(badge);
        }
        let art = document.createElement("div");
        art.className = "weapon-art";
        let iconSrc = weapon && (weapon.skinDisplayIcon || weapon.weaponDisplayIcon);
        if (iconSrc) { let img = document.createElement("img"); img.src = safeHttps(iconSrc); img.alt = weapon.skinDisplayName || weapon.weapon || weaponName; art.append(img); }
        let copy = document.createElement("div");
        copy.className = "weapon-copy";
        let label = document.createElement("span");
        label.className = "weapon-name";
        label.textContent = weaponName;
        let name = document.createElement("strong");
        name.textContent = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
        name.title = name.textContent + vc + COPY_HINT;
        copy.append(label, name);
        tile.append(art, copy);
        if (weapon && weapon.buddy_displayIcon) {
            tile.classList.add("has-buddy");
            let buddy = document.createElement("img");
            buddy.className = "buddy";
            buddy.src = safeHttps(weapon.buddy_displayIcon);
            buddy.alt = weapon.buddy_displayName || "Buddy";
            buddy.title = (weapon.buddy_displayName || "Buddy") + COPY_HINT;
            tile.append(buddy);
        }
        return tile;
    }

    function getWeapon(player, weaponName) {
        return (player._weaponMap || {})[weaponName] || null;
    }

    function buildAgentAvatar(src, alt, selectionState) {
        if (!src || selectionState === "") return makeAvatarPlaceholder();
        let img = document.createElement("img");
        img.className = "agent-avatar";
        if (selectionState === "selected") img.classList.add("is-selecting");
        img.alt = alt || "";
        img.src = safeHttps(src);
        return img;
    }

    function makeAvatarPlaceholder() {
        let el = document.createElement("div");
        el.className = "agent-avatar agent-avatar-placeholder";
        el.textContent = "?";
        el.setAttribute("aria-label", "Unknown agent");
        return el;
    }

    function capitalize(str) { return str ? str.charAt(0).toUpperCase() + str.slice(1) : ""; }

    function renderPlayedWith() {
        // Skip the (relatively expensive) player-map scan on every render when
        // the played-with section is collapsed - it's only useful when visible
        //(mirrors the early-return guard in renderJson()).
        if (els.playedWithPanel && !els.playedWithPanel.open) return;
        let payload = state.payload;
        let entries = (payload && Array.isArray(payload.alreadyPlayedWith)) ? payload.alreadyPlayedWith : [];

        // Only show players currently in this lobby/game
        let currentPlayerNames = {};
        if (payload && payload.players) {
            let myPuuid = payload.puuid;
            let pl = payload.players;
            for (let puuid in pl) {
                if (!Object.prototype.hasOwnProperty.call(pl, puuid)) continue;
                if (puuid !== myPuuid && pl[puuid].name) {
                    currentPlayerNames[pl[puuid].name] = true;
                }
            }
        }
        entries = entries.filter(function (entry) {
            return currentPlayerNames[entry.name] === true;
        });

        els.playedWithEmpty.hidden = entries.length > 0;
        els.playedWithTable.hidden = entries.length === 0;
        if (!entries.length) return;
        els.playedWithBody.replaceChildren();
        let frag = document.createDocumentFragment();
        entries.forEach(function (entry) {
            let row = document.createElement("tr");
            let nc = document.createElement("td"); nc.textContent = txt(entry.name, "Unknown"); row.append(nc);
            let rel = entry.relation === "ally" ? "Ally" : "Enemy";
            let cc = document.createElement("td"); cc.textContent = rel + " " + txt(entry.agent, "Unknown"); row.append(cc);
            let tc = document.createElement("td"); tc.textContent = formatEncounterTimes(entry); row.append(tc);
            let lastAgent = txt(entry.lastAgent || entry.agent, "Unknown");
            let lastMap = txt(entry.lastMap || entry.map, "Unknown");
            let lc = document.createElement("td"); lc.textContent = capitalize(txt(entry.relation_name, "player")) + " " + lastAgent + " on " + lastMap + " \u2014 " + formatTimeAgo(entry.time_diff) + " ago"; row.append(lc);
            let rc = document.createElement("td"); rc.textContent = formatEncounterRecord(entry); row.append(rc);
            frag.append(row);
        });
        els.playedWithBody.append(frag);
    }

    function renderJson() {
        // Skip the (relatively expensive) JSON.stringify entirely when the
        // debug panel is closed - it's only useful to the user when visible.
        if (els.hbPanel && !els.hbPanel.open) return;
        let text = state.payload ? JSON.stringify(state.payload, null, 2) : "No data yet.";
        if (els.jsonPre.textContent !== text) {
            els.jsonPre.textContent = text;
        }
    }

    // Single clipboard-write primitive used everywhere. Resolves once `text`
    // is on the clipboard (via the async Clipboard API or the execCommand
    // fallback) so callers only need a success handler. Centralises the
    // previously-inconsistent clipboard error handling.
    function copyToClipboard(text) {
        if (navigator.clipboard && navigator.clipboard.writeText) {
            return navigator.clipboard.writeText(text).catch(function (e) {
                console.error("[VRY] Clipboard API write failed:", e);
                return new Promise(function (resolve) { fallbackCopy(text, resolve); });
            });
        }
        return new Promise(function (resolve) { fallbackCopy(text, resolve); });
    }

    function copyText(text, button) {
        if (!text) { showToast("Nothing to copy yet."); return; }
        let done = function () {
            let original = button.textContent;
            button.textContent = "Copied!";
            button.classList.add("copied");
            if (button._copyT) clearTimeout(button._copyT);
            button._copyT = setTimeout(function () { button.textContent = original; button.classList.remove("copied"); }, 1500);
        };
        copyToClipboard(text).then(done).catch(function (e) { console.error("[VRY] copyText error:", e); showToast("Copy failed."); });
    }

    function copyJson(button) {
        try {
            let text = state.payload ? JSON.stringify(state.payload, null, 2) : "";
            copyText(text, button);
        } catch (e) {
            console.error("[VRY] copyJson error:", e);
            showToast("Failed to copy JSON.");
        }
    }

    function renderInvokeText(invokeMethod, preEl, fallbackText, errorText) {
        tauriInvoke(invokeMethod).then(function (text) {
            let displayText = text || fallbackText;
            if (preEl.textContent !== displayText) {
                preEl.textContent = displayText;
            }
        }).catch(function (e) {
            console.error("[VRY] IPC failed:", invokeMethod, e);
            if (preEl.textContent !== errorText) {
                preEl.textContent = errorText;
            }
        });
    }

function renderHeartbeatTail() { renderInvokeText("get_heartbeat_log", els.hbPre, "(no heartbeat data yet)", "Failed to fetch heartbeat log."); }

function showAuthErrorModal(message) {
    let modal = document.getElementById("authErrorModal");
    let messageEl = document.getElementById("authErrorMessage");
    if (!modal || !messageEl) return;
    messageEl.textContent = message;
    modal.hidden = false;
}

function renderLogTail() { renderInvokeText("get_gui_log_tail", els.logPre, "(empty log)", "Failed to fetch log."); }

    function fallbackCopy(text, done) {
        let ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.append(ta);
        ta.select();
        try { document.execCommand("copy"); done(); } catch (e) { console.error("[VRY] execCommand copy failed:", e); showToast("Copy failed."); }
        ta.remove();
    }

    // ---- refresh / restart ----
    function openModal() { els.confirmModal.hidden = false; }
    function closeModal() { els.confirmModal.hidden = true; }

    function setRefreshButtonsBusy(busy) {
        document.querySelectorAll(".refresh-btn").forEach(function (btn) {
            btn.disabled = busy;
            btn.classList.toggle("is-spinning", busy);
        });
    }

    function requestRestart() {
        closeModal();
        setRefreshButtonsBusy(true);
        setStatus("Reconnecting", "pending");
        tauriInvoke("clear_all_cache").then(function () {
            return tauriInvoke("restart_application");
        }).then(function () {
            showToast("Reconnecting to backend...");
        }).catch(function (e) {
            console.error("[VRY] restart_application IPC failed:", e);
            showToast("Connection error, retrying...");
        }).finally(function () {
            setTimeout(function () { setRefreshButtonsBusy(false); }, 4000);
        });
    }

    // ---- wiring ----
    // Event delegation for player grids
    els.blueGrid.addEventListener("click", handlePlayerGridClick);
    els.redGrid.addEventListener("click", handlePlayerGridClick);
    els.blueGrid.addEventListener("error", handleGridImageError, true);
    els.redGrid.addEventListener("error", handleGridImageError, true);

    els.closeDetailsButton.addEventListener("click", deselectPlayer);
    els.detailsPanel.addEventListener("click", function (e) { if (e.target === els.detailsPanel) { deselectPlayer(); } });
    window.addEventListener("keydown", function (e) {
        if (e.key === "Escape") { if (!els.confirmModal.hidden) { closeModal(); return; } if (state.selectedPuuid) { deselectPlayer(); } }
    });
    window.addEventListener("auxclick", function (e) {
        if (e.button === 3 && state.selectedPuuid) { e.preventDefault(); deselectPlayer(); }
    });
    els.refreshButton.addEventListener("click", openModal);
    els.loadingRefreshButton.addEventListener("click", openModal);
    els.modalCancel.addEventListener("click", closeModal);
    els.modalConfirm.addEventListener("click", requestRestart);
    els.confirmModal.addEventListener("click", function (e) { if (e.target === els.confirmModal) closeModal(); });
    els.jsonCopyBtn.addEventListener("click", function () { copyJson(els.jsonCopyBtn); });
        els.screenshotPlayerBtn.addEventListener("click", takeDetailScreenshot);
    els.copyStatsBtn.addEventListener("click", function () {
        try {
            let p = (state.payload && state.selectedPuuid) ? state.payload.players[state.selectedPuuid] : null;
            if (!p) { showToast("Nothing to copy yet."); return; }
            let peakStr = rankName(p.peakRank, false);
            let parts = [
                "Player Name: " + txt(p.name, "Unknown"),
                "Win Rate (Comp): " + winRateDisplay(p.winPercentage),
                "Rank: " + rankName(p.rank, false),
                "RR: " + txt(p.rr),
                "Leaderboard: " + (isEmpty(p.leaderboard) || Number(p.leaderboard) <= 0 ? NA : "#" + p.leaderboard),
                "Peak Rank: " + (peakStr !== NA && p.peakRankAct ? peakStr + String(p.peakRankAct).trim() : peakStr),
                "Last Act: " + rankName(p.previousRank, false),
                "Level: " + txt(p.level),
                "Last Active (Comp): " + txt(p.lastActive),
            ];
            copyToClipboard(parts.join(" | ")).then(function () { showToast("Copied: Stats"); }).catch(function (e) { console.error("[VRY] Stats copy failed:", e); showToast("Failed."); });
        } catch (e) {
            console.error("[VRY] copyStatsBtn error:", e);
            showToast("Failed to copy stats.");
        }
    });
    els.screenshotButton.addEventListener("click", takeScreenshot);

    // Open log file
    let logOpenBtn = document.getElementById("logOpenBtn");
    if (logOpenBtn) {
        logOpenBtn.addEventListener("click", function () {
            tauriInvoke("open_log_file").catch(function (e) { console.error("[VRY] open_log_file failed:", e); showToast("Failed to open log file"); });
        });
    }

    // Open heartbeat file
    let hbOpenBtn = document.getElementById("hbOpenBtn");
    if (hbOpenBtn) {
        hbOpenBtn.addEventListener("click", function () {
            tauriInvoke("open_heartbeat_file").catch(function (e) { console.error("[VRY] open_heartbeat_file failed:", e); showToast("Failed to open heartbeat file"); });
        });
    }

    // Auth error modal
    let authErrorRefreshBtn = document.getElementById("authErrorRefreshBtn");
    let authErrorModal = document.getElementById("authErrorModal");
    let authErrorMessage = document.getElementById("authErrorMessage");
    if (authErrorRefreshBtn && authErrorModal && authErrorMessage) {
        authErrorRefreshBtn.addEventListener("click", function () {
            authErrorModal.hidden = true;
            requestRestart();
        });
    }

    // Start-up view: Open logs buttons
    function openLogFile() {
        tauriInvoke("open_log_file").catch(function (e) { console.error("[VRY] open_log_file failed:", e); showToast("Failed to open log file"); });
    }
    let loadingLogBtn = document.getElementById("loadingLogBtn");
    if (loadingLogBtn) {
        loadingLogBtn.addEventListener("click", openLogFile);
    }
    let authErrorLogBtn = document.getElementById("authErrorLogBtn");
    if (authErrorLogBtn) {
        authErrorLogBtn.addEventListener("click", openLogFile);
    }

    // Open log file
    [
        { panel: els.logPanel, pre: els.logPre, copy: els.logCopyBtn, render: renderLogTail },
        { panel: els.hbPanel, pre: els.hbPre, copy: els.hbCopyBtn, refresh: els.hbRefreshBtn, render: renderHeartbeatTail },
    ].forEach(function (p) {
        p.panel.addEventListener("toggle", function () { if (p.panel.open) p.render(); });
        p.copy.addEventListener("click", function () { copyText(p.pre.textContent, p.copy); });
        if (p.refresh) p.refresh.addEventListener("click", p.render);
    });

    // renderJson() early-returns while els.hbPanel is closed, so refresh
    // immediately on open to avoid showing stale data. Matches the sibling
    // open-to-refresh pattern used by the panels above.
    if (els.hbPanel) {
        els.hbPanel.addEventListener("toggle", function () { if (els.hbPanel.open) renderJson(); });
    }

    // renderPlayedWith() early-returns while collapsed, so render on open to
    // avoid showing stale/empty content.
    if (els.playedWithPanel) {
        els.playedWithPanel.addEventListener("toggle", function () { if (els.playedWithPanel.open) renderPlayedWith(); });
    }

    // ---- boot ----
    setStatus("Starting", "pending");
    setupTauriListeners();
})();
