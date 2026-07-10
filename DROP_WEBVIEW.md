# Architectural Plan: Drop WebView2

**Goal:** Replace the Tauri/WebView2 frontend (Chromium) with a native Rust GUI to eliminate ~80-100MB memory overhead from WebView2.

**Current memory profile:** ~100-150MB total (WebView2 = ~80-100MB)  
**Target memory profile:** ~20-40MB total (native rendering)

---

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Tauri Application                      │
│                                                              │
│  ┌──────────────────────┐   ┌────────────────────────────┐  │
│  │    Rust Backend      │   │   WebView2 (Chromium)      │  │
│  │                      │   │                            │  │
│  │  state_machine.rs    │───▶  index.html                │  │
│  │  payload_builder.rs  │   │  vry_gui.js (853 lines)    │  │
│  │  services/*.rs       │   │  vry_gui.css (733 lines)   │  │
│  │  models/*.rs         │   │  style.css (886 lines)     │  │
│  └──────────────────────┘   └────────────────────────────┘  │
│                                                              │
│  Bundle: ~100MB+                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Target Architecture

```
┌────────────────────────────────────────────────────────────┐
│                  Native Rust GUI Application                │
│                                                            │
│  ┌──────────────────────┐   ┌───────────────────────────┐  │
│  │    Rust Backend      │   │   egui / eframe           │  │
│  │    (unchanged)       │───▶                           │  │
│  │                      │   │  setup() + render() loop  │  │
│  │  state_machine.rs    │   │  custom widgets           │  │
│  │  payload_builder.rs  │   │  async texture loading    │  │
│  │  services/*.rs       │   │  clipboard (arboard)      │  │
│  │  models/*.rs         │   │  screenshot (pixel buf)   │  │
│  └──────────────────────┘   └───────────────────────────┘  │
│                                                            │
│  Bundle: ~8-15MB (LTO + strip)                             │
└────────────────────────────────────────────────────────────┘
```

---

## Framework Recommendation: `egui` / `eframe`

| Framework | Style | Pros | Cons |
|-----------|-------|------|------|
| **egui** (eframe) | Immediate mode | Fastest to port (re-render pattern matches our heartbeat loop), GPU-accelerated, hot-reload, built-in texture loading, excellent docs | Not "native" looking; manual layout |
| **iced** | Elm-Architecture | Native look, widget-based | Heavier, more boilerplate, less mature image support |
| **slint** | Declarative markup | Good perf, built-in image loading | Newer, binding overhead |

**Why egui:**
- Our rendering model (full re-render on each heartbeat) maps perfectly to immediate mode
- Built-in texture loading for async images (agent icons, weapon skins, player cards)
- `egui-winit` + `egui-wgpu` for GPU-accelerated rendering
- `arboard` for clipboard (text + image)
- Active community, well-maintained

---

## Migration Phases

### Phase 1: Scaffold + Player Grid (Week 1)

**Goal:** Replace the player roster grid (the main view).

**New dependencies:**
```toml
eframe = "0.27"
egui = "0.27"
egui_extras = "0.27"  # for image loading
arboard = "3.3"       # clipboard
reqwest = { version = "0.12", features = ["json"] }
image = "0.25"        # decode images before GPU upload
```

**Work items:**

1. **Create shell app**
   - New `vry-rust` crate or feature-gate in existing
   - `eframe::NativeOptions` with `always_on_top`, `decorated`, `min_window_size`
   - Window title: "VRY - Match Loadouts"

2. **Port data ingestion**
   - Receive heartbeat data via `std::sync::mpsc::Receiver<HeartbeatPayload>`
   - Backend pushes to channel in `state_machine.rs`
   - Frontend consumes in `update()` method

3. **Port player grid** (`renderPlayers` equivalent)
   - **Layout:** Two columns (Blue/RED teams) or single column for MENUS
   - **Player card:** egui `Frame` with rounded corners, team-colored border
   - **Agent avatar:** Async download → decode → `egui::TextureId` cache
   - **Text:** Player name, agent name, rank name
   - **Rank icon:** Same texture loading pattern
   - **Preview row:** 3 small weapon previews (Vandal, Phantom, Melee)
   - **Stat chips:** 8 small grid chips below the preview row

4. **Port interaction**
   - Click → store `selected_puuid` → trigger detail panel render
   - Right-click → egui `response.context_menu()` → "Copy name"
   - Self card highlight: gold border

```rust
// Player grid rendering sketch
fn render_player_grid(ui: &mut Ui, players: &[PlayerHeartbeat]) {
    egui::Grid::new("player_grid")
        .min_col_width(210.0)
        .max_col_width(240.0)
        .show(ui, |ui| {
            for player in players {
                let response = ui.add(
                    Frame::none()
                        .fill(if player.is_self { GOLD_BG } else { PANEL_BG })
                        .rounding(12.0)
                        .show(ui, |ui| {
                            // Agent avatar (texture)
                            // Player name + agent
                            // Stat grid
                            // Preview row
                        }).response;
                if response.clicked() {
                    self.selected_puuid = Some(player.puuid.clone());
                }
                response.context_menu(|ui| {
                    if ui.button("Copy name").clicked() {
                        let _ = Clipboard::new().map(|mut c| c.set_text(&player.name));
                        ui.close_menu();
                    }
                });
            }
        });
}
```

**Milestone:** Player grid with agent avatars, rank icons, stat chips, right-click copy. Click selects player.

---

### Phase 2: Details Panel (Week 2)

**Goal:** Full player detail view matching the current webview.

**Work items:**

1. **Stat bar** (8 chips): `egui::Frame` per stat with label + value, optional rank icon

2. **Weapon inventory** (4 columns):
   - Sidearms, SMGs/Shotguns, Rifles/Melee, Sniper/MG
   - Each tile: weapon icon texture + weapon name label + skin name
   - Chroma color extracted and shown inline
   - Buddy icon overlaid on tile
   - Right-click: copy "WeaponName: SkinName (ChromaColor)"

3. **Expression wheel** (4-slot radial):
   - Crosshair lines in a circular grid
   - 4 tiles positioned at top/right/bottom/left
   - Hover shows tooltip with name + type
   - `is-flex` green glow

4. **Player card preview**:
   - Polygon clip shape (hexagon-like)
   - Async load player card image
   - Level badge, name bar (gold), title text
   - Right-click: copy card name

5. **Detail panel layout:**
   - Overlay (semi-transparent background) when a player is selected
   - Details window: weapons left, player card + expressions right
   - Close button (X)
   - TRN/VTL links

**Milestone:** Full details panel with weapons, expressions, player card, stat bar.

---

### Phase 3: Remaining Views (Week 3)

**Work items:**

1. **"Played with" table**
   - Collapsible panel (egui `CollapsingHeader`)
   - Columns: Name, Times (ally/enemy), Last Seen (relation + agent + map + time ago), Record (W-L)

2. **JSON panel**
   - `CollapsingHeader` with scrollable text
   - Copy button

3. **Top bar + match meta**
   - State/mode/map/server/time chips with colored styling
   - Status pill (live/pending/error)
   - Screenshot button
   - Refresh button

4. **State transition overlay**
   - Loading spinner when state changes
   - "Loading [State] Data..."

5. **Screenshot**
   - Render current teams layout to offscreen `egui::PaintCallback`
   - Extract pixel buffer → encode as PNG
   - Write to clipboard via `arboard`

6. **Confirm modal + restart**
   - Modal backdrop with cancel/confirm buttons
   - Calls `restart_application` backend command

7. **Toast notifications**
   - Fixed bottom-right, slide-up animation
   - Auto-dismiss after 3.2s

**Milestone:** Feature parity with current webview.

---

### Phase 4: Polish + Styling (Week 4)

**Goal:** Match the current CSS visual design.

| Webview (CSS) | egui Equivalent |
|----------------|-----------------|
| `display: flex;` | `ui.horizontal()` / `ui.vertical()` |
| `display: grid;` | Custom layout or `egui::Grid` |
| `clip-path: polygon(...)` | `Shape::path()` with clipping |
| `position: absolute;` | `ui.put()` with `Rect` |
| `background-image` | `Image` widget + `Frame::fill()` |
| `::before` / `::after` | Manual `Shape::rect_filled()` / `Shape::line_segment()` |
| `@keyframes shimmer` | `ctx.request_repaint_after()` + interpolated color |
| `:hover` | `response.on_hover_ui()` / `response.on_hover_text()` |
| `transition: all 0.2s` | Manual interpolation across frames |
| `[hidden] { display: none }` | Conditional `ui.add_visible()` |
| `border-radius`, `box-shadow` | `Frame::rounding()`, `Frame::shadow()` |

**Color palette (from CSS variables):**
```rust
const BG: Color32 = Color32::from_rgb(15, 17, 21);
const PANEL: Color32 = Color32::from_rgb(26, 29, 36);
const GOLD: Color32 = Color32::from_rgb(221, 179, 81);
const BLUE: Color32 = Color32::from_rgb(76, 151, 237);
const RED: Color32 = Color32::from_rgb(238, 77, 77);
const TEXT: Color32 = Color32::from_rgb(231, 231, 231);
const MUTED: Color32 = Color32::from_rgb(102, 102, 102);
```

**Milestone:** Visual parity with webview.

---

### Phase 5: Release Prep (Week 5)

1. **Build optimization:**
   ```bash
   cargo build --release
   strip target/release/vry-rust.exe
   upx --best target/release/vry-rust.exe  # optional
   ```

2. **LTO in Cargo.toml:**
   ```toml
   [profile.release]
   lto = true
   codegen-units = 1
   panic = "abort"
   strip = true
   ```

3. **Testing matrix:**
   - Full cycle: MENUS → PREGAME → INGAME → MENUS
   - All payload fields present and correctly formatted
   - Image loading for all asset types
   - Right-click copy on all elements
   - Screenshot output matches webview version
   - Memory: verify < 40MB

4. **Remove Tauri dependency:**
   - Delete `tauri_rewrite/` Tauri config
   - Remove Tauri crate from `Cargo.toml`
   - Rename crate as needed

**Milestone:** Standalone ~10MB .exe with no WebView2 dependency.

---

## Key Technical Challenges

### 1. Async Image Loading

**Problem:** 50+ concurrent image URLs (agent icons, weapon skins, player cards, rank icons, expression icons, buddy icons) need async download + decode + GPU upload.

**Solution:**
```rust
struct ImageCache {
    textures: HashMap<String, egui::TextureId>,
    pending: Vec<(String, Arc<Mutex<Option<egui::TextureId>>>)>,
    runtime: Handle,
}

impl ImageCache {
    fn get_or_load(&mut self, url: &str, ctx: &egui::Context) -> Option<egui::TextureId> {
        if let Some(id) = self.textures.get(url) {
            return Some(*id);
        }
        let url = url.to_owned();
        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();
        self.runtime.spawn(async move {
            let bytes = reqwest::get(&url).await.ok()?.bytes().await.ok()?;
            let img = image::load_from_memory(&bytes).ok()?;
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let pixels = rgba.into_vec();
            *result_clone.lock().unwrap() = Some((pixels, w, h));
            // ctx.set_texture() needs to happen on UI thread
        });
        self.pending.push((url, result));
        None // Returns None while loading; caller shows placeholder
    }

    fn process_pending(&mut self, ctx: &egui::Context) {
        let mut ready = Vec::new();
        for (url, result) in &self.pending {
            if let Some((pixels, w, h)) = result.lock().unwrap().take() {
                let id = ctx.load_texture(url, egui::ColorImage::from_rgba_unmultiplied([w as _, h as _], &pixels));
                self.textures.insert(url.clone(), id);
                ready.push(url.clone());
            }
        }
        self.pending.retain(|(url, _)| !ready.contains(url));
    }
}
```

### 2. Layout Equivalents

| CSS Layout | egui Strategy |
|------------|--------------|
| 4-column weapon grid | `egui::Grid` with 4 columns + custom width |
| Radial expression wheel | Manual `Pos2` positioning via `ui.put()` |
| Polygon clip on card | `Shape::path()` with custom vertices |
| Overlay details panel | `egui::Area::new("details").show()` with `fixed_pos` |
| Stat bar (flex wrap) | `ui.horizontal_wrapped()` |
| Player button grid | `egui::Grid` or manual column layout |
| Shimmer animation | `ctx.request_repaint_after(Duration::from_millis(30))` |

### 3. Screenshot (Replaces html2canvas)

```rust
fn take_screenshot(ui: &egui::Ui, ctx: &egui::Context) {
    // Render current frame to offscreen buffer
    // This requires using egui's PaintCallback or a secondary render pass
    
    // Alternative: use the native window's framebuffer
    // For eframe/wgpu, read the swapchain texture
    
    // Encode as PNG
    let png_bytes = encode_to_png(&pixels, width, height);
    
    // Copy to clipboard
    let mut cb = arboard::Clipboard::new().unwrap();
    cb.set_image(arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: std::borrow::Cow::Borrowed(&png_bytes),
    }).unwrap();
}
```

### 4. Memory Management

| Asset | Count | Resolution | Memory (est.) |
|-------|-------|-----------|--------------|
| Agent icons | 22 | 256×256 RGBA | ~5.5MB |
| Weapon skins | 20 | 512×512 RGBA | ~40MB (LRU: keep ~5) |
| Player cards | 10 | 1024×512 RGBA | ~20MB (LRU: keep ~2) |
| Rank icons | 27 | 128×128 RGBA | ~1.7MB |
| Expression icons | 40 | 256×256 RGBA | ~10MB (LRU: keep ~4) |
| Total (all loaded) | ~120 | — | ~77MB |
| Total (LRU, ~20) | ~20 | — | ~15MB |

Use an LRU cache (`lru` crate) to limit texture memory to ~30MB max.

---

## Porting Complexity Summary

| Component | JS/CSS Lines | egui Lines (est.) | Difficulty |
|-----------|-------------|-------------------|-----------|
| Topbar + status pill + meta | 50 | 60 | Easy |
| Player grid (compact roster) | 250 | 300 | Medium (image loading) |
| Team headers + team logic | 80 | 60 | Easy |
| Details panel layout | 100 | 120 | Easy |
| Stat bar (8 chips) | 80 | 100 | Easy |
| Weapon groups (4 columns) | 200 | 350 | Medium (grid + icons) |
| Weapon tiles (per-weapon) | 100 | 150 | Medium (buddy overlay) |
| Expression wheel (radial) | 150 | 200 | Medium (circular layout) |
| Player card preview | 100 | 120 | Medium (clip + images) |
| "Played with" table | 80 | 80 | Easy |
| JSON panel | 60 | 60 | Easy |
| Context menus | 60 | 80 | Medium |
| Screenshot | 60 | 100 | Medium (pixel buf → clipboard) |
| Loading overlay | 60 | 80 | Easy |
| Confirm modal | 60 | 60 | Easy |
| Toast notifications | 40 | 50 | Easy |
| **Total** | ~1530 | ~1970 | **3–5 weeks** |

---

## Minimal Viable Alternative (1 Week)

If full feature parity isn't needed immediately:

- **Phase 1 only** (player grid + select + basic details)
- No expression wheel
- No weapon detail view
- No "Played with" table
- No screenshot
- Minimal styling (dark theme + team colors only)
- Essential right-click copy (name + title + card)
- Still eliminates WebView2, drops memory to ~20-30MB

```
cargo build --release  # ~8MB binary
# vs current: ~100MB Tauri bundle
```
