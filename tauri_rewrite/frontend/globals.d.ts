// Ambient type declarations for the VRY Tauri frontend.
// These describe APIs injected by the Tauri runtime and browser/V8 extensions.

interface TauriCore {
  invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}
interface TauriEvent {
  listen(event: string, handler: (event: TauriEventPayload) => void): Promise<() => void>;
}
interface TauriEventPayload {
  payload: Record<string, unknown>;
  id: number;
}
interface Window {
  __TAURI__?: {
    core?: TauriCore & { event?: TauriEvent };
    invoke?: TauriCore["invoke"];
    event?: TauriEvent;
    listen?: TauriEvent["listen"];
  };
}

declare function gc(): void;

// Element reference map — mirrors the `els` object in vry_gui.js.
// Each key maps to the specific element type used at runtime so property
// accesses (src, href, disabled, open, etc.) resolve without casting.
// The `els` object is built from getElementById() calls whose return type
// is HTMLElement | null, but every id referenced in the template exists.
// We type properties as non-nullable and use a JSDoc type assertion
// (/** @type {ElsType} */) at the assignment site to override the
// narrower runtime return type.
interface ElsType {
  blueGrid: HTMLDivElement;
  redGrid: HTMLDivElement;
  detailsPanel: HTMLDivElement;
  emptyState: HTMLDivElement;
  emptyStateTitle: HTMLSpanElement;
  emptyStateText: HTMLSpanElement;
  selectedAgent: HTMLImageElement;
  selectedTeam: HTMLSpanElement;
  selectedName: HTMLSpanElement;
  trnLink: HTMLAnchorElement;
  vtlLink: HTMLAnchorElement;
  topTrnLink: HTMLAnchorElement & { disabled?: boolean };
  topVtlLink: HTMLAnchorElement & { disabled?: boolean };
  copyStatsBtn: HTMLButtonElement;
  screenshotPlayerRow: HTMLDivElement;
  screenshotPlayerBtn: HTMLButtonElement;
  selectedCardTitle: HTMLHeadingElement;
  selectedModalName: HTMLSpanElement;
  selectedLevel: HTMLSpanElement;
  playerCardPreview: HTMLDivElement;
  expressionGrid: HTMLDivElement;
  weaponGroups: HTMLDivElement;
  closeDetailsButton: HTMLButtonElement;
  statBar: HTMLDivElement;
  statusPill: HTMLSpanElement;
  statusText: HTMLSpanElement;
  loadingOverlay: HTMLDivElement;
  loadingRefreshButton: HTMLButtonElement;
  loadingLogTail: HTMLPreElement;
  matchMeta: HTMLDivElement;
  refreshButton: HTMLButtonElement;
  confirmModal: HTMLDivElement;
  modalCancel: HTMLButtonElement;
  modalConfirm: HTMLButtonElement;
  toast: HTMLDivElement;
  jsonPre: HTMLPreElement;
  jsonCopyBtn: HTMLButtonElement;
  defHeader: HTMLHeadingElement;
  atkHeader: HTMLHeadingElement;
  defSection: HTMLDivElement;
  atkSection: HTMLDivElement;
  teamDivider: HTMLDivElement;
  screenshotButton: HTMLButtonElement;
  playedWithEmpty: HTMLDivElement;
  playedWithPanel: HTMLDetailsElement;
  playedWithTable: HTMLTableElement;
  playedWithBody: HTMLTableSectionElement;
  logPanel: HTMLDetailsElement;
  logPre: HTMLPreElement;
  logCopyBtn: HTMLButtonElement;
  hbPanel: HTMLDetailsElement;
  hbPre: HTMLPreElement;
  hbCopyBtn: HTMLButtonElement;
  hbRefreshBtn: HTMLButtonElement;
  teamsLayout: HTMLDivElement;
  metaUpdatedChip: HTMLSpanElement;
}

// Player data types used in heartbeat payloads
interface PlayerData {
  puuid: string;
  isSelf: boolean;
  _realName?: string;
  _weaponMap?: Record<string, WeaponEntry>;
  name?: string;
  agent?: string;
  agentImgLink?: string;
  agentSelectionState?: string;
  team?: string;
  rank?: number | string;
  rr?: number | string;
  leaderboard?: number | string;
  peakRank?: number | string;
  peakRankAct?: string;
  previousRank?: number | string;
  level?: number | string;
  lastActive?: string;
  winPercentage?: string;
  partyNumber?: number | string;
  title?: string;
  playerCard?: string;
  playerCardName?: string;
  weapons?: Record<string, WeaponEntry>;
  sprays?: Record<string, SprayEntry>;
  alreadyPlayedWith?: EncounterEntry[];
}

interface WeaponEntry {
  weapon?: string;
  skinDisplayName?: string;
  skinDisplayIcon?: string;
  chromaDisplayName?: string;
  contentTierColor?: string;
  contentTierName?: string;
  contentTierIcon?: string;
  weaponDisplayIcon?: string;
  buddy_displayIcon?: string;
  buddy_displayName?: string;
}

interface SprayEntry {
  displayName?: string;
  displayIcon?: string;
  fullTransparentIcon?: string;
  type?: string;
  index?: number;
}

interface EncounterEntry {
  name?: string;
  agent?: string;
  relation?: string;
  relation_name?: string;
  times?: number | string;
  time_diff?: number | string;
  lastAgent?: string;
  lastMap?: string;
  map?: string;
  ally_count?: number;
  enemy_count?: number;
  ally_wins?: number;
  ally_losses?: number;
  ally_unknown?: number;
  enemy_wins?: number;
  enemy_losses?: number;
  enemy_unknown?: number;
}

// Application state shape — mirrors the `state` object in vry_gui.js.
interface StateType {
  payload: HeartbeatPayload | null;
  players: PlayerData[];
  selectedPuuid: string | null;
  matchPuuid: string | undefined;
  lastRenderKey: number | null;
  epoch: number | null;
  rankIcons: Record<string, string> | null;
  lastGameState: string | null;
  prevGameState: string | null;
  playerButtons: Map<string, HTMLButtonElement>;
  dirty: { meta: boolean; players: boolean; playedWith: boolean; details: boolean; json: boolean };
}

interface HeartbeatPayload {
  sessionId?: number;
  version?: number;
  state?: string;
  mode?: string;
  map?: string | string[];
  server?: string;
  time?: number;
  puuid?: string;
  matchId?: string;
  players?: Record<string, PlayerData>;
  alreadyPlayedWith?: EncounterEntry[];
  rankIcons?: Record<string, string>;
}
