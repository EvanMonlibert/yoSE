//! Point d'entrée principal de **yoSE** (yo Search Engine).
//!
//! Architecture & Optimisations Cache :
//! - **Pool de Cache LRU des WebViews (`webview_cache`)** :
//!   Les onglets visités sont mis en cache mémoire et gelés (`__yoseFreeze`, `set_visible(false)`)
//!   au lieu d'être détruits et rechargés à chaque clic. Le retour sur un onglet en cache
//!   est **instantané (0 ms, 0 requête réseau)**.
//! - **Éviction LRU automatique & Purge à la demande** :
//!   Au-delà de `MAX_HOT_CACHED_TABS` (ou via le bouton « ❄ Purger » / clic droit sur un groupe),
//!   l'onglet le plus ancien est déchargé en veille disque SQLite (`hibernated`, 0 Mo RAM).
//! - **Réutilisation d'instance (`wv.load_url`)** lors de la navigation au sein d'un même onglet.
//! - **En-têtes HTTP `Cache-Control: immutable`** sur le protocole `yose://` pour les logos.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod ui;

use db::{Database, TabStatus};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use tao::{
    dpi::{LogicalPosition, LogicalSize, Position, Size},
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Icon, Theme, Window, WindowBuilder},
};
use wry::{
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        Response, StatusCode,
    },
    Rect, WebView, WebViewBuilder,
};

/// Nombre maximal d'onglets gardés simultanément chauds dans le cache RAM LRU.
/// Au-delà, l'onglet le moins récemment consulté passe automatiquement en veille disque (`hibernated`).
const MAX_HOT_CACHED_TABS: usize = 6;

/// Entrée du pool de cache WebView.
struct CachedTabWebView {
    webview: WebView,
    last_url: String,
}

/// Commandes IPC JSON envoyées par l'UI ou les WebViews de contenu vers le backend Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum IpcCommand {
    InitShell,
    QueryFts {
        query: String,
    },
    Navigate {
        input: String,
    },
    SwitchTab {
        tab_id: i64,
    },
    NewTab {
        group_id: i64,
    },
    CloseTab {
        tab_id: i64,
    },
    /// Création d'un groupe avec nom, couleur et icône personnalisés.
    NewGroup {
        name: String,
        #[serde(default = "default_group_color")]
        color: String,
        #[serde(default = "default_group_icon")]
        icon: String,
    },
    /// Modification complète d'un groupe (via menu clic droit).
    UpdateGroup {
        group_id: i64,
        name: String,
        color: String,
        icon: String,
    },
    /// Déplacement d'un groupe vers le haut (`-1`) ou vers le bas (`1`).
    MoveGroup {
        group_id: i64,
        direction: i64,
    },
    /// Mise en veille profonde de tous les onglets d'un groupe (purge du cache RAM).
    HibernateGroup {
        group_id: i64,
    },
    /// Suppression d'un groupe (avec conservation ou fermeture de ses onglets).
    DeleteGroup {
        group_id: i64,
        keep_tabs: bool,
    },
    ToggleGroup {
        group_id: i64,
    },
    /// Décharge l'onglet actif du cache RAM et le place en veille disque.
    HibernateActive,
    /// Purge tous les onglets en arrière-plan du cache RAM.
    PurgeAllCache,
    ToggleBookmark,
    /// Ajout d'un nouveau favori depuis la page `yose://bookmarks`.
    AddBookmark {
        title: String,
        url: String,
    },
    /// Modification du titre ou de l'URL d'un favori existant.
    UpdateBookmark {
        id: i64,
        title: String,
        url: String,
    },
    /// Suppression d'un favori.
    DeleteBookmark {
        id: i64,
    },
    OmniboxOverlay {
        visible: bool,
    },
    UpdateScroll {
        scroll_pos: f64,
    },
    PageLoaded {
        url: String,
        title: String,
        meta: String,
        scroll_pos: f64,
    },
}

fn default_group_color() -> String {
    "#2E7D47".to_string()
}

fn default_group_icon() -> String {
    "◆".to_string()
}

/// État central de l'application yoSE côté Rust.
struct AppState {
    window: Rc<Window>,
    db: Database,
    shell_webview: WebView,
    /// Pool LRU d'onglets en cache mémoire (`tab_id -> CachedTabWebView`) pour un changement en 0 ms.
    webview_cache: HashMap<i64, CachedTabWebView>,
    /// File d'ordre LRU (le plus ancien en tête, le plus récent en queue).
    lru_order: VecDeque<i64>,
    /// Identifiant de l'onglet actuellement visible à l'écran.
    current_visible_tab_id: Option<i64>,
    /// Dernière position de scroll connue de l'onglet actif.
    live_scroll_pos: f64,
    /// Indique si un menu flottant (Omnibox FTS5 ou Menu Clic Droit Groupe) est ouvert.
    overlay_open: bool,
}

impl AppState {
    /// Synchronise l'état SQLite avec le Shell UI.
    fn sync_shell(&self) {
        if let Ok(snapshot) = self.db.snapshot() {
            if let Ok(json) = serde_json::to_string(&snapshot) {
                let script = format!(
                    "window.__yoseUI && window.__yoseUI.onSnapshot({});",
                    json
                );
                let _ = self.shell_webview.evaluate_script(&script);
            }
        }
    }

    /// Pousse les résultats FTS5 au Shell UI.
    fn push_fts_results(&self, query: &str) {
        if let Ok(hits) = self.db.search_fts(query, 8) {
            let payload = serde_json::json!({
                "query": query,
                "hits": hits,
            });
            let script = format!(
                "window.__yoseUI && window.__yoseUI.onFtsResults({});",
                payload
            );
            let _ = self.shell_webview.evaluate_script(&script);
        }
    }

    /// Applique le thème de l'OS (`Theme::Light` -> `logoT-sombre`, `Theme::Dark` -> `logoT-clair`).
    fn apply_os_theme(&self, theme: Theme) {
        let theme_str = match theme {
            Theme::Light => "light",
            _ => "dark",
        };
        let script = format!(
            "window.__yoseUI && window.__yoseUI.setOsTheme('{}');",
            theme_str
        );
        let _ = self.shell_webview.evaluate_script(&script);
    }

    /// Rectangle géométrique du viewport de contenu Web.
    fn content_bounds(&self) -> Rect {
        let scale = self.window.scale_factor();
        let size = self.window.inner_size().to_logical::<f64>(scale);

        let x = ui::SIDEBAR_WIDTH as f64;
        let y = ui::TOPBAR_HEIGHT as f64;
        let width = (size.width - x).max(0.0);
        let height = if self.overlay_open {
            0.0
        } else {
            (size.height - y).max(0.0)
        };

        Rect {
            position: Position::Logical(LogicalPosition::new(x, y)),
            size: Size::Logical(LogicalSize::new(width, height)),
        }
    }

    /// Rectangle de mise en sommeil (0x0 hors champ) pour les WebViews gardées en cache.
    fn hidden_bounds() -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size: Size::Logical(LogicalSize::new(0.0, 0.0)),
        }
    }

    /// Redimensionne le Shell et la WebView visible uniquement.
    fn resize_viewports(&self) {
        let scale = self.window.scale_factor();
        let size = self.window.inner_size().to_logical::<f64>(scale);

        let shell_rect = Rect {
            position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size: Size::Logical(LogicalSize::new(size.width, size.height)),
        };
        let _ = self.shell_webview.set_bounds(shell_rect);

        if let Some(active_id) = self.current_visible_tab_id {
            if let Some(cached) = self.webview_cache.get(&active_id) {
                let bounds = self.content_bounds();
                let _ = cached.webview.set_bounds(bounds);
                let _ = cached.webview.set_visible(!self.overlay_open);
            }
        }
    }

    /// Met à jour l'ordre d'utilisation dans la file LRU et évince l'onglet le plus ancien si plein.
    fn touch_lru(&mut self, tab_id: i64) {
        self.lru_order.retain(|&id| id != tab_id);
        self.lru_order.push_back(tab_id);

        while self.webview_cache.len() > MAX_HOT_CACHED_TABS {
            if let Some(oldest_id) = self.lru_order.pop_front() {
                if Some(oldest_id) != self.current_visible_tab_id {
                    self.webview_cache.remove(&oldest_id);
                    let _ = self
                        .db
                        .set_tab_status(oldest_id, TabStatus::Hibernated, None);
                }
            } else {
                break;
            }
        }
    }

    /// Supprime un onglet spécifique du cache RAM (ex: fermeture ou hibernation forcée).
    fn evict_tab_from_cache(&mut self, tab_id: i64) {
        self.webview_cache.remove(&tab_id);
        self.lru_order.retain(|&id| id != tab_id);
        if self.current_visible_tab_id == Some(tab_id) {
            self.current_visible_tab_id = None;
        }
    }

    /// Purge tous les onglets en arrière-plan du cache RAM (ne garde que l'onglet actif).
    fn purge_background_cache(&mut self) {
        let active_id = self.current_visible_tab_id;
        let to_remove: Vec<i64> = self
            .webview_cache
            .keys()
            .copied()
            .filter(|&id| Some(id) != active_id)
            .collect();

        for tab_id in to_remove {
            self.webview_cache.remove(&tab_id);
            self.lru_order.retain(|&id| id != tab_id);
            let _ = self.db.set_tab_status(tab_id, TabStatus::Hibernated, None);
        }
    }

    /// Bascule vers l'onglet actif en utilisant le **Cache Mémoire Instantané** :
    /// - Si l'onglet précédent était affiché, il est figé en cache (`__yoseFreeze`, `set_visible(false)`)
    ///   sans être détruit.
    /// - Si le nouvel onglet est déjà dans `webview_cache`, il est simplement réaffiché (`set_visible(true)`)
    ///   ou navigue via `load_url` si l'URL a changé : **0 rechargement inutile** !
    fn mount_active_tab(&mut self, proxy: &EventLoopProxy<IpcCommand>) {
        // 1. Figer l'onglet précédemment visible dans le cache (0% CPU, mais gardé en RAM)
        if let Some(prev_id) = self.current_visible_tab_id.take() {
            if let Some(prev_cached) = self.webview_cache.get(&prev_id) {
                let _ = prev_cached
                    .webview
                    .evaluate_script("window.__yoseFreeze && window.__yoseFreeze();");
                let _ = prev_cached.webview.set_bounds(Self::hidden_bounds());
                let _ = prev_cached.webview.set_visible(false);
            }
        }

        let active_tab = match self.db.get_active_tab().ok().flatten() {
            Some(tab) => tab,
            None => return,
        };

        self.live_scroll_pos = active_tab.scroll_pos;

        // 2. Si l'onglet actif est `yose://home`, aucune WebView externe n'est affichée
        if active_tab.url.starts_with("yose://") {
            self.push_fts_results("");
            return;
        }

        let tab_id = active_tab.id;
        let bounds = self.content_bounds();

        // 3. CACHE HIT : L'onglet est déjà en cache mémoire -> Restauration instantanée (0 ms) !
        if let Some(cached) = self.webview_cache.get_mut(&tab_id) {
            if cached.last_url != active_tab.url {
                cached.last_url = active_tab.url.clone();
                let _ = cached.webview.load_url(&active_tab.url);
            } else {
                let resume_js = format!(
                    "window.__yoseResume && window.__yoseResume({});",
                    active_tab.scroll_pos
                );
                let _ = cached.webview.evaluate_script(&resume_js);
            }
            let _ = cached.webview.set_bounds(bounds);
            let _ = cached.webview.set_visible(!self.overlay_open);
            self.current_visible_tab_id = Some(tab_id);
            self.touch_lru(tab_id);
            return;
        }

        // 4. CACHE MISS : Premier chargement de cet onglet -> Création et insertion dans le cache LRU
        let target_scroll = active_tab.scroll_pos;
        let restore_script = format!(
            "{}\nwindow.addEventListener('load', function() {{ window.__yoseResume && window.__yoseResume({}); }});",
            ui::CONTENT_INIT_SCRIPT,
            target_scroll
        );

        let proxy_clone = proxy.clone();
        let builder = WebViewBuilder::new()
            .with_bounds(bounds)
            .with_url(&active_tab.url)
            .with_initialization_script(&restore_script)
            .with_ipc_handler(move |req| {
                if let Ok(cmd) = serde_json::from_str::<IpcCommand>(req.body()) {
                    let _ = proxy_clone.send_event(cmd);
                }
            });

        if let Ok(wv) = builder.build_as_child(&*self.window) {
            self.webview_cache.insert(
                tab_id,
                CachedTabWebView {
                    webview: wv,
                    last_url: active_tab.url,
                },
            );
            self.current_visible_tab_id = Some(tab_id);
            self.touch_lru(tab_id);
        }
    }
}

/// Décode `assets/logo_color.png` une seule fois au démarrage pour l'icône native de l'OS.
fn load_os_window_icon() -> Option<Icon> {
    let img = image::load_from_memory(ui::ASSET_LOGO_COLOR)
        .ok()?
        .into_rgba8();
    let (width, height) = img.dimensions();
    Icon::from_rgba(img.into_raw(), width, height).ok()
}

/// Protocole personnalisé `yose://` avec en-têtes `Cache-Control: immutable`
/// pour que les logos ne soient jamais rechargés ni redécodés par le moteur Web.
fn handle_yose_protocol(uri: &str) -> Response<Cow<'static, [u8]>> {
    let path = uri
        .strip_prefix("yose://")
        .unwrap_or("")
        .trim_end_matches('/');

    let (mime, cache_header, bytes): (&str, &str, &'static [u8]) = match path {
        "assets/logoT-clair.png" => (
            "image/png",
            "public, max-age=31536000, immutable",
            ui::ASSET_LOGO_CLAIR,
        ),
        "assets/logoT-sombre.png" => (
            "image/png",
            "public, max-age=31536000, immutable",
            ui::ASSET_LOGO_SOMBRE,
        ),
        "assets/logo_color.png" => (
            "image/png",
            "public, max-age=31536000, immutable",
            ui::ASSET_LOGO_COLOR,
        ),
        _ => (
            "text/html; charset=utf-8",
            "no-cache",
            ui::SHELL_HTML.as_bytes(),
        ),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, mime)
        .header(CACHE_CONTROL, cache_header)
        .body(Cow::Borrowed(bytes))
        .unwrap()
}

/// Résout une saisie de l'Omnibox selon la stratégie **Local-First** (SQLite FTS5 -> URL -> DuckDuckGo).
fn resolve_omnibox_target(db: &Database, raw_input: &str) -> (String, String) {
    let input = raw_input.trim();
    if input.is_empty() || input.eq_ignore_ascii_case("yose://home") {
        return (
            "yose://home".to_string(),
            "yoSE - Recherche".to_string(),
        );
    }

    if input.eq_ignore_ascii_case("yose://bookmarks") || input.eq_ignore_ascii_case("yose://favoris") {
        return (
            "yose://bookmarks".to_string(),
            "yoSE — Favoris".to_string(),
        );
    }

    if input.starts_with("yose://") {
        return (input.to_string(), format!("yoSE — {}", input));
    }

    if let Ok(Some(local_url)) = db.find_exact_local_match(input) {
        return (local_url, input.to_string());
    }

    if input.starts_with("https://") || input.starts_with("http://") {
        return (input.to_string(), input.to_string());
    }

    let looks_like_domain = !input.contains(' ')
        && input.contains('.')
        && !input.starts_with('.')
        && !input.ends_with('.');

    if looks_like_domain {
        return (format!("https://{}", input), input.to_string());
    }

    if let Ok(hits) = db.search_fts(input, 1) {
        if let Some(best) = hits.first() {
            if best.is_bookmark {
                return (best.url.clone(), best.title.clone());
            }
        }
    }

    let encoded = urlencoding::encode(input);
    (
        format!("https://duckduckgo.com/?q={}", encoded),
        format!("Recherche : {}", input),
    )
}

/// Résout un chemin d'écriture fiable pour `yose_session.db` (compatible `.app` macOS et `.exe` Windows).
fn resolve_db_path() -> std::path::PathBuf {
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let dir = std::path::PathBuf::from(home).join(".yose");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir.join("yose_session.db");
        }
    }
    std::path::PathBuf::from("yose_session.db")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    gtk::init()?;

    let db = Database::open(resolve_db_path())?;

    let event_loop = EventLoopBuilder::<IpcCommand>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window_icon = load_os_window_icon();
    let window = Rc::new(
        WindowBuilder::new()
            .with_title("yoSE")
            .with_inner_size(LogicalSize::new(1260.0, 820.0))
            .with_min_inner_size(LogicalSize::new(760.0, 480.0))
            .with_window_icon(window_icon)
            .build(&event_loop)?,
    );

    let scale = window.scale_factor();
    let win_size = window.inner_size().to_logical::<f64>(scale);
    let shell_proxy = proxy.clone();

    let shell_webview = WebViewBuilder::new()
        .with_bounds(Rect {
            position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size: Size::Logical(LogicalSize::new(win_size.width, win_size.height)),
        })
        .with_custom_protocol("yose".into(), move |_webview_id, request| {
            handle_yose_protocol(request.uri().to_string().as_str())
        })
        .with_url("yose://shell")
        .with_ipc_handler(move |req| {
            if let Ok(cmd) = serde_json::from_str::<IpcCommand>(req.body()) {
                let _ = shell_proxy.send_event(cmd);
            }
        })
        .build_as_child(&*window)?;

    let app_state = Rc::new(RefCell::new(AppState {
        window: Rc::clone(&window),
        db,
        shell_webview,
        webview_cache: HashMap::with_capacity(MAX_HOT_CACHED_TABS + 1),
        lru_order: VecDeque::with_capacity(MAX_HOT_CACHED_TABS + 1),
        current_visible_tab_id: None,
        live_scroll_pos: 0.0,
        overlay_open: false,
    }));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                let state = app_state.borrow();
                state.apply_os_theme(state.window.theme());
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    let state = app_state.borrow();
                    let _ = state.db.save_active_scroll(state.live_scroll_pos);
                    *control_flow = ControlFlow::Exit;
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    app_state.borrow().resize_viewports();
                }
                WindowEvent::ThemeChanged(new_theme) => {
                    app_state.borrow().apply_os_theme(new_theme);
                }
                _ => {}
            },

            Event::UserEvent(cmd) => {
                let mut state = app_state.borrow_mut();
                match cmd {
                    IpcCommand::InitShell => {
                        state.apply_os_theme(state.window.theme());
                        state.sync_shell();
                        state.push_fts_results("");
                        state.mount_active_tab(&proxy);
                    }

                    IpcCommand::QueryFts { query } => {
                        state.push_fts_results(&query);
                    }

                    IpcCommand::OmniboxOverlay { visible } => {
                        state.overlay_open = visible;
                        state.resize_viewports();
                    }

                    IpcCommand::Navigate { input } => {
                        state.overlay_open = false;
                        let (target_url, initial_title) =
                            resolve_omnibox_target(&state.db, &input);
                        let _ = state.db.update_active_tab(&target_url, &initial_title, 0.0);
                        let _ = state
                            .db
                            .index_page(&target_url, &initial_title, &input, false);

                        // Si on retourne sur `yose://home`, on libère l'éventuelle WebView de cet onglet
                        if target_url.starts_with("yose://") {
                            if let Some(active) = state.db.get_active_tab().ok().flatten() {
                                state.evict_tab_from_cache(active.id);
                            }
                        }

                        state.mount_active_tab(&proxy);
                        state.sync_shell();
                    }

                    IpcCommand::SwitchTab { tab_id } => {
                        state.overlay_open = false;
                        let current_scroll = state.live_scroll_pos;
                        // L'onglet précédent reste en cache mémoire (`keep_previous_cached = true`)
                        let prev_was_cached = state
                            .current_visible_tab_id
                            .map(|id| state.webview_cache.contains_key(&id))
                            .unwrap_or(false);
                        let _ = state.db.activate_tab(
                            tab_id,
                            Some(current_scroll),
                            prev_was_cached,
                        );
                        state.mount_active_tab(&proxy);
                        state.sync_shell();
                    }

                    IpcCommand::NewTab { group_id } => {
                        state.overlay_open = false;
                        let current_scroll = state.live_scroll_pos;
                        let prev_was_cached = state
                            .current_visible_tab_id
                            .map(|id| state.webview_cache.contains_key(&id))
                            .unwrap_or(false);
                        let _ = state.db.create_tab(
                            group_id,
                            "yose://home",
                            "yoSE - Recherche",
                            Some(current_scroll),
                            prev_was_cached,
                        );
                        state.mount_active_tab(&proxy);
                        state.sync_shell();
                    }

                    IpcCommand::CloseTab { tab_id } => {
                        state.evict_tab_from_cache(tab_id);
                        let _ = state.db.close_tab(tab_id);
                        state.mount_active_tab(&proxy);
                        state.sync_shell();
                    }

                    // =========================================================
                    // ACTIONS DE PERSONNALISATION DES GROUPES (CLIC DROIT)
                    // =========================================================
                    IpcCommand::NewGroup { name, color, icon } => {
                        let _ = state.db.create_group(&name, &color, &icon);
                        state.sync_shell();
                    }

                    IpcCommand::UpdateGroup {
                        group_id,
                        name,
                        color,
                        icon,
                    } => {
                        state.overlay_open = false;
                        let _ = state.db.update_group(group_id, &name, &color, &icon);
                        state.resize_viewports();
                        state.sync_shell();
                    }

                    IpcCommand::MoveGroup {
                        group_id,
                        direction,
                    } => {
                        state.overlay_open = false;
                        let _ = state.db.move_group(group_id, direction);
                        state.resize_viewports();
                        state.sync_shell();
                    }

                    IpcCommand::HibernateGroup { group_id } => {
                        state.overlay_open = false;
                        if let Ok(tab_ids) = state.db.hibernate_group(group_id) {
                            for tid in tab_ids {
                                state.evict_tab_from_cache(tid);
                            }
                        }
                        state.resize_viewports();
                        state.sync_shell();
                    }

                    IpcCommand::DeleteGroup {
                        group_id,
                        keep_tabs,
                    } => {
                        state.overlay_open = false;
                        if let Ok(removed_tabs) = state.db.delete_group(group_id, keep_tabs) {
                            for tid in removed_tabs {
                                state.evict_tab_from_cache(tid);
                            }
                        }
                        state.mount_active_tab(&proxy);
                        state.resize_viewports();
                        state.sync_shell();
                    }

                    IpcCommand::ToggleGroup { group_id } => {
                        let _ = state.db.toggle_group_collapse(group_id);
                        state.sync_shell();
                    }

                    // =========================================================
                    // GESTION DU CACHE & HIBERNATION MANUELLE
                    // =========================================================
                    IpcCommand::HibernateActive => {
                        if let Ok(Some(active)) = state.db.get_active_tab() {
                            let scroll = state.live_scroll_pos;
                            state.evict_tab_from_cache(active.id);
                            let _ = state.db.set_tab_status(
                                active.id,
                                TabStatus::Hibernated,
                                Some(scroll),
                            );
                            state.sync_shell();
                        }
                    }

                    IpcCommand::PurgeAllCache => {
                        state.purge_background_cache();
                        state.sync_shell();
                    }

                    IpcCommand::ToggleBookmark => {
                        if let Ok(Some(active)) = state.db.get_active_tab() {
                            let _ = state.db.toggle_bookmark(&active.url, &active.title);
                            state.sync_shell();
                            state.push_fts_results("");
                        }
                    }

                    IpcCommand::AddBookmark { title, url } => {
                        let _ = state.db.add_bookmark(&title, &url);
                        state.sync_shell();
                        state.push_fts_results("");
                    }

                    IpcCommand::UpdateBookmark { id, title, url } => {
                        let _ = state.db.update_bookmark(id, &title, &url);
                        state.sync_shell();
                        state.push_fts_results("");
                    }

                    IpcCommand::DeleteBookmark { id } => {
                        let _ = state.db.delete_bookmark(id);
                        state.sync_shell();
                        state.push_fts_results("");
                    }

                    IpcCommand::UpdateScroll { scroll_pos } => {
                        state.live_scroll_pos = scroll_pos;
                        let _ = state.db.save_active_scroll(scroll_pos);
                    }

                    IpcCommand::PageLoaded {
                        url,
                        title,
                        meta,
                        scroll_pos,
                    } => {
                        state.live_scroll_pos = scroll_pos;
                        if let Some(active_id) = state.current_visible_tab_id {
                            if let Some(cached) = state.webview_cache.get_mut(&active_id) {
                                cached.last_url = url.clone();
                            }
                        }
                        let _ = state.db.update_active_tab(&url, &title, scroll_pos);
                        let _ = state.db.index_page(&url, &title, &meta, false);
                        state.sync_shell();
                    }
                }
            }

            _ => {}
        }
    });
}
