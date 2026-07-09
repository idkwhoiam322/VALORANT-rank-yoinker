(function () {
    "use strict";

    var COPY_HINT = " (Right-click to copy)";
    var stripHint = function (t) { return t ? t.replace(COPY_HINT, "") : ""; };
    var chromaColor = function (name) { var m = name && name.match(/\(Variant \d+ (.+)\)$/); return m ? m[1] : ""; };

    // ---- static lookup tables (mirrors src/constants.py NUMBERTORANKS / SHORT_NUMBERTORANKS) ----
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
    var CACHE_KEY = "vry.testPage.cache";
    var DEFAULT_PORT = "1100";
    var PREVIEW_WEAPONS = ["Vandal", "Phantom", "Melee"];

    var state = {
        socket: null,
        payload: null,
        players: [],
        selectedPuuid: null,
        reconnectTimer: null,
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
        defHeader: document.getElementById("defHeader"),
        atkHeader: document.getElementById("atkHeader"),
        defSection: document.getElementById("defSection"),
        atkSection: document.getElementById("atkSection"),
        teamDivider: document.getElementById("teamDivider"),
        screenshotButton: document.getElementById("screenshotButton"),
        playedWithEmpty: document.getElementById("playedWithEmpty"),
        playedWithTable: document.getElementById("playedWithTable"),
        playedWithBody: document.getElementById("playedWithBody"),
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

    // ---------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------

    function isEmpty(v) {
        return v === null || v === undefined || v === "" || (typeof v === "number" && Number.isNaN(v));
    }

    function txt(v, fallback) {
        return isEmpty(v) ? (fallback === undefined ? NA : fallback) : String(v);
    }

    function rankName(rankIndex, short) {
        if (isEmpty(rankIndex)) return NA;
        var idx = Number(rankIndex);
        var table = short ? RANK_NAMES_SHORT : RANK_NAMES_FULL;
        if (Number.isNaN(idx) || idx < 0 || idx >= table.length) return NA;
        return table[idx];
    }

    function rankColor(rankIndex) {
        var idx = Number(rankIndex);
        if (isEmpty(rankIndex) || Number.isNaN(idx) || idx < 0 || idx >= RANK_COLORS.length) return null;
        var rgb = RANK_COLORS[idx];
        return "rgb(" + rgb[0] + ", " + rgb[1] + ", " + rgb[2] + ")";
    }

    function winRateDisplay(v) {
        // Backend sends "62 (10)" (winrate, game count) with no "%".
        if (isEmpty(v)) return NA;
        var s = String(v);
        if (s.indexOf("%") !== -1) return s;
        var m = s.match(/^(-?[\d.]+)(\s*\(.*)$/);
        return m ? (m[1] + "%" + m[2]) : s;
    }

    function stripWeaponName(skinName, weaponName) {
        if (!skinName) return skinName;
        var withoutWeapon = skinName
            .replace(new RegExp("\\b" + weaponName + "\\b", "ig"), "")
            .replace(/\s+/g, " ")
            .trim();
        return withoutWeapon || skinName;
    }

    function formatTimeAgo(seconds) {
        var s = Math.max(0, Math.floor(Number(seconds) || 0));
        if (s < 60) return s + (s === 1 ? " second" : " seconds");
        if (s < 3600) {
            var m = Math.floor(s / 60);
            return m + (m === 1 ? " minute" : " minutes");
        }
        if (s < 86400) {
            var h = Math.floor(s / 3600);
            return h + (h === 1 ? " hour" : " hours");
        }
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
            var allyRecord = entry.ally_wins + "W-" + entry.ally_losses + "L";
            if (entry.ally_unknown) allyRecord += " (" + entry.ally_unknown + " unknown)";
            parts.push("Ally " + allyRecord);
        }
        if (entry.enemy_count) {
            var enemyRecord = entry.enemy_wins + "W-" + entry.enemy_losses + "L";
            if (entry.enemy_unknown) enemyRecord += " (" + entry.enemy_unknown + " unknown)";
            parts.push("Enemy " + enemyRecord);
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
        showToast._t = setTimeout(function () {
            els.toast.classList.remove("show");
        }, 3200);
    }

    function takeScreenshot() {
        if (typeof html2canvas === "undefined") {
            showToast("Screenshot library not loaded yet.");
            return;
        }
        els.screenshotButton.disabled = true;
        var target = document.querySelector(".teams-layout");
        if (!target) {
            showToast("No teams layout found.");
            els.screenshotButton.disabled = false;
            return;
        }

        var cleanup = [];

        function suppressOverlays() {
            var s = document.createElement("style");
            s.id = "tmp-scr";
            s.textContent =
                "body::before { display: none !important; }" +
                ".player-button { background: rgba(10, 14, 24, 0.92) !important; }" +
                ".player-button::after { display: none !important; }" +
                ".player-button.self-card," +
                ".player-button.is-blue," +
                ".player-button.is-red  { background: rgba(10, 14, 24, 0.92) !important; }" +
                ".player-button.self-card," +
                ".player-button.is-blue," +
                ".player-button.is-red  { box-shadow: 0 18px 40px rgba(0,0,0,0.5) !important; }" +
                ".player-button:hover," +
                ".player-button:focus-visible," +
                ".player-button.is-selected { transform: none !important; }" +
                "* { animation: none !important; }";
            document.head.appendChild(s);
            cleanup.push(function () {
                var el = document.getElementById("tmp-scr");
                if (el) el.remove();
            });
        }

        function hideDetailsPanel() {
            var panel = els.detailsPanel;
            if (!panel.hidden) {
                panel.hidden = true;
                cleanup.push(function () {
                    panel.hidden = false;
                });
            }
        }

        suppressOverlays();
        hideDetailsPanel();
        document.body.classList.add("show-you-badge");

        html2canvas(target, {
            scale: 2,
            useCORS: true,
            backgroundColor: "#0f1115",
        }).then(function (canvas) {
            cleanup.forEach(function (fn) { fn(); });
            document.body.classList.remove("show-you-badge");
            canvas.toBlob(function (blob) {
                if (!blob) {
                    showToast("Screenshot failed.");
                    els.screenshotButton.disabled = false;
                    return;
                }
                if (navigator.clipboard && navigator.clipboard.write) {
                    navigator.clipboard.write([
                        new ClipboardItem({ "image/png": blob })
                    ]).then(function () {
                        els.toast.classList.add("is-success");
                        showToast("Screenshot copied!");
                        setTimeout(function () {
                            els.toast.classList.remove("is-success");
                        }, 600);
                    }).catch(function () {
                        showToast("Copy failed. Try refreshing.");
                    });
                } else {
                    showToast("Clipboard API unavailable.");
                }
                els.screenshotButton.disabled = false;
            }, "image/png");
        }).catch(function () {
            cleanup.forEach(function (fn) { fn(); });
            document.body.classList.remove("show-you-badge");
            showToast("Screenshot failed.");
            els.screenshotButton.disabled = false;
        });
    }

    function setStatus(text, cls) {
        els.statusPill.className = "status-pill " + cls;
        els.statusText.textContent = text;
        updateLoadingOverlay(cls);
    }

    // ---------------------------------------------------------------
    // loading overlay (desktop app only -- plain-browser use of this
    // page has no window.pywebview.api, so it just never shows)
    // ---------------------------------------------------------------
    var loadingLogPollTimer = null;

    function pollLoadingLogs() {
        if (!window.pywebview || !window.pywebview.api || !window.pywebview.api.get_gui_log_tail) {
            return;
        }
        window.pywebview.api.get_gui_log_tail().then(function (text) {
            els.loadingLogTail.textContent = text || "";
            els.loadingLogTail.scrollTop = els.loadingLogTail.scrollHeight;
        }).catch(function () { /* noop -- api not ready yet */ });
    }

    function updateLoadingOverlay(cls) {
        var isLive = cls === "live";
        els.loadingOverlay.hidden = isLive;

        if (isLive && loadingLogPollTimer) {
            clearInterval(loadingLogPollTimer);
            loadingLogPollTimer = null;
            return;
        }
        if (!isLive && !loadingLogPollTimer) {
            pollLoadingLogs();
            loadingLogPollTimer = setInterval(pollLoadingLogs, 1000);
        }
    }

    // ---------------------------------------------------------------
    // websocket
    // ---------------------------------------------------------------

    function sanitizePort(value) {
        var digits = String(value || "").replace(/\D/g, "");
        var parsed = Number(digits);
        if (!parsed || parsed < 1 || parsed > 65535) return DEFAULT_PORT;
        return String(parsed);
    }

    function getPort() {
        // Prefer search ("?port=...") but fall back to hash ("#port=...")
        var raw = window.location.search || (window.location.hash ? ('?' + window.location.hash.replace(/^#/, '')) : '');
        var params = new URLSearchParams(raw);
        return sanitizePort(params.get("port") || DEFAULT_PORT);
    }

    function connect() {
        var port = getPort();
        if (state.socket) {
            try { state.socket.close(); } catch (e) { /* noop */ }
        }

        setStatus("Connecting", "pending");
        var host = window.location.hostname || "127.0.0.1";
        var socket;
        try {
            socket = new WebSocket("ws://" + host + ":" + port + "/");
        } catch (e) {
            setStatus("Connection failed", "error");
            scheduleReconnect();
            return;
        }
        state.socket = socket;

        socket.addEventListener("open", function () {
            setStatus("Connected", "live");
        });
        socket.addEventListener("close", function () {
            setStatus("Disconnected", "error");
            scheduleReconnect();
        });
        socket.addEventListener("error", function () {
            setStatus("Connection failed", "error");
        });
        socket.addEventListener("message", function (event) {
            var payload;
            try {
                payload = JSON.parse(event.data);
            } catch (e) {
                return;
            }
            if (payload.type === "state_change") {
                if (payload.state === state.lastGameState) return;
                renderStateTransition(payload.state);
                return;
            }
            if (!payload || (payload.type && payload.type !== "heartbeat")) {
                return;
            }
            if (payload.players) {
                state.lastGameState = payload.state;
                setPayload(payload, true);
            }
        });
    }

    function scheduleReconnect() {
        if (state.reconnectTimer) return;
        state.reconnectTimer = setTimeout(function () {
            state.reconnectTimer = null;
            connect();
        }, 2500);
    }

    function sendAction(action) {
        if (state.socket && state.socket.readyState === WebSocket.OPEN) {
            state.socket.send(JSON.stringify({ action: action }));
            return true;
        }
        return false;
    }

    // ---------------------------------------------------------------
    // state / rendering
    // ---------------------------------------------------------------

    function payloadRenderKey(payload) {
        // "time" is a fresh timestamp on every single heartbeat, so it's
        // excluded here -- otherwise two heartbeats with identical game
        // state would never compare as equal.
        return JSON.stringify(payload, function (key, value) {
            return key === "time" ? undefined : value;
        });
    }

    function bumpTimestampOnly(payload) {
        if (!payload || !payload.time) return;
        var chip = document.getElementById("metaUpdatedChip");
        if (chip) chip.textContent = "Updated " + new Date(payload.time * 1000).toLocaleTimeString();
    }

    function setPayload(payload, shouldCache) {
        var renderKey = payloadRenderKey(payload);
        var unchanged = renderKey === state.lastRenderKey;

        // Always keep the latest raw payload around (JSON copy /
        // screenshot reads straight from state.payload), even when
        // skipping the render below.
        state.payload = payload;
        state.rankIcons = payload && payload.rankIcons;

        if (shouldCache) {
            try { localStorage.setItem(CACHE_KEY, JSON.stringify(payload)); } catch (e) { /* storage full/unavailable */ }
        }

        if (unchanged) {
            bumpTimestampOnly(payload);
            return;
        }

        state.lastRenderKey = renderKey;
        state.players = normalizePlayers(payload);
        if (!state.players.some(function (p) { return p.puuid === state.selectedPuuid; })) {
            state.selectedPuuid = null;
        }
        render();
    }

    function normalizePlayers(payload) {
        var rawPlayers = (payload && payload.players) || {};
        var myPuuid = payload && payload.puuid;
        return Object.keys(rawPlayers)
            .map(function (puuid) {
                var p = rawPlayers[puuid] || {};
                p.puuid = p.puuid || puuid;
                p.isSelf = !!(myPuuid && p.puuid === myPuuid);
                return p;
            })
            .filter(function (p) { return p.name || p.agent || p.weapons; })
            .sort(function (a, b) {
                var t = teamRank(a.team) - teamRank(b.team);
                if (t !== 0) return t;
                return String(a.name || "").localeCompare(String(b.name || ""));
            });
    }

    function teamRank(team) {
        if (team === "Blue") return 0;
        if (team === "Red") return 1;
        return 2;
    }

    function teamClass(team) {
        if (team === "Blue") return "is-blue";
        if (team === "Red") return "is-red";
        return "";
    }

    function render() {
        renderMeta();
        renderPlayers();
        renderPlayedWith();
        renderDetails();
        renderJson();
    }

    var STATE_LABELS = {
        INGAME: "In-Game",
        PREGAME: "Agent Select",
        MENUS: "In-Menus",
        DISCONNECTED: "Disconnected",
    };

    var STATE_CLASSES = {
        INGAME: "state-ingame",
        PREGAME: "state-pregame",
        MENUS: "state-menus",
        DISCONNECTED: "state-disconnected",
    };

    function renderStateTransition(newState) {
        var label = STATE_LABELS[newState] || newState || "Unknown";
        els.matchMeta.replaceChildren();
        var chip = document.createElement("span");
        chip.className = "meta-chip";
        var spinner = document.createElement("span");
        spinner.className = "meta-spinner";
        chip.append(spinner, document.createTextNode("Loading " + label + " Data\u2026"));
        els.matchMeta.append(chip);
    }

    function renderMeta() {
        var payload = state.payload;
        els.matchMeta.replaceChildren();

        if (!payload) {
            var waitingChip = document.createElement("span");
            waitingChip.className = "meta-chip updated";
            var spinner = document.createElement("span");
            spinner.className = "meta-spinner";
            waitingChip.append(spinner, document.createTextNode("Waiting for data\u2026"));
            els.matchMeta.append(waitingChip);
            return;
        }

        var segments = [];
        segments.push({ text: STATE_LABELS[payload.state] || txt(payload.state, "Unknown"), cls: STATE_CLASSES[payload.state] || "" });

        if (payload.mode) segments.push({ text: payload.mode, cls: "mode" });

        var mapVal = payload.map;
        if (Array.isArray(mapVal)) mapVal = mapVal[0];
        if (mapVal) segments.push({ text: mapVal, cls: "map" });

        if (payload.server) segments.push({ text: payload.server, cls: "server" });

        if (payload.time) {
            var d = new Date(payload.time * 1000);
            segments.push({ text: "Updated " + d.toLocaleTimeString(), cls: "updated", id: "metaUpdatedChip" });
        }

        segments.forEach(function (segment, index) {
            if (index > 0) {
                var sep = document.createElement("span");
                sep.className = "meta-sep";
                sep.textContent = "\u2022";
                els.matchMeta.append(sep);
            }
            appendMetaChip(segment.text, segment.cls, segment.id);
        });
    }

    function buildRefreshButton(compact) {
        var button = document.createElement("button");
        button.type = "button";
        button.className = "refresh-btn" + (compact ? " is-compact" : "");
        button.title = "Force-refresh all data by restarting vRY";
        var icon = document.createElement("span");
        icon.className = "refresh-icon";
        button.append(icon, document.createTextNode("Refresh"));
        button.addEventListener("click", openModal);
        return button;
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

        state.players.forEach(function (player) {
            var button = document.createElement("button");
            button.type = "button";
            button.className = "player-button " + teamClass(player.team);
            button.classList.toggle("self-card", player.isSelf);
            button.classList.toggle("is-selected", player.puuid === state.selectedPuuid);
            button.title = txt(player.name, "Unknown Player") + COPY_HINT;
            button.addEventListener("click", function () {
                state.selectedPuuid = player.puuid;
                render();
            });
            button.addEventListener("contextmenu", function (e) {
                e.preventDefault();
                var text = stripHint(e.target.title || button.title);
                navigator.clipboard.writeText(text).then(function () {
                    showToast('Copied: ' + text);
                }, function () {
                    showToast('Failed to copy.');
                });
            });

            var avatar = buildAgentAvatar(player.agentImgLink, player.agent);

            var identity = document.createElement("div");
            identity.className = "player-main";

            var name = document.createElement("span");
            name.className = "player-name";
            name.textContent = txt(player.name, "Unknown Player");
            name.title = name.textContent + COPY_HINT;

            var youBadge = null;
            if (player.isSelf) {
                youBadge = document.createElement("span");
                youBadge.className = "self-badge";
                youBadge.textContent = "You";
            }

            var agent = document.createElement("span");
            agent.className = "agent-name";
            agent.textContent = txt(player.agent, "Agent " + NA);
            agent.title = agent.textContent;

            var metaRow = document.createElement("span");
            metaRow.className = "player-meta-row";
            var rankBadge = document.createElement("span");
            rankBadge.className = "player-meta";
            var rankIconUrl = state.rankIcons && state.rankIcons[player.rank];
            if (rankIconUrl) {
                var rankImg = document.createElement("img");
                rankImg.className = "rank-icon";
                rankImg.src = rankIconUrl;
                rankImg.loading = "lazy";
                rankBadge.append(rankImg);
                rankBadge.append(" ");
            }
            rankBadge.append(document.createTextNode(rankName(player.rank, true)));
            var badgeColor = rankColor(player.rank);
            if (badgeColor) rankBadge.style.color = badgeColor;
            metaRow.append(rankBadge);

            var action = document.createElement("span");
            action.className = "player-action";
            action.textContent = "View loadout & stats";

            identity.append(name);
            if (youBadge) identity.append(youBadge);
            identity.append(agent, metaRow, action);
            button.append(avatar, identity, buildCardStats(player), buildPreviewRow(player));

            var grid = (player.team === "Red") ? els.redGrid : els.blueGrid;
            grid.append(button);
        });
    }

    function myTeam(payload) {
        if (!payload || !payload.players) return null;
        for (var k in payload.players) {
            if (payload.players[k].isSelf) return payload.players[k].team;
        }
        return null;
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

        var hasBlue = false, hasRed = false;
        var blueHasSelf = false, redHasSelf = false;
        for (var k in payload.players) {
            var p = payload.players[k];
            if (p.team === "Blue") hasBlue = true;
            if (p.team === "Red") hasRed = true;
            if (p.isSelf) {
                if (p.team === "Blue") blueHasSelf = true;
                if (p.team === "Red") redHasSelf = true;
            }
        }

        var singleTeam = (hasBlue && !hasRed) || (!hasBlue && hasRed);
        document.querySelector(".teams-layout").classList.toggle("is-unified", singleTeam);

        if (singleTeam) {
            els.teamDivider.hidden = true;
            if (hasBlue) {
                els.atkSection.hidden = true;
                els.defSection.hidden = false;
                els.defHeader.textContent = (blueHasSelf ? "ALLY" : "ENEMY") + " (DEF)";
                els.defHeader.classList.toggle("is-my-team", blueHasSelf);
            } else {
                els.defSection.hidden = true;
                els.atkSection.hidden = false;
                els.atkHeader.textContent = (redHasSelf ? "ALLY" : "ENEMY") + " (ATK)";
                els.atkHeader.classList.toggle("is-my-team", redHasSelf);
            }
        } else {
            els.defSection.hidden = false;
            els.atkSection.hidden = false;
            els.teamDivider.hidden = false;
            els.defHeader.textContent = (blueHasSelf ? "ALLY" : "ENEMY") + " (DEF)";
            els.atkHeader.textContent = (redHasSelf ? "ALLY" : "ENEMY") + " (ATK)";
            els.defHeader.classList.toggle("is-my-team", blueHasSelf);
            els.atkHeader.classList.toggle("is-my-team", redHasSelf);
        }
    }

    function buildPreviewRow(player) {
        var row = document.createElement("div");
        row.className = "preview-row";

        PREVIEW_WEAPONS.forEach(function (weaponName) {
            var weapon = getWeapon(player, weaponName);
            var slot = document.createElement("span");
            slot.className = "preview-slot";

            if (weapon && weapon.skinDisplayIcon) {
                var img = document.createElement("img");
                img.src = weapon.skinDisplayIcon;
                img.alt = weapon.skinDisplayName || weapon.weapon || "Weapon";
                slot.append(img);
            }

            var copy = document.createElement("span");
            copy.className = "preview-copy";

            var label = document.createElement("span");
            label.className = "preview-label";
            label.textContent = weaponName;

            var name = document.createElement("span");
            name.className = "preview-name";
            var skinLabel = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
            if (weapon && weapon.skinDisplayName && weaponName !== "Melee") {
                skinLabel = stripWeaponName(skinLabel, weaponName);
            }
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

        if (!hasSelection && state.players.length === 0) {
            els.emptyStateTitle.textContent = "No match loadout cached";
            els.emptyStateText.textContent = "Waiting for vRY websocket data on localhost.";
        } else if (!hasSelection) {
            els.emptyStateTitle.textContent = "No player selected";
            els.emptyStateText.textContent = "Click a player card to view loadout and stats.";
        }

        if (!selected) return;

        els.selectedAgent.src = selected.agentImgLink || "";
        els.selectedAgent.hidden = !selected.agentImgLink;
        els.selectedAgent.alt = selected.agent || "";
        els.selectedName.textContent = txt(selected.name, "Unknown Player");

        // Update external profile links
        var hasName = selected.name && selected.name.indexOf("#") !== -1;
        if (hasName) {
            var trnHref = "https://tracker.gg/valorant/profile/riot/" + encodeURIComponent(selected.name) + "/overview";
            var vtlHref = "https://vtl.lol/id/" + encodeURIComponent(selected.name.replace("#", "_"));
            els.trnLink.href = trnHref;
            els.trnLink.title = trnHref + COPY_HINT;
            els.trnLink.hidden = false;
            els.vtlLink.href = vtlHref;
            els.vtlLink.title = vtlHref + COPY_HINT;
            els.vtlLink.hidden = false;
        } else {
            els.trnLink.hidden = true;
            els.vtlLink.hidden = true;
        }

        els.selectedCardTitle.textContent = selected.title || "";
        els.selectedCardTitle.hidden = !selected.title;
        els.selectedModalName.textContent = txt(selected.agent, "Agent " + NA);
        els.selectedLevel.textContent = isEmpty(selected.level) ? "Level " + NA : ("Level " + selected.level);
        els.selectedTeam.textContent = selected.team ? ((myTeam(state.payload) === selected.team) ? "ALLY" : "ENEMY") : "Unknown";
        els.selectedTeam.className = "team-pill " + teamClass(selected.team);

        if (selected.playerCard) {
            els.playerCardPreview.style.backgroundImage = "url(\"" + selected.playerCard + "\")";
        } else {
            els.playerCardPreview.style.backgroundImage = "";
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
        var expressions = Object.keys(player.sprays || {})
            .map(function (index) {
                var e = player.sprays[index] || {};
                return Object.assign({ index: Number(index) }, e);
            })
            .sort(function (a, b) { return a.index - b.index; });

        var slots = expressions.slice(0, 4);
        while (slots.length < 4) slots.push(null);

        slots.forEach(function (expression, index) {
            var tile = document.createElement("div");
            tile.className = "expression-tile expression-slot-" + index;
            tile.title = (expression ? (expression.displayName || "Expression") : ("Empty slot " + (index + 1))) + COPY_HINT;
            tile.setAttribute("aria-label", tile.title);
            if (expression && expression.type === "flex") tile.classList.add("is-flex");

            var art = document.createElement("div");
            art.className = "expression-art";
            var iconSrc = expression && (expression.fullTransparentIcon || expression.displayIcon);
            if (iconSrc) {
                var img = document.createElement("img");
                img.src = iconSrc;
                img.alt = (expression && expression.displayName) || "Expression";
                art.append(img);
            }

            var copy = document.createElement("div");
            copy.className = "expression-copy";

            var name = document.createElement("strong");
            name.textContent = expression ? (expression.displayName || NA) : ("Slot " + (index + 1));

            var type = document.createElement("span");
            type.className = "expression-type";
            type.textContent = expression ? (expression.type || "expression") : "empty";

            copy.append(name, type);
            tile.append(art, copy);
            els.expressionGrid.append(tile);

            tile.addEventListener("contextmenu", function (e) {
                e.preventDefault();
                var text = stripHint(e.target.title || tile.title);
                navigator.clipboard.writeText(text).then(function () {
                    showToast("Copied: " + text);
                }, function () {
                    showToast("Failed to copy.");
                });
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

                group.weapons.forEach(function (weaponName) {
                    grid.append(buildWeaponTile(player, weaponName));
                });

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
        var varColor = chromaColor(weapon && weapon.chromaDisplayName);
        varColor = varColor ? " (" + varColor + ")" : "";
        tile.title = (weapon
            ? (weaponName + ": " + (weapon.skinDisplayName || weapon.weapon || "Unknown skin") + varColor)
            : (weaponName + ": " + NA)) + COPY_HINT;

        var art = document.createElement("div");
        art.className = "weapon-art";
        var iconSrc = weapon && (weapon.skinDisplayIcon || weapon.weaponDisplayIcon);
        if (iconSrc) {
            var img = document.createElement("img");
            img.src = iconSrc;
            img.alt = weapon.skinDisplayName || weapon.weapon || weaponName;
            art.append(img);
        }

        var copy = document.createElement("div");
        copy.className = "weapon-copy";

        var label = document.createElement("span");
        label.className = "weapon-name";
        label.textContent = weaponName;

        var name = document.createElement("strong");
        name.textContent = weapon ? (weapon.skinDisplayName || weapon.weapon || weaponName) : NA;
        name.title = name.textContent + varColor + COPY_HINT;

        copy.append(label, name);
        tile.append(art, copy);

        tile.addEventListener("contextmenu", function (e) {
            e.preventDefault();
            var text = stripHint(e.target.title || tile.title);
            navigator.clipboard.writeText(text).then(function () {
                showToast("Copied: " + text);
            }, function () {
                showToast("Failed to copy.");
            });
        });

        if (weapon && weapon.buddy_displayIcon) {
            tile.classList.add("has-buddy");
            var buddy = document.createElement("img");
            buddy.className = "buddy";
            buddy.src = weapon.buddy_displayIcon;
            buddy.alt = weapon.buddy_displayName || "Buddy";
            buddy.title = (weapon.buddy_displayName || "Buddy") + COPY_HINT;
            buddy.addEventListener("contextmenu", function (e) {
                e.stopPropagation();
                e.preventDefault();
                var text = stripHint(e.target.title || buddy.title);
                navigator.clipboard.writeText(text).then(function () {
                    showToast("Copied: " + text);
                }, function () {
                    showToast("Failed to copy.");
                });
            });
            tile.append(buddy);
        }

        return tile;
    }

    function getWeapon(player, weaponName) {
        var weapons = player.weapons || {};
        var match = null;
        Object.keys(weapons).forEach(function (key) {
            var w = weapons[key];
            if (w && w.weapon === weaponName) match = w;
        });
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

    function renderPlayedWith() {
        var payload = state.payload;
        var entries = (payload && Array.isArray(payload.alreadyPlayedWith)) ? payload.alreadyPlayedWith : [];

        els.playedWithEmpty.hidden = entries.length > 0;
        els.playedWithTable.hidden = entries.length === 0;
        if (!entries.length) return;

        els.playedWithBody.replaceChildren();
        entries.forEach(function (entry) {
            var row = document.createElement("tr");

            var nameCell = document.createElement("td");
            nameCell.textContent = txt(entry.name, "Unknown");
            row.append(nameCell);

            var timesCell = document.createElement("td");
            timesCell.textContent = formatEncounterTimes(entry);
            row.append(timesCell);

            var lastSeenCell = document.createElement("td");
            lastSeenCell.textContent = txt(entry.relation_name, "player") + " " + txt(entry.agent, "Unknown") +
                " on " + txt(entry.map, "Unknown") + " \u2014 " + formatTimeAgo(entry.time_diff) + " ago";
            row.append(lastSeenCell);

            var recordCell = document.createElement("td");
            recordCell.textContent = formatEncounterRecord(entry);
            row.append(recordCell);

            els.playedWithBody.append(row);
        });
    }

    function renderJson() {
        var text = state.payload ? JSON.stringify(state.payload, null, 2) : "No data yet.";
        els.jsonPre.textContent = text;
        els.jsonPreEmpty.textContent = text;
    }

    function copyJson(button) {
        var text = state.payload ? JSON.stringify(state.payload, null, 2) : "";
        if (!text) {
            showToast("Nothing to copy yet.");
            return;
        }
        var done = function () {
            var original = button.textContent;
            button.textContent = "Copied!";
            button.classList.add("copied");
            setTimeout(function () {
                button.textContent = original;
                button.classList.remove("copied");
            }, 1500);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).then(done).catch(function () { fallbackCopy(text, done); });
        } else {
            fallbackCopy(text, done);
        }
    }

    function copyText(text, button, label) {
        var done = function () {
            var original = button.textContent;
            button.textContent = label || "\u2713";
            button.classList.add("copied");
            setTimeout(function () {
                button.textContent = original;
                button.classList.remove("copied");
            }, 1500);
        };
        if (!text) { showToast("Nothing to copy."); return; }
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).then(done).catch(function () { fallbackCopy(text, done); });
        } else {
            fallbackCopy(text, done);
        }
    }

    function fallbackCopy(text, done) {
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.append(ta);
        ta.select();
        try { document.execCommand("copy"); done(); } catch (e) { showToast("Copy failed - select manually."); }
        ta.remove();
    }

    // ---------------------------------------------------------------
    // refresh / restart flow
    // ---------------------------------------------------------------

    function openModal() { els.confirmModal.hidden = false; }
    function closeModal() { els.confirmModal.hidden = true; }

    function setRefreshButtonsBusy(busy) {
        document.querySelectorAll(".refresh-btn").forEach(function (button) {
            button.disabled = busy;
            button.classList.toggle("is-spinning", busy);
        });
    }

    function requestRestart() {
        closeModal();
        setRefreshButtonsBusy(true);
        try { localStorage.removeItem(CACHE_KEY); } catch (e) { /* noop */ }
        state.payload = null;
        state.players = [];
        state.lastRenderKey = null;
        state.selectedPuuid = null;
        render();

        var sent = sendAction("restart_application");
        if (sent) {
            showToast("Restart requested \u2014 vRY is reloading data now.");
        } else {
            showToast("Not connected yet \u2014 reconnecting and retrying\u2026");
            connect();
            setTimeout(function () {
                if (!sendAction("restart_application")) {
                    showToast("Still couldn't reach vRY. Check that the app is running.");
                }
            }, 1200);
        }

        setTimeout(function () {
            setRefreshButtonsBusy(false);
        }, 4000);
    }

    // ---------------------------------------------------------------
    // wiring
    // ---------------------------------------------------------------

    els.closeDetailsButton.addEventListener("click", function () {
        state.selectedPuuid = null;
        render();
    });
    els.detailsPanel.addEventListener("click", function (event) {
        if (event.target === els.detailsPanel) {
            state.selectedPuuid = null;
            render();
        }
    });
    window.addEventListener("keydown", function (event) {
        if (event.key === "Escape") {
            if (!els.confirmModal.hidden) { closeModal(); return; }
            if (state.selectedPuuid) { state.selectedPuuid = null; render(); }
        }
    });

    els.refreshButton.addEventListener("click", openModal);
    els.loadingRefreshButton.addEventListener("click", openModal);
    els.modalCancel.addEventListener("click", closeModal);
    els.modalConfirm.addEventListener("click", requestRestart);
    els.confirmModal.addEventListener("click", function (event) {
        if (event.target === els.confirmModal) closeModal();
    });

    els.jsonCopyBtn.addEventListener("click", function () { copyJson(els.jsonCopyBtn); });
    els.jsonCopyBtnEmpty.addEventListener("click", function () { copyJson(els.jsonCopyBtnEmpty); });
    els.trnLink.addEventListener("contextmenu", function (e) {
        e.preventDefault();
        var text = stripHint(this.title);
        navigator.clipboard.writeText(text).then(function () {
            showToast("Copied: " + text);
        }, function () {
            showToast("Failed to copy.");
        });
    });
    els.vtlLink.addEventListener("contextmenu", function (e) {
        e.preventDefault();
        var text = stripHint(this.title);
        navigator.clipboard.writeText(text).then(function () {
            showToast("Copied: " + text);
        }, function () {
            showToast("Failed to copy.");
        });
    });

    els.screenshotButton.addEventListener("click", takeScreenshot);

    // ---- boot ----
    var cached = null;
    try { cached = localStorage.getItem(CACHE_KEY); } catch (e) { /* noop */ }
    if (cached) {
        try { setPayload(JSON.parse(cached), false); } catch (e) {
            try { localStorage.removeItem(CACHE_KEY); } catch (e2) { /* noop */ }
        }
    } else {
        render();
    }

    connect();
})();
