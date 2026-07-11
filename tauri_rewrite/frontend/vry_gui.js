// Tauri IPC bridge
const tauriInvoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || (() => Promise.reject(new Error('IPC unavailable')));
const tauriListen = window.__TAURI__?.event?.listen || window.__TAURI__?.listen || (() => Promise.resolve(() => {}));
const tauriEmit = window.__TAURI__?.event?.emit || window.__TAURI__?.emit || (() => Promise.resolve());

(function () {
    "use strict";

    var COPY_HINT = " (Right-click to copy)";
    var stripHint = function (t) { return t ? t.replace(COPY_HINT, "") : ""; };
    var chromaColor = function (name) { var m = name && name.match(/\(Variant \d+ (.+)\)$/); return m ? m[1] : ""; };

    var RANK_NAMES_FULL = [
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

    var RANK_NAMES_SHORT = [
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

    var RANK_COLORS = [
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

    var NA = "N/A";
    var PREVIEW_WEAPONS = ["Vandal", "Phantom", "Melee"];

    var state = {
        payload: null,
        players: [],
        selectedPuuid: null,
        lastRenderKey: null,
        rankIcons: null,
        lastGameState: null,
    };

    var els = {
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
        jsonPreEmpty: document.getElementById("jsonPreEmpty"),
        jsonCopyBtnEmpty: document.getElementById("jsonCopyBtnEmpty"),
        jsonPanelSummary: document.getElementById("jsonPanelSummary"),
        jsonPanelEmptySummary: document.getElementById("jsonPanelEmptySummary"),
        defHeader: document.getElementById("defHeader"),
        atkHeader: document.getElementById("atkHeader"),
        defSection: document.getElementById("defSection"),
        atkSection: document.getElementById("atkSection"),
        teamDivider: document.getElementById("teamDivider"),
        screenshotButton: document.getElementById("screenshotButton"),
        playedWithEmpty: document.getElementById("playedWithEmpty"),
        playedWithTable: document.getElementById("playedWithTable"),
        playedWithBody: document.getElementById("playedWithBody"),
        logPanel: document.getElementById("logPanel"),
        logPre: document.getElementById("logPre"),
        logCopyBtn: document.getElementById("logCopyBtn"),
        logRefreshBtn: document.getElementById("logRefreshBtn"),
        logInterval: null,
        hbPanel: document.getElementById("hbPanel"),
        hbPre: document.getElementById("hbPre"),
        hbCopyBtn: document.getElementById("hbCopyBtn"),
        hbRefreshBtn: document.getElementById("hbRefreshBtn"),
        hbInterval: null,
    };

    var WEAPON_COLUMNS = [
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

    function rankName(idx, short) {
        if (isEmpty(idx)) return NA;
        var n = Number(idx);
        var t = short ? RANK_NAMES_SHORT : RANK_NAMES_FULL;
        if (Number.isNaN(n) || n < 0 || n >= t.length) return NA;
        return t[n];
    }

    function rankColor(idx) {
        var n = Number(idx);
        if (isEmpty(idx) || Number.isNaN(n) || n < 0 || n >= RANK_COLORS.length) return null;
        var rgb = RANK_COLORS[n];
        return "rgb(" + rgb[0] + ", " + rgb[1] + ", " + rgb[2] + ")";
    }

    function winRateDisplay(v) {
        if (isEmpty(v)) return NA;
        var s = String(v);
        if (s.indexOf("%") !== -1) return s;
        var m = s.match(/^(-?[\d.]+)(\s*\(.*)$/);
        return m ? (m[1] + "%" + m[2]) : s;
    }

    function stripWeaponName(skinName, weaponName) {
        if (!skinName) return skinName;
        return skinName.replace(new RegExp("\\b" + weaponName + "\\b", "ig"), "").replace(/\s+/g, " ").trim() || skinName;
    }

    function formatTimeAgo(seconds) {
        var s = Math.max(0, Math.floor(Number(seconds) || 0));
        if (s < 60) return s + (s === 1 ? " second" : " seconds");
        if (s < 3600) { var m = Math.floor(s / 60); return m + (m === 1 ? " minute" : " minutes"); }
        if (s < 86400) { var h = Math.floor(s / 3600); return h + (h === 1 ? " hour" : " hours"); }
        var d = Math.floor(s / 86400);
        return d + (d === 1 ? " day" : " days");
    }

    function formatEncounterTimes(entry) {
        var parts = [];
        if (entry.ally_count) parts.push(entry.ally_count + " as ally");
        if (entry.enemy_count) parts.push(entry.enemy_count + " as enemy");
        var breakdown = parts.length ? " (" + parts.join(", ") + ")" : "";
        return txt(entry.times, "0") + breakdown;
    }

    function formatEncounterRecord(entry) {
        var parts = [];
        if (entry.ally_count) {
            var r = entry.ally_wins + "W-" + entry.ally_losses + "L";
            if (entry.ally_unknown) r += " (" + entry.ally_unknown + " unknown)";
            parts.push("Ally " + r);
        }
        if (entry.enemy_count) {
            var r2 = entry.enemy_wins + "W-" + entry.enemy_losses + "L";
            if (entry.enemy_unknown) r2 += " (" + entry.enemy_unknown + " unknown)";
            parts.push("Enemy " + r2);
        }
        return parts.length ? parts.join(" / ") : NA;
    }

    function setStatChip(container, label, value, isNA, chipClass, textColor, iconUrl) {
        var chip = document.createElement("div");
        chip.className = chipClass || "stat-chip";
        var l = document.createElement("span");
        l.className = "stat-label";
        l.textContent = label;
        var v = document.createElement("span");
        v.className = "stat-value" + (isNA ? " na" : "");
        if (iconUrl) {
            var img = document.createElement("img");
            img.className = "rank-icon";
            img.src = iconUrl;
            img.loading = "lazy";
            v.append(img);
            v.append(" ");
        }
        v.append(document.createTextNode(value));
        if (textColor && !isNA) v.style.color = textColor;
        chip.append(l, v);
        container.append(chip);
    }

    function buildCardStats(player) {
        var grid = document.createElement("div");
        grid.className = "card-stats";
        setStatChip(grid, "Win Rate", winRateDisplay(player.winPercentage), isEmpty(player.winPercentage), "card-stat");
        setStatChip(grid, "Rank", rankName(player.rank, true), isEmpty(player.rank), "card-stat", rankColor(player.rank), state.rankIcons && state.rankIcons[player.rank]);
        setStatChip(grid, "RR", txt(player.rr), isEmpty(player.rr), "card-stat");
        setStatChip(grid, "Leaderboard", isEmpty(player.leaderboard) ? NA : (Number(player.leaderboard) <= 0 ? NA : "#" + player.leaderboard), isEmpty(player.leaderboard), "card-stat");
        setStatChip(grid, "Peak Rank", (rn=>rn!==NA&&player.peakRankAct?rn+String(player.peakRankAct).trim():rn)(rankName(player.peakRank,true)), isEmpty(player.peakRank), "card-stat", rankColor(player.peakRank), state.rankIcons && state.rankIcons[player.peakRank]);
        setStatChip(grid, "Last Act", rankName(player.previousRank, true), isEmpty(player.previousRank), "card-stat", rankColor(player.previousRank), state.rankIcons && state.rankIcons[player.previousRank]);
        setStatChip(grid, "Level", txt(player.level), isEmpty(player.level), "card-stat");
        setStatChip(grid, "Last Active", txt(player.lastActive), isEmpty(player.lastActive), "card-stat");
        return grid;
    }

    function showToast(message) {
        els.toast.textContent = message;
        els.toast.classList.add("show");
        clearTimeout(showToast._t);
        showToast._t = setTimeout(function () { els.toast.classList.remove("show"); }, 3200);
    }

    function takeScreenshot() {
        if (typeof html2canvas === "undefined") { showToast("Screenshot library not loaded yet."); return; }
        els.screenshotButton.disabled = true;
        var target = document.querySelector(".teams-layout");
        if (!target) { showToast("No teams layout found."); els.screenshotButton.disabled = false; return; }
        var cleanup = [];
        function suppressOverlays() {
            var s = document.createElement("style");
            s.id = "tmp-scr";
            s.textContent = "body::before { display: none !important; }.player-button { background: rgba(10, 14, 24, 0.92) !important; }.player-button::after { display: none !important; }.player-button.self-card,.player-button.is-blue,.player-button.is-red { background: rgba(10, 14, 24, 0.92) !important; box-shadow: 0 18px 40px rgba(0,0,0,0.5) !important; }.player-button:hover,.player-button:focus-visible,.player-button.is-selected { transform: none !important; }* { animation: none !important; }";
            document.head.appendChild(s);
            cleanup.push(function () { var el = document.getElementById("tmp-scr"); if (el) el.remove(); });
        }
        function hideDetailsPanel() {
            var panel = els.detailsPanel;
            if (!panel.hidden) { panel.hidden = true; cleanup.push(function () { panel.hidden = false; }); }
        }
        suppressOverlays();
        hideDetailsPanel();
        document.body.classList.add("show-you-badge");
        html2canvas(target, { scale: 2, useCORS: true, backgroundColor: "#0f1115" })
            .then(function (canvas) {
                cleanup.forEach(function (fn) { fn(); });
                document.body.classList.remove("show-you-badge");
                canvas.toBlob(function (blob) {
                    if (!blob) { showToast("Screenshot failed."); els.screenshotButton.disabled = false; return; }
                    if (navigator.clipboard && navigator.clipboard.write) {
                        navigator.clipboard.write([new ClipboardItem({ "image/png": blob })])
                            .then(function () { els.toast.classList.add("is-success"); showToast("Screenshot copied!"); setTimeout(function () { els.toast.classList.remove("is-success"); }, 600); })
                            .catch(function () { showToast("Copy failed."); });
                    } else { showToast("Clipboard API unavailable."); }
                    els.screenshotButton.disabled = false;
                }, "image/png");
            }).catch(function () { cleanup.forEach(function (fn) { fn(); }); document.body.classList.remove("show-you-badge"); showToast("Screenshot failed."); els.screenshotButton.disabled = false; });
    }

    function setStatus(text, cls) {
        els.statusPill.className = "status-pill " + cls;
        els.statusText.textContent = text;
        updateLoadingOverlay(cls);
    }

    // ---- Tauri IPC setup ----
    var unlisteners = [];

    function setupTauriListeners() {
        tauriListen("heartbeat", function (event) {
            setStatus("Connected", "live");
            if (event.payload && event.payload.players) {
                state.lastGameState = event.payload.state;
                setPayload(event.payload, true);
            }
        }).then(function (fn) { unlisteners.push(fn); });

        tauriListen("state_change", function (event) {
            if (event.payload && event.payload.state) {
                renderStateTransition(event.payload.state);
            }
        }).then(function (fn) { unlisteners.push(fn); });

        tauriListen("backend_ready", function () {
            setStatus("Connected", "live");
        }).then(function (fn) { unlisteners.push(fn); });
    }

    function cleanupTauriListeners() {
        unlisteners.forEach(function (fn) { fn(); });
        unlisteners = [];
    }

    // ---- loading overlay ----
    var loadingLogPollTimer = null;

    function pollLoadingLogs() {
        tauriInvoke("get_gui_log_tail").then(function (text) {
            if (els.loadingLogTail) {
                els.loadingLogTail.textContent = text || "";
                els.loadingLogTail.scrollTop = els.loadingLogTail.scrollHeight;
            }
        }).catch(function () {});
    }

    function updateLoadingOverlay(cls) {
        var isLive = cls === "live";
        els.loadingOverlay.hidden = isLive;
        if (isLive && loadingLogPollTimer) { clearInterval(loadingLogPollTimer); loadingLogPollTimer = null; return; }
        if (!isLive && !loadingLogPollTimer) {
            pollLoadingLogs();
            loadingLogPollTimer = setInterval(pollLoadingLogs, 1000);
        }
    }

    // ---- state / rendering ----
    function payloadRenderKey(payload) {
        return JSON.stringify(payload, function (key, value) { return key === "time" ? undefined : value; });
    }

    function bumpTimestampOnly(payload) {
        if (!payload || !payload.time) return;
        var chip = document.getElementById("metaUpdatedChip");
        if (chip) chip.textContent = "Updated " + new Date(payload.time * 1000).toLocaleTimeString();
    }

    function setPayload(payload, shouldCache) {
        var renderKey = payloadRenderKey(payload);
        var unchanged = renderKey === state.lastRenderKey;
        state.payload = payload;
        state.rankIcons = payload && payload.rankIcons;
        if (shouldCache) { try { localStorage.setItem("vry-rust.cache", JSON.stringify(payload)); } catch (e) {} }
        if (unchanged) { bumpTimestampOnly(payload); return; }
        state.lastRenderKey = renderKey;
        state.players = normalizePlayers(payload);
        if (!state.players.some(function (p) { return p.puuid === state.selectedPuuid; })) { state.selectedPuuid = null; }
        render();
    }

    function normalizePlayers(payload) {
        var rawPlayers = (payload && payload.players) || {};
        var myPuuid = payload && payload.puuid;
        return Object.keys(rawPlayers).map(function (puuid) {
            var p = rawPlayers[puuid] || {};
            p.puuid = p.puuid || puuid;
            p.isSelf = !!(myPuuid && p.puuid === myPuuid);
            return p;
        }).filter(function (p) { return p.name || p.agent || p.weapons; }).sort(function (a, b) {
            // Self first
            if (a.isSelf) return -1;
            if (b.isSelf) return 1;
            // Party members next (non-zero partyNumber), grouped together
            var pa = Number(a.partyNumber) || 0;
            var pb = Number(b.partyNumber) || 0;
            if (pa !== pb) return pb - pa;
            // Team ordering (Blue before Red)
            var t = teamRank(a.team) - teamRank(b.team);
            if (t !== 0) return t;
            // By rank descending (higher rank first)
            var ra = Number(a.rank) || 0;
            var rb = Number(b.rank) || 0;
            if (ra !== rb) return rb - ra;
            // Alphabetically by name as final tiebreaker
            return String(a.name || "").localeCompare(String(b.name || ""));
        });
    }

    function teamRank(team) { if (team === "Blue") return 0; if (team === "Red") return 1; return 2; }
    function teamClass(team) { if (team === "Blue") return "is-blue"; if (team === "Red") return "is-red"; return ""; }

    function render() { renderMeta(); renderPlayers(); renderPlayedWith(); renderDetails(); renderJson(); }

    var STATE_LABELS = { INGAME: "In-Game", PREGAME: "Agent Select", MENUS: "In-Menus", DISCONNECTED: "Disconnected" };
    var STATE_CLASSES = { INGAME: "state-ingame", PREGAME: "state-pregame", MENUS: "state-menus", DISCONNECTED: "state-disconnected" };

    function renderStateTransition(newState) {
        // When going from pregame to in-game, keep the existing UI visible
        // (same players, only stats update). For all other transitions, clear
        // everything and show a loading state.
        var isPregameToIngame = (state.lastGameState === "PREGAME" && newState === "INGAME");
        if (isPregameToIngame) {
            state.lastRenderKey = null; // Ensure next heartbeat triggers a re-render
            var label = STATE_LABELS[newState] || newState || "Unknown";
            els.matchMeta.replaceChildren();
            var chip = document.createElement("span");
            chip.className = "meta-chip";
            var spinner = document.createElement("span");
            spinner.className = "meta-spinner";
            chip.append(spinner, document.createTextNode("Loading " + label + " Data\u2026"));
            els.matchMeta.append(chip);
            return;
        }
        // Original clearing behavior for all other transitions
        state.payload = null;
        state.lastRenderKey = null;
        state.players = [];
        if (els.jsonPanelSummary) els.jsonPanelSummary.textContent = "Raw heartbeat JSON";
        if (els.jsonPanelEmptySummary) els.jsonPanelEmptySummary.textContent = "Raw heartbeat JSON";
        var label = STATE_LABELS[newState] || newState || "Unknown";
        els.matchMeta.replaceChildren();
        var chip = document.createElement("span");
        chip.className = "meta-chip";
        var spinner = document.createElement("span");
        spinner.className = "meta-spinner";
        chip.append(spinner, document.createTextNode("Loading " + label + " Data\u2026"));
        els.matchMeta.append(chip);
        // Clear player grids and show loading state across full width
        els.blueGrid.replaceChildren();
        els.redGrid.replaceChildren();
        els.defHeader.textContent = label;
        els.defSection.hidden = false;
        els.atkSection.hidden = true;
        els.teamDivider.hidden = true;
        document.querySelector(".teams-layout").classList.add("is-unified");
        var loadMsg = document.createElement("div");
        loadMsg.className = "loading-grid-message";
        var loadSpinner = document.createElement("span");
        loadSpinner.className = "meta-spinner";
        loadMsg.append(loadSpinner, document.createTextNode(" Waiting for " + label + " data\u2026"));
        els.blueGrid.append(loadMsg);
    }

    function renderMeta() {
        var p = state.payload;
        els.matchMeta.replaceChildren();
        if (!p) {
            var w = document.createElement("span");
            w.className = "meta-chip updated";
            var s = document.createElement("span");
            s.className = "meta-spinner";
            w.append(s, document.createTextNode("Waiting for data\u2026"));
            els.matchMeta.append(w);
            return;
        }
        var segments = [];
        segments.push({ text: STATE_LABELS[p.state] || txt(p.state, "Unknown"), cls: STATE_CLASSES[p.state] || "" });
        if (p.mode) segments.push({ text: p.mode, cls: "mode" });
        var mapVal = p.map;
        if (Array.isArray(mapVal)) mapVal = mapVal[0];
        if (mapVal) segments.push({ text: mapVal, cls: "map" });
        if (p.server) segments.push({ text: p.server, cls: "server" });
        if (p.time) { var d = new Date(p.time * 1000); segments.push({ text: "Updated " + d.toLocaleTimeString(), cls: "updated", id: "metaUpdatedChip" }); }
        segments.forEach(function (seg, i) {
            if (i > 0) { var sep = document.createElement("span"); sep.className = "meta-sep"; sep.textContent = "\u2022"; els.matchMeta.append(sep); }
            appendMetaChip(seg.text, seg.cls, seg.id);
        });
    }

    function appendMetaChip(text, cls, id) {
        var chip = document.createElement("span");
        chip.className = "meta-chip" + (cls ? " " + cls : "");
        if (id) chip.id = id;
        chip.textContent = text;
        els.matchMeta.append(chip);
    }

    function renderPlayers() {
        els.blueGrid.replaceChildren();
        els.redGrid.replaceChildren();
        renderTeamHeaders();
        if (state.players.length === 0) {
            var msg = document.createElement("div");
            msg.className = "loading-grid-message";
            msg.textContent = "Waiting for player data\u2026";
            els.blueGrid.append(msg);
            return;
        }
        var selfTeam = myTeam(state.payload);
        state.players.forEach(function (player) {
            var button = document.createElement("button");
            button.type = "button";
            button.className = "player-button " + teamClass(player.team);
            button.classList.toggle("self-card", player.isSelf);
            button.classList.toggle("is-selected", player.puuid === state.selectedPuuid);
            button.title = txt(player.name, "Unknown Player") + COPY_HINT;
            button.addEventListener("click", function () { state.selectedPuuid = player.puuid; render(); });
            button.addEventListener("contextmenu", function (e) {
                e.preventDefault();
                var text = stripHint(e.target.title || button.title);
                navigator.clipboard.writeText(text).then(function () { showToast('Copied: ' + text); }, function () { showToast('Failed to copy.'); });
            });
            var avatar = buildAgentAvatar(player.agentImgLink, player.agent);
            var identity = document.createElement("div");
            identity.className = "player-main";
            var name = document.createElement("span");
            name.className = "player-name";
            name.textContent = txt(player.name, "Unknown Player");
            name.title = name.textContent + COPY_HINT;
            var youBadge = null;
            if (player.isSelf) { youBadge = document.createElement("span"); youBadge.className = "self-badge"; youBadge.textContent = "You"; }
            var agent = document.createElement("span");
            agent.className = "agent-name";
            agent.textContent = txt(player.agent, "Agent " + NA);
            agent.title = agent.textContent;
            var metaRow = document.createElement("span");
            metaRow.className = "player-meta-row";
            var rankBadge = document.createElement("span");
            rankBadge.className = "player-meta";
            var rankIconUrl = state.rankIcons && state.rankIcons[player.rank];
            if (rankIconUrl) { var ri = document.createElement("img"); ri.className = "rank-icon"; ri.src = rankIconUrl; ri.loading = "lazy"; rankBadge.append(ri); rankBadge.append(" "); }
            rankBadge.append(document.createTextNode(rankName(player.rank, true)));
            var bc = rankColor(player.rank);
            if (bc) rankBadge.style.color = bc;
            metaRow.append(rankBadge);
            var action = document.createElement("span");
            action.className = "player-action";
            action.textContent = "View loadout & stats";
            identity.append(name);
            if (youBadge) identity.append(youBadge);
            identity.append(agent, metaRow, action);
            button.append(avatar, identity, buildCardStats(player), buildPreviewRow(player));
            // Always put self's team on the LEFT (blueGrid), other team on the RIGHT (redGrid)
            var isSelfTeam = !selfTeam || player.team === selfTeam;
            var grid = isSelfTeam ? els.blueGrid : els.redGrid;
            grid.append(button);
        });
    }

    function myTeam(payload) {
        if (!payload || !payload.players) return null;
        for (var k in payload.players) { if (payload.players[k].isSelf) return payload.players[k].team; }
        return null;
    }

    function teamSide(team) {
        if (team === "Blue") return "DEF";
        if (team === "Red") return "ATK";
        return "";
    }

    function renderTeamHeaders() {
        var payload = state.payload;
        if (!payload || payload.state === "MENUS" || payload.state === "DISCONNECTED") {
            els.defHeader.textContent = "PARTY";
            els.defHeader.classList.toggle("is-my-team", true);
            els.defSection.hidden = false;
            els.atkSection.hidden = true;
            els.teamDivider.hidden = true;
            document.querySelector(".teams-layout").classList.add("is-unified");
            return;
        }
        var selfTeam = myTeam(payload);
        var hasBlue = false, hasRed = false;
        for (var k in payload.players) {
            var p = payload.players[k];
            if (p.team === "Blue") hasBlue = true;
            if (p.team === "Red") hasRed = true;
        }
        var singleTeam = (hasBlue && !hasRed) || (!hasBlue && hasRed);
        document.querySelector(".teams-layout").classList.toggle("is-unified", singleTeam);
        if (singleTeam) {
            els.teamDivider.hidden = true;
            var onlyTeam = hasBlue ? "Blue" : "Red";
            var isMyTeam = selfTeam === onlyTeam;
            els.atkSection.hidden = true;
            els.defSection.hidden = false;
            els.defHeader.textContent = (isMyTeam ? "ALLY" : "ENEMY") + " (" + teamSide(onlyTeam) + ")";
            els.defHeader.classList.toggle("is-my-team", isMyTeam);
        } else {
            els.defSection.hidden = false; els.atkSection.hidden = false; els.teamDivider.hidden = false;
            // LEFT side (defSection/blueGrid) = self's team, RIGHT side (atkSection/redGrid) = other team
            var leftTeam = selfTeam || "Blue";
            var rightTeam = (leftTeam === "Blue") ? "Red" : "Blue";
            els.defHeader.textContent = (leftTeam === selfTeam ? "ALLY" : "ENEMY") + " (" + teamSide(leftTeam) + ")";
            els.atkHeader.textContent = (rightTeam === selfTeam ? "ALLY" : "ENEMY") + " (" + teamSide(rightTeam) + ")";
            els.defHeader.classList.toggle("is-my-team", leftTeam === selfTeam);
            els.atkHeader.classList.toggle("is-my-team", rightTeam === selfTeam);
        }
    }

    function buildPreviewRow(player) {
        var row = document.createElement("div");
        row.className = "preview-row";
        PREVIEW_WEAPONS.forEach(function (weaponName) {
            var weapon = getWeapon(player, weaponName);
            var slot = document.createElement("span");
            slot.className = "preview-slot";
            if (weapon && weapon.skinDisplayIcon) { var img = document.createElement("img"); img.src = weapon.skinDisplayIcon; img.alt = weapon.skinDisplayName || weapon.weapon || "Weapon"; slot.append(img); }
            var copy = document.createElement("span");
            copy.className = "preview-copy";
            var label = document.createElement("span");
            label.className = "preview-label";
            label.textContent = weaponName;
            var name = document.createElement("span");
            name.className = "preview-name";
            var skinLabel = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
            if (weapon && weapon.skinDisplayName && weaponName !== "Melee") { skinLabel = stripWeaponName(skinLabel, weaponName); }
            name.textContent = skinLabel;
            name.title = name.textContent;
            copy.append(label, name);
            slot.append(copy);
            row.append(slot);
        });
        return row;
    }

    function renderDetails() {
        var selected = state.players.find(function (p) { return p.puuid === state.selectedPuuid; });
        var hasSelection = Boolean(selected);
        els.detailsPanel.hidden = !hasSelection;
        els.emptyState.hidden = hasSelection || state.players.length > 0;
        if (!hasSelection && state.players.length === 0) { els.emptyStateTitle.textContent = "No match data"; els.emptyStateText.textContent = "Waiting for VRY backend data."; }
        else if (!hasSelection) { els.emptyStateTitle.textContent = "No player selected"; els.emptyStateText.textContent = "Click a player card to view loadout and stats."; }
        if (!selected) return;
        els.selectedAgent.src = selected.agentImgLink || "";
        els.selectedAgent.hidden = !selected.agentImgLink;
        els.selectedAgent.alt = selected.agent || "";
        els.selectedName.textContent = txt(selected.name, "Unknown Player");
        els.selectedName.title = txt(selected.name, "Unknown Player") + COPY_HINT;
        var hasName = selected.name && selected.name.indexOf("#") !== -1;
        if (hasName) {
            var trnHref = "https://tracker.gg/valorant/profile/riot/" + encodeURIComponent(selected.name) + "/overview";
            var vtlHref = "https://vtl.lol/id/" + encodeURIComponent(selected.name.replace("#", "_"));
            els.trnLink.href = trnHref; els.trnLink.title = trnHref + COPY_HINT; els.trnLink.hidden = false;
            els.vtlLink.href = vtlHref; els.vtlLink.title = vtlHref + COPY_HINT; els.vtlLink.hidden = false;
        } else { els.trnLink.hidden = true; els.vtlLink.hidden = true; }
        els.selectedCardTitle.textContent = selected.title || "";
        els.selectedCardTitle.hidden = !selected.title;
        els.selectedCardTitle.title = (selected.title || "Title") + COPY_HINT;
        els.selectedModalName.textContent = txt(selected.agent, "Agent " + NA);
        els.selectedLevel.textContent = isEmpty(selected.level) ? "Level " + NA : ("Level " + selected.level);
        els.selectedTeam.textContent = selected.team ? ((myTeam(state.payload) === selected.team) ? "ALLY" : "ENEMY") : "Unknown";
        els.selectedTeam.className = "team-pill " + teamClass(selected.team);
        if (selected.playerCard) {
            els.playerCardPreview.style.backgroundImage = "url(\"" + selected.playerCard + "\")";
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
        setStatChip(els.statBar, "Win Rate", winRateDisplay(player.winPercentage), isEmpty(player.winPercentage), "stat-chip");
        setStatChip(els.statBar, "Rank", rankName(player.rank, false), isEmpty(player.rank), "stat-chip", rankColor(player.rank), state.rankIcons && state.rankIcons[player.rank]);
        setStatChip(els.statBar, "RR", txt(player.rr), isEmpty(player.rr), "stat-chip");
        setStatChip(els.statBar, "Leaderboard", isEmpty(player.leaderboard) ? NA : (Number(player.leaderboard) <= 0 ? NA : "#" + player.leaderboard), isEmpty(player.leaderboard), "stat-chip");
        setStatChip(els.statBar, "Peak Rank", (rn=>rn!==NA&&player.peakRankAct?rn+String(player.peakRankAct).trim():rn)(rankName(player.peakRank,false)), isEmpty(player.peakRank), "stat-chip", rankColor(player.peakRank), state.rankIcons && state.rankIcons[player.peakRank]);
        setStatChip(els.statBar, "Last Act", rankName(player.previousRank, false), isEmpty(player.previousRank), "stat-chip", rankColor(player.previousRank), state.rankIcons && state.rankIcons[player.previousRank]);
        setStatChip(els.statBar, "Level", txt(player.level), isEmpty(player.level), "stat-chip");
        setStatChip(els.statBar, "Last Active", txt(player.lastActive), isEmpty(player.lastActive), "stat-chip");
    }

    function renderExpressions(player) {
        els.expressionGrid.replaceChildren();
        var expressions = Object.keys(player.sprays || {}).map(function (idx) {
            var e = player.sprays[idx] || {};
            return Object.assign({ index: Number(idx) }, e);
        }).sort(function (a, b) { return a.index - b.index; });
        var slots = expressions.slice(0, 4);
        while (slots.length < 4) slots.push(null);
        slots.forEach(function (expression, idx) {
            var tile = document.createElement("div");
            tile.className = "expression-tile expression-slot-" + idx;
            tile.title = (expression ? (expression.displayName || "Expression") : ("Empty slot " + (idx + 1))) + COPY_HINT;
            tile.setAttribute("aria-label", tile.title);
            if (expression && expression.type === "flex") tile.classList.add("is-flex");
            var art = document.createElement("div");
            art.className = "expression-art";
            var iconSrc = expression && (expression.fullTransparentIcon || expression.displayIcon);
            if (iconSrc) { var img = document.createElement("img"); img.src = iconSrc; img.alt = (expression && expression.displayName) || "Expression"; art.append(img); }
            var copy = document.createElement("div");
            copy.className = "expression-copy";
            var name = document.createElement("strong");
            name.textContent = expression ? (expression.displayName || NA) : ("Slot " + (idx + 1));
            var type = document.createElement("span");
            type.className = "expression-type";
            type.textContent = expression ? (expression.type || "expression") : "empty";
            copy.append(name, type);
            tile.append(art, copy);
            els.expressionGrid.append(tile);
            tile.addEventListener("contextmenu", function (e) {
                e.preventDefault();
                var text = stripHint(e.target.title || tile.title);
                navigator.clipboard.writeText(text).then(function () { showToast("Copied: " + text); }, function () { showToast("Failed to copy."); });
            });
        });
    }

    function renderWeapons(player) {
        els.weaponGroups.replaceChildren();
        WEAPON_COLUMNS.forEach(function (column) {
            var columnNode = document.createElement("div");
            columnNode.className = "weapon-column " + column.className;
            column.groups.forEach(function (group) {
                var section = document.createElement("section");
                section.className = "weapon-group weapon-group-" + group.slug;
                var heading = document.createElement("h3");
                heading.textContent = group.title;
                var grid = document.createElement("div");
                grid.className = "weapon-grid";
                group.weapons.forEach(function (weaponName) { grid.append(buildWeaponTile(player, weaponName)); });
                section.append(heading, grid);
                columnNode.append(section);
            });
            els.weaponGroups.append(columnNode);
        });
    }

    function buildWeaponTile(player, weaponName) {
        var weapon = getWeapon(player, weaponName);
        var tile = document.createElement("div");
        tile.className = "weapon-tile";
        tile.classList.toggle("is-empty", !weapon);
        var vc = chromaColor(weapon && weapon.chromaDisplayName);
        vc = vc ? " (" + vc + ")" : "";
        tile.title = (weapon ? (weaponName + ": " + (weapon.skinDisplayName || weapon.weapon || "Unknown skin") + vc) : (weaponName + ": " + NA)) + COPY_HINT;
        var art = document.createElement("div");
        art.className = "weapon-art";
        var iconSrc = weapon && (weapon.skinDisplayIcon || weapon.weaponDisplayIcon);
        if (iconSrc) { var img = document.createElement("img"); img.src = iconSrc; img.alt = weapon.skinDisplayName || weapon.weapon || weaponName; art.append(img); }
        var copy = document.createElement("div");
        copy.className = "weapon-copy";
        var label = document.createElement("span");
        label.className = "weapon-name";
        label.textContent = weaponName;
        var name = document.createElement("strong");
        name.textContent = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
        name.title = name.textContent + vc + COPY_HINT;
        copy.append(label, name);
        tile.append(art, copy);
        tile.addEventListener("contextmenu", function (e) {
            e.preventDefault();
            var text = stripHint(e.target.title || tile.title);
            navigator.clipboard.writeText(text).then(function () { showToast("Copied: " + text); }, function () { showToast("Failed to copy."); });
        });
        if (weapon && weapon.buddy_displayIcon) {
            tile.classList.add("has-buddy");
            var buddy = document.createElement("img");
            buddy.className = "buddy";
            buddy.src = weapon.buddy_displayIcon;
            buddy.alt = weapon.buddy_displayName || "Buddy";
            buddy.title = (weapon.buddy_displayName || "Buddy") + COPY_HINT;
            buddy.addEventListener("contextmenu", function (e) { e.stopPropagation(); e.preventDefault(); var text = stripHint(e.target.title || buddy.title); navigator.clipboard.writeText(text).then(function () { showToast("Copied: " + text); }, function () { showToast("Failed to copy."); }); });
            tile.append(buddy);
        }
        return tile;
    }

    function getWeapon(player, weaponName) {
        var weapons = player.weapons || {};
        var match = null;
        Object.keys(weapons).forEach(function (key) { var w = weapons[key]; if (w && w.weapon === weaponName) match = w; });
        return match;
    }

    function buildAgentAvatar(src, alt) {
        if (!src) return makeAvatarPlaceholder();
        var img = document.createElement("img");
        img.className = "agent-avatar";
        img.alt = alt || "";
        img.src = src;
        img.addEventListener("error", function () { img.replaceWith(makeAvatarPlaceholder()); });
        return img;
    }

    function makeAvatarPlaceholder() {
        var el = document.createElement("div");
        el.className = "agent-avatar agent-avatar-placeholder";
        el.textContent = "?";
        el.setAttribute("aria-label", "Unknown agent");
        return el;
    }

    function capitalize(str) { return str ? str.charAt(0).toUpperCase() + str.slice(1) : ""; }

    function renderPlayedWith() {
        var payload = state.payload;
        var entries = (payload && Array.isArray(payload.alreadyPlayedWith)) ? payload.alreadyPlayedWith : [];

        // Only show players currently in this lobby/game
        var currentPlayerNames = {};
        if (payload && payload.players) {
            var myPuuid = payload.puuid;
            Object.keys(payload.players).forEach(function (puuid) {
                if (puuid !== myPuuid && payload.players[puuid].name) {
                    currentPlayerNames[payload.players[puuid].name] = true;
                }
            });
        }
        entries = entries.filter(function (entry) {
            return currentPlayerNames[entry.name] === true;
        });

        els.playedWithEmpty.hidden = entries.length > 0;
        els.playedWithTable.hidden = entries.length === 0;
        if (!entries.length) return;
        els.playedWithBody.replaceChildren();
        entries.forEach(function (entry) {
            var row = document.createElement("tr");
            var nc = document.createElement("td"); nc.textContent = txt(entry.name, "Unknown"); row.append(nc);
            var rel = entry.relation === "ally" ? "Ally" : "Enemy";
            var cc = document.createElement("td"); cc.textContent = rel + " " + txt(entry.agent, "Unknown"); row.append(cc);
            var tc = document.createElement("td"); tc.textContent = formatEncounterTimes(entry); row.append(tc);
            var lc = document.createElement("td"); lc.textContent = capitalize(txt(entry.relation_name, "player")) + " " + txt(entry.agent, "Unknown") + " on " + txt(entry.map, "Unknown") + " \u2014 " + formatTimeAgo(entry.time_diff) + " ago"; row.append(lc);
            var rc = document.createElement("td"); rc.textContent = formatEncounterRecord(entry); row.append(rc);
            els.playedWithBody.append(row);
        });
    }

    function renderJson() {
        var text = state.payload ? JSON.stringify(state.payload, null, 2) : "No data yet.";
        els.jsonPre.textContent = text;
        els.jsonPreEmpty.textContent = text;
        if (els.jsonPanelSummary) els.jsonPanelSummary.textContent = "Raw heartbeat JSON";
        if (els.jsonPanelEmptySummary) els.jsonPanelEmptySummary.textContent = "Raw heartbeat JSON";
    }

    function copyText(text, button) {
        if (!text) { showToast("Nothing to copy yet."); return; }
        var done = function () {
            var original = button.textContent;
            button.textContent = "Copied!";
            button.classList.add("copied");
            setTimeout(function () { button.textContent = original; button.classList.remove("copied"); }, 1500);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).then(done).catch(function () { fallbackCopy(text, done); });
        } else { fallbackCopy(text, done); }
    }

    function copyJson(button) {
        var text = state.payload ? JSON.stringify(state.payload, null, 2) : "";
        copyText(text, button);
    }

    function renderLogTail() {
        tauriInvoke("get_gui_log_tail").then(function (text) {
            var displayText = text || "(empty log)";
            if (els.logPre.textContent !== displayText) {
                els.logPre.textContent = displayText;
            }
        }).catch(function () {
            if (els.logPre.textContent !== "Failed to fetch log.") {
                els.logPre.textContent = "Failed to fetch log.";
            }
        });
    }

    function renderHeartbeatTail() {
        tauriInvoke("get_heartbeat_log").then(function (text) {
            var displayText = text || "(no heartbeat data yet)";
            if (els.hbPre.textContent !== displayText) {
                els.hbPre.textContent = displayText;
            }
        }).catch(function () {
            if (els.hbPre.textContent !== "Failed to fetch heartbeat log.") {
                els.hbPre.textContent = "Failed to fetch heartbeat log.";
            }
        });
    }

    function fallbackCopy(text, done) {
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.append(ta);
        ta.select();
        try { document.execCommand("copy"); done(); } catch (e) { showToast("Copy failed."); }
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
        try { localStorage.removeItem("vry-rust.cache"); } catch (e) {}
        state.payload = null;
        state.players = [];
        state.lastRenderKey = null;
        state.selectedPuuid = null;
        render();
        setStatus("Reconnecting", "pending");
        tauriInvoke("restart_application").then(function () {
            showToast("Reconnecting to backend...");
        }).catch(function () {
            showToast("Connection error, retrying...");
        }).finally(function () {
            setTimeout(function () { setRefreshButtonsBusy(false); }, 4000);
        });
    }

    // ---- wiring ----
    els.closeDetailsButton.addEventListener("click", function () { state.selectedPuuid = null; render(); });
    els.detailsPanel.addEventListener("click", function (e) { if (e.target === els.detailsPanel) { state.selectedPuuid = null; render(); } });
    window.addEventListener("keydown", function (e) {
        if (e.key === "Escape") { if (!els.confirmModal.hidden) { closeModal(); return; } if (state.selectedPuuid) { state.selectedPuuid = null; render(); } }
    });
    els.refreshButton.addEventListener("click", openModal);
    els.loadingRefreshButton.addEventListener("click", openModal);
    els.modalCancel.addEventListener("click", closeModal);
    els.modalConfirm.addEventListener("click", requestRestart);
    els.confirmModal.addEventListener("click", function (e) { if (e.target === els.confirmModal) closeModal(); });
    els.jsonCopyBtn.addEventListener("click", function () { copyJson(els.jsonCopyBtn); });
    els.jsonCopyBtnEmpty.addEventListener("click", function () { copyJson(els.jsonCopyBtnEmpty); });
    els.trnLink.addEventListener("contextmenu", function (e) { e.preventDefault(); navigator.clipboard.writeText(stripHint(this.title)).then(function () { showToast("Copied."); }, function () { showToast("Failed."); }); });
    els.vtlLink.addEventListener("contextmenu", function (e) { e.preventDefault(); navigator.clipboard.writeText(stripHint(this.title)).then(function () { showToast("Copied."); }, function () { showToast("Failed."); }); });
    [els.playerCardPreview, els.selectedName, els.selectedCardTitle].forEach(function (el) {
        if (!el) return;
        el.addEventListener("contextmenu", function (e) {
            e.preventDefault();
            e.stopPropagation();
            var text = stripHint(this.title || this.textContent);
            navigator.clipboard.writeText(text).then(function () { showToast("Copied: " + text); }, function () { showToast("Failed to copy."); });
        });
    });
    els.screenshotButton.addEventListener("click", takeScreenshot);

    els.logPanel.addEventListener("toggle", function () {
        if (els.logPanel.open) {
            renderLogTail();
            els.logInterval = setInterval(renderLogTail, 1000);
        } else {
            clearInterval(els.logInterval);
            els.logInterval = null;
        }
    });
    els.logCopyBtn.addEventListener("click", function () { copyText(els.logPre.textContent, els.logCopyBtn); });
    els.logRefreshBtn.addEventListener("click", renderLogTail);

    els.hbPanel.addEventListener("toggle", function () {
        if (els.hbPanel.open) {
            renderHeartbeatTail();
            els.hbInterval = setInterval(renderHeartbeatTail, 1000);
        } else {
            clearInterval(els.hbInterval);
            els.hbInterval = null;
        }
    });
    els.hbCopyBtn.addEventListener("click", function () { copyText(els.hbPre.textContent, els.hbCopyBtn); });
    els.hbRefreshBtn.addEventListener("click", renderHeartbeatTail);

    // ---- boot ----
    var cached = null;
    try { cached = localStorage.getItem("vry-rust.cache"); } catch (e) {}
    if (cached) {
        try { setPayload(JSON.parse(cached), false); } catch (e) { try { localStorage.removeItem("vry-rust.cache"); } catch (e2) {} }
    } else { render(); }

    setStatus("Starting", "pending");
    setupTauriListeners();
})();
