//! Interface graphique "Moss & Obsidian" — Édition Radicalement Minimaliste.
//!
//! Inclut :
//! - Accueil `yose://home` pur (uniquement le logo dynamique et la barre de recherche).
//! - Troisième icône dans la barre supérieure (entre `❄` Freeze et `◐` Thème) ouvrant
//!   la page dédiée `yose://bookmarks` pour ajouter, renommer, gérer et supprimer ses favoris.

/// Icône système couleur (Dock / Barre des tâches / Exécutable).
pub const ASSET_LOGO_COLOR: &[u8] = include_bytes!("../assets/logo_color.png");

/// Logo transparent clair (affiché en Dark Mode — défaut).
pub const ASSET_LOGO_CLAIR: &[u8] = include_bytes!("../assets/logoT-clair.png");

/// Logo transparent sombre (affiché en Light Mode).
pub const ASSET_LOGO_SOMBRE: &[u8] = include_bytes!("../assets/logoT-sombre.png");

/// Dimensions ultra-compactes du layout (partagées avec `main.rs`).
pub const SIDEBAR_WIDTH: u32 = 200;
pub const TOPBAR_HEIGHT: u32 = 36;

/// Script d'instrumentation injecté dans les WebViews de contenu Web.
pub const CONTENT_INIT_SCRIPT: &str = r#"
(function() {
    if (window.__yoseContentInjected) return;
    window.__yoseContentInjected = true;

    let scrollTimer = null;
    let lastReportedScroll = -1;

    window.addEventListener('scroll', function() {
        if (scrollTimer) return;
        scrollTimer = setTimeout(function() {
            scrollTimer = null;
            const y = Math.round(window.scrollY || document.documentElement.scrollTop || 0);
            if (Math.abs(y - lastReportedScroll) >= 4 && window.ipc) {
                lastReportedScroll = y;
                window.ipc.postMessage(JSON.stringify({
                    cmd: 'UpdateScroll',
                    scroll_pos: y
                }));
            }
        }, 200);
    }, { passive: true });

    window.addEventListener('DOMContentLoaded', function() {
        const metaDesc = document.querySelector('meta[name="description"]');
        const metaKeywords = document.querySelector('meta[name="keywords"]');
        const metaText = [
            metaDesc ? metaDesc.content : '',
            metaKeywords ? metaKeywords.content : ''
        ].filter(Boolean).join(' ');

        if (window.ipc) {
            window.ipc.postMessage(JSON.stringify({
                cmd: 'PageLoaded',
                url: window.location.href,
                title: document.title || window.location.hostname || window.location.href,
                meta: metaText,
                scroll_pos: window.scrollY || 0
            }));
        }
    });

    window.__yoseFreeze = function() {
        try {
            document.querySelectorAll('video, audio').forEach(function(media) {
                if (!media.paused) media.pause();
            });
        } catch (_) {}
    };

    window.__yoseResume = function(pos) {
        try {
            if (typeof pos === 'number' && pos > 0 && Math.abs((window.scrollY || 0) - pos) > 8) {
                window.scrollTo({ top: pos, behavior: 'instant' });
            }
        } catch (_) {}
    };
})();
"#;

/// Coquille HTML/CSS ultra-minimaliste "Moss & Obsidian".
pub const SHELL_HTML: &str = r#"<!DOCTYPE html>
<html lang="fr" data-os-theme="dark">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>yoSE</title>
<style>
:root {
    --bg-base: #0D110E;
    --bg-surface: #161D18;
    --border: #253329;
    --text-main: #E3EBE5;
    --text-muted: #7E9484;
    --accent: #2E7D47;
    --status-hibernated: #4ADE80;
    --status-cached: #38BDF8;
    --danger: #D9534F;

    --font-ui: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    --font-mono: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace;
    --radius: 4px;
}

@media (prefers-color-scheme: light) {
    :root:not([data-os-theme="dark-forced"]) {
        --bg-base: #F4F7F5;
        --bg-surface: #E8EEEA;
        --border: #CBD6CE;
        --text-main: #111713;
        --text-muted: #566B5C;
        --accent: #2E7D47;
        --status-hibernated: #16A34A;
        --status-cached: #0284C7;
    }
}

html[data-os-theme="light"] {
    --bg-base: #F4F7F5;
    --bg-surface: #E8EEEA;
    --border: #CBD6CE;
    --text-main: #111713;
    --text-muted: #566B5C;
    --accent: #2E7D47;
    --status-hibernated: #16A34A;
    --status-cached: #0284C7;
}

* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
    user-select: none;
    -webkit-user-select: none;
}

html, body {
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--bg-base);
    color: var(--text-main);
    font-family: var(--font-ui);
    font-size: 12px;
}

/* Logos dynamiques Clair / Sombre */
.brand-logo {
    display: inline-flex;
    align-items: center;
}
.brand-logo img {
    display: block;
    height: 100%;
    width: auto;
}
.logo-clair { display: block !important; }
.logo-sombre { display: none !important; }

@media (prefers-color-scheme: light) {
    html:not([data-os-theme="dark-forced"]) .logo-clair { display: none !important; }
    html:not([data-os-theme="dark-forced"]) .logo-sombre { display: block !important; }
}
html[data-os-theme="light"] .logo-clair { display: none !important; }
html[data-os-theme="light"] .logo-sombre { display: block !important; }

/* ==========================================================================
   LAYOUT ULTRA-COMPACT : SIDEBAR (200px) + TOPBAR (36px)
   ========================================================================== */
.app-shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    grid-template-rows: 36px 1fr;
    width: 100vw;
    height: 100vh;
}

/* SIDEBAR */
.sidebar {
    grid-column: 1 / 2;
    grid-row: 1 / 3;
    background: var(--bg-base);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.sidebar-header {
    height: 36px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
}

.sidebar-brand {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
}

.sidebar-brand .brand-logo {
    height: 16px;
}

.cache-dot-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    cursor: pointer;
}
.cache-dot-btn:hover {
    color: var(--text-main);
}

.groups-container {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
}

.group-block {
    margin-bottom: 8px;
}

.group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 3px 6px;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
    border-radius: var(--radius);
    cursor: pointer;
}

.group-header:hover {
    color: var(--text-main);
    background: var(--bg-surface);
}

.group-title-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1;
}

.group-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
}

.group-name-text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.group-add-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1;
    opacity: 0;
    cursor: pointer;
    padding: 0 2px;
}

.group-header:hover .group-add-btn {
    opacity: 1;
}
.group-add-btn:hover {
    color: var(--text-main);
}

.tab-list {
    margin-top: 2px;
    display: flex;
    flex-direction: column;
    gap: 1px;
}

.tab-item {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 26px;
    padding: 0 7px;
    border-radius: var(--radius);
    color: var(--text-muted);
    cursor: pointer;
}

.tab-item:hover {
    background: var(--bg-surface);
    color: var(--text-main);
}

.tab-item.active {
    background: var(--bg-surface);
    color: var(--text-main);
    border-left: 2px solid var(--accent);
}

.tab-status-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
}

.tab-item.active .tab-status-dot {
    background: var(--accent);
}
.tab-item.cached .tab-status-dot {
    background: var(--status-cached);
}
.tab-item.hibernated .tab-status-dot {
    background: var(--status-hibernated);
    opacity: 0.45;
}

.tab-title {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
}

.tab-close {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    opacity: 0;
    cursor: pointer;
    padding: 0 2px;
}

.tab-item:hover .tab-close {
    opacity: 0.8;
}
.tab-close:hover {
    opacity: 1 !important;
    color: var(--text-main);
}

.sidebar-footer {
    height: 30px;
    padding: 0 8px;
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
}

.footer-link {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
}
.footer-link:hover {
    color: var(--text-main);
}

/* ==========================================================================
   TOPBAR MINIMALISTE : OMNIBOX + 3 ICÔNES (FREEZE · FAVORIS · THÈME)
   ========================================================================== */
.topbar {
    grid-column: 2 / 3;
    grid-row: 1 / 2;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 8px;
    gap: 4px;
}

.omnibox-wrapper {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    margin-right: 2px;
}

.omnibox-input {
    width: 100%;
    height: 24px;
    padding: 0 26px 0 8px;
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    outline: none;
    user-select: text;
    -webkit-user-select: text;
}

.omnibox-input:focus {
    border-color: var(--accent);
}

/* Indicateur unique de signet à droite dans l'Omnibox */
.omnibox-star {
    position: absolute;
    right: 6px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    opacity: 0.55;
    padding: 2px;
}

.omnibox-star:hover {
    opacity: 1;
}

.omnibox-star.active {
    color: var(--status-hibernated);
    opacity: 1;
}

.omnibox-star.hidden {
    display: none;
}

.top-icon-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-muted);
    border-radius: var(--radius);
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    cursor: pointer;
}

.top-icon-btn:hover {
    color: var(--text-main);
    border-color: var(--border);
}

.top-icon-btn.active {
    color: var(--status-hibernated);
    border-color: var(--border);
    background: var(--bg-surface);
}

/* ==========================================================================
   VIEWPORT : ACCUEIL `yose://home` & GESTIONNAIRE DE FAVORIS `yose://bookmarks`
   ========================================================================== */
.viewport {
    grid-column: 2 / 3;
    grid-row: 2 / 3;
    position: relative;
    background: var(--bg-base);
    overflow-y: auto;
}

.fts-overlay {
    display: none;
    position: absolute;
    top: 4px;
    left: 8px;
    right: 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    z-index: 50;
    padding: 4px;
}

.fts-overlay.visible {
    display: block;
}

.fts-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 8px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    cursor: pointer;
}

.fts-item:hover {
    background: var(--bg-base);
}

.fts-item-title {
    color: var(--text-main);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.fts-item-url {
    color: var(--text-muted);
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 45%;
}

/* Accueil Minimaliste Pur (Zéro liste indexée) */
.home-view {
    max-width: 440px;
    margin: 0 auto;
    padding-top: 26vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
}

.home-view.hidden {
    display: none;
}

.home-logo {
    height: 56px;
    opacity: 0.92;
}

.home-search-box {
    width: 100%;
}

.home-search-input {
    width: 100%;
    height: 34px;
    padding: 0 12px;
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 12px;
    outline: none;
    user-select: text;
    -webkit-user-select: text;
}

.home-search-input:focus {
    border-color: var(--accent);
}

/* Page dédiée des Favoris (`yose://bookmarks`) */
.bookmarks-view {
    max-width: 620px;
    margin: 0 auto;
    padding: 36px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
}

.bookmarks-view.hidden {
    display: none;
}

.bm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
}

.bm-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-main);
}

.bm-filter-input {
    width: 180px;
    height: 24px;
    padding: 0 8px;
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    outline: none;
    user-select: text;
    -webkit-user-select: text;
}
.bm-filter-input:focus {
    border-color: var(--accent);
}

.bm-add-form {
    display: grid;
    grid-template-columns: 1fr 1.4fr auto;
    gap: 6px;
}

.bm-input {
    height: 26px;
    padding: 0 8px;
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    outline: none;
    user-select: text;
    -webkit-user-select: text;
}
.bm-input:focus {
    border-color: var(--accent);
}

.bm-btn {
    height: 26px;
    padding: 0 10px;
    background: var(--bg-surface);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 11px;
    cursor: pointer;
}
.bm-btn:hover {
    border-color: var(--accent);
}

.bm-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.bm-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
}

.bm-row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    cursor: pointer;
}

.bm-row-name {
    color: var(--text-main);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.bm-row-url {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 50%;
}

.bm-row-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
}

.bm-row:hover .bm-row-actions {
    opacity: 1;
}

.bm-icon-action {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 12px;
    padding: 0 4px;
    cursor: pointer;
}
.bm-icon-action:hover {
    color: var(--text-main);
}
.bm-icon-action.danger:hover {
    color: var(--danger);
}

.bm-edit-grid {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1.4fr auto auto;
    gap: 5px;
    align-items: center;
}

.bm-empty {
    color: var(--text-muted);
    font-size: 11px;
    padding: 12px 0;
    text-align: center;
}

/* ==========================================================================
   MENU CLIC DROIT GROUPE (COMPACT)
   ========================================================================== */
.group-ctx-backdrop {
    display: none;
    position: fixed;
    inset: 0;
    z-index: 900;
}
.group-ctx-backdrop.open {
    display: block;
}

.group-ctx-menu {
    position: fixed;
    width: 190px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px;
    z-index: 901;
    display: flex;
    flex-direction: column;
    gap: 6px;
}

.ctx-input {
    width: 100%;
    height: 24px;
    padding: 0 6px;
    background: var(--bg-base);
    color: var(--text-main);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 11px;
    outline: none;
    user-select: text;
    -webkit-user-select: text;
}
.ctx-input:focus {
    border-color: var(--accent);
}

.ctx-swatches {
    display: flex;
    justify-content: space-between;
}

.color-swatch {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
}
.color-swatch.selected {
    border-color: var(--text-main);
}

.ctx-divider {
    height: 1px;
    background: var(--border);
}

.ctx-row {
    display: flex;
    gap: 4px;
}

.ctx-btn {
    flex: 1;
    padding: 4px 6px;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: var(--radius);
    font-size: 11px;
    text-align: left;
    cursor: pointer;
}
.ctx-btn:hover {
    background: var(--bg-base);
    color: var(--text-main);
}
.ctx-btn.danger:hover {
    color: var(--danger);
}
</style>
</head>
<body>
<div class="app-shell">
    <!-- SIDEBAR ÉPURÉE -->
    <aside class="sidebar">
        <div class="sidebar-header">
            <div class="sidebar-brand" id="btnHomeBrand" title="Accueil (yose://home)">
                <span class="brand-logo">
                    <img class="logo-clair" src="yose://assets/logoT-clair.png" decoding="async" alt="yo">
                    <img class="logo-sombre" src="yose://assets/logoT-sombre.png" decoding="async" alt="yo">
                </span>
            </div>
            <button class="cache-dot-btn" id="cacheCounter" title="Purger le cache d'arrière-plan">0 ◈</button>
        </div>

        <div class="groups-container" id="groupsContainer"></div>

        <div class="sidebar-footer">
            <button class="footer-link" id="btnNewTab">+ Onglet</button>
            <button class="footer-link" id="btnNewGroup">+ Groupe</button>
        </div>
    </aside>

    <!-- TOPBAR ÉPURÉE : OMNIBOX + [FREEZE ❄] [FAVORIS ★] [THÈME ◐] -->
    <header class="topbar">
        <div class="omnibox-wrapper">
            <input
                type="text"
                id="omniboxInput"
                class="omnibox-input"
                placeholder="Rechercher ou saisir une adresse..."
                autocomplete="off"
                spellcheck="false"
            />
            <button class="omnibox-star hidden" id="btnBookmark" title="Signet">☆</button>
        </div>

        <button class="top-icon-btn" id="btnHibernateNow" title="Mettre l'onglet en veille">❄</button>
        <button class="top-icon-btn" id="btnBookmarksPage" title="Gérer les favoris (yose://bookmarks)">★</button>
        <button class="top-icon-btn" id="btnThemeToggle" title="Thème Clair / Sombre">◐</button>
    </header>

    <!-- VIEWPORT -->
    <main class="viewport">
        <div class="fts-overlay" id="ftsOverlay">
            <div id="ftsResultsList"></div>
        </div>

        <!-- VUE 1 : ACCUEIL PUR `yose://home` -->
        <section class="home-view" id="homeView">
            <span class="brand-logo home-logo">
                <img class="logo-clair" src="yose://assets/logoT-clair.png" decoding="async" alt="yoSE">
                <img class="logo-sombre" src="yose://assets/logoT-sombre.png" decoding="async" alt="yoSE">
            </span>

            <form class="home-search-box" id="homeSearchForm">
                <input
                    type="text"
                    id="homeSearchInput"
                    class="home-search-input"
                    placeholder="Rechercher..."
                    autocomplete="off"
                    spellcheck="false"
                />
            </form>
        </section>

        <!-- VUE 2 : GESTIONNAIRE DE FAVORIS `yose://bookmarks` -->
        <section class="bookmarks-view hidden" id="bookmarksView">
            <div class="bm-header">
                <span class="bm-title">Favoris</span>
                <input
                    type="text"
                    id="bmFilterInput"
                    class="bm-filter-input"
                    placeholder="Filtrer..."
                    autocomplete="off"
                    spellcheck="false"
                />
            </div>

            <form class="bm-add-form" id="bmAddForm">
                <input
                    type="text"
                    id="bmAddTitle"
                    class="bm-input"
                    placeholder="Nom du favori"
                    autocomplete="off"
                    spellcheck="false"
                />
                <input
                    type="text"
                    id="bmAddUrl"
                    class="bm-input"
                    placeholder="https://..."
                    autocomplete="off"
                    spellcheck="false"
                />
                <button type="submit" class="bm-btn">+ Ajouter</button>
            </form>

            <div class="bm-list" id="bookmarksList"></div>
        </section>
    </main>
</div>

<!-- MENU CLIC DROIT SUR UN GROUPE -->
<div class="group-ctx-backdrop" id="groupCtxBackdrop">
    <div class="group-ctx-menu" id="groupCtxMenu">
        <input type="text" class="ctx-input" id="ctxGroupName" placeholder="Nom du groupe" spellcheck="false" />
        <div class="ctx-swatches" id="ctxColorSwatches"></div>
        <div class="ctx-divider"></div>
        <div class="ctx-row">
            <button class="ctx-btn" id="ctxMoveUpBtn">▲ Haut</button>
            <button class="ctx-btn" id="ctxMoveDownBtn">▼ Bas</button>
        </div>
        <button class="ctx-btn" id="ctxHibernateGroupBtn">❄ Figer le groupe</button>
        <button class="ctx-btn danger" id="ctxDeleteKeepTabsBtn">Supprimer le groupe</button>
    </div>
</div>

<script>
(function() {
    const GROUP_COLORS = ['#2E7D47', '#3B82F6', '#F59E0B', '#A855F7', '#14B8A6', '#EC4899', '#EF4444'];

    const state = {
        groups: [],
        tabs: [],
        bookmarks: [],
        stats: null,
        activeGroupId: 1,
        activeTab: null,
        editingGroup: null,
        selectedColor: '#2E7D47',
        editingBookmarkId: null,
        bookmarkFilter: ''
    };

    function sendIpc(payload) {
        if (window.ipc && typeof window.ipc.postMessage === 'function') {
            window.ipc.postMessage(JSON.stringify(payload));
        }
    }

    function escapeHtml(str) {
        return String(str || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    function renderBookmarksList() {
        const listEl = document.getElementById('bookmarksList');
        const q = (state.bookmarkFilter || '').toLowerCase().trim();
        const filtered = (state.bookmarks || []).filter(b =>
            !q || b.title.toLowerCase().includes(q) || b.url.toLowerCase().includes(q)
        );

        if (filtered.length === 0) {
            listEl.innerHTML = '<div class="bm-empty">Aucun favori enregistré.</div>';
            return;
        }

        listEl.innerHTML = filtered.map(b => {
            if (state.editingBookmarkId === b.id) {
                return `
                    <div class="bm-row" data-bm-id="${b.id}">
                        <div class="bm-edit-grid">
                            <input type="text" class="bm-input" id="bmEditTitle" value="${escapeHtml(b.title)}" spellcheck="false" />
                            <input type="text" class="bm-input" id="bmEditUrl" value="${escapeHtml(b.url)}" spellcheck="false" />
                            <button class="bm-btn" data-bm-save="${b.id}">✓</button>
                            <button class="bm-btn" data-bm-cancel="1">✕</button>
                        </div>
                    </div>
                `;
            }

            return `
                <div class="bm-row" data-bm-id="${b.id}">
                    <div class="bm-row-main" data-navigate-url="${escapeHtml(b.url)}">
                        <span class="bm-row-name">${escapeHtml(b.title)}</span>
                        <span class="bm-row-url">${escapeHtml(b.url)}</span>
                    </div>
                    <div class="bm-row-actions">
                        <button class="bm-icon-action" data-bm-edit="${b.id}" title="Renommer / modifier">✎</button>
                        <button class="bm-icon-action danger" data-bm-delete="${b.id}" title="Supprimer">×</button>
                    </div>
                </div>
            `;
        }).join('');
    }

    function renderSession(snapshot) {
        state.groups = snapshot.groups || [];
        state.tabs = snapshot.tabs || [];
        state.bookmarks = snapshot.bookmarks || [];
        state.stats = snapshot.stats || {};
        state.activeTab = state.tabs.find(t => t.is_active) || null;

        const activeUrl = state.activeTab ? state.activeTab.url : 'yose://home';
        const isInternal = activeUrl.startsWith('yose://');
        const isBookmarksPage = activeUrl === 'yose://bookmarks';
        const isHomePage = isInternal && !isBookmarksPage;

        if (state.activeTab) {
            state.activeGroupId = state.activeTab.group_id;
            const omnibox = document.getElementById('omniboxInput');
            if (document.activeElement !== omnibox) {
                omnibox.value = isHomePage ? '' : activeUrl;
            }
        }

        // 1 SEUL indicateur de signet dans la barre d'adresse (uniquement sur les pages Web)
        const bmStar = document.getElementById('btnBookmark');
        const isBookmarked = Boolean(state.activeTab && state.activeTab.is_bookmarked);
        bmStar.classList.toggle('hidden', isInternal);
        bmStar.classList.toggle('active', isBookmarked);
        bmStar.textContent = isBookmarked ? '★' : '☆';
        bmStar.title = isBookmarked ? 'Retirer des favoris' : 'Ajouter aux favoris';

        // État actif de l'icône Favoris en haut à droite
        document.getElementById('btnBookmarksPage').classList.toggle('active', isBookmarksPage);

        const cachedCount = state.stats ? (state.stats.cached_tabs || 0) : 0;
        document.getElementById('cacheCounter').textContent = `${cachedCount} ◈`;

        // Bascule entre yose://home et yose://bookmarks
        document.getElementById('homeView').classList.toggle('hidden', !isHomePage);
        document.getElementById('bookmarksView').classList.toggle('hidden', !isBookmarksPage);

        if (isBookmarksPage) {
            renderBookmarksList();
        }

        // Groupes & Onglets minimalistes
        const container = document.getElementById('groupsContainer');
        container.innerHTML = state.groups.map(group => {
            const groupTabs = state.tabs.filter(t => t.group_id === group.id);
            const groupColor = group.color || '#2E7D47';

            const tabsHtml = group.collapsed ? '' : groupTabs.map(tab => {
                let statusClass = 'hibernated';
                if (tab.is_active) statusClass = 'active';
                else if (tab.status === 'cached') statusClass = 'cached';

                return `
                    <div class="tab-item ${statusClass}" data-tab-id="${tab.id}" title="${escapeHtml(tab.title)} — ${escapeHtml(tab.url)}">
                        <span class="tab-status-dot"></span>
                        <span class="tab-title">${escapeHtml(tab.title)}</span>
                        <button class="tab-close" data-close-id="${tab.id}">×</button>
                    </div>
                `;
            }).join('');

            return `
                <div class="group-block">
                    <div class="group-header" data-group-toggle="${group.id}" data-group-id="${group.id}">
                        <div class="group-title-wrap">
                            <span class="group-dot" style="background: ${escapeHtml(groupColor)};"></span>
                            <span class="group-name-text">${escapeHtml(group.name)}</span>
                        </div>
                        <button class="group-add-btn" data-add-tab-group="${group.id}" title="Nouvel onglet">+</button>
                    </div>
                    <div class="tab-list">${tabsHtml}</div>
                </div>
            `;
        }).join('');
    }

    // =========================================================================
    // GESTION DES FAVORIS SUR `yose://bookmarks`
    // =========================================================================
    document.getElementById('bmFilterInput').addEventListener('input', function(e) {
        state.bookmarkFilter = e.target.value;
        renderBookmarksList();
    });

    document.getElementById('bmAddForm').addEventListener('submit', function(e) {
        e.preventDefault();
        const titleEl = document.getElementById('bmAddTitle');
        const urlEl = document.getElementById('bmAddUrl');
        if (!urlEl.value.trim()) return;
        sendIpc({
            cmd: 'AddBookmark',
            title: titleEl.value.trim(),
            url: urlEl.value.trim()
        });
        titleEl.value = '';
        urlEl.value = '';
    });

    document.getElementById('bookmarksList').addEventListener('click', function(e) {
        const navEl = e.target.closest('[data-navigate-url]');
        if (navEl) {
            sendIpc({ cmd: 'Navigate', input: navEl.dataset.navigateUrl });
            return;
        }

        const editBtn = e.target.closest('[data-bm-edit]');
        if (editBtn) {
            state.editingBookmarkId = Number(editBtn.dataset.bmEdit);
            renderBookmarksList();
            const input = document.getElementById('bmEditTitle');
            if (input) { input.focus(); input.select(); }
            return;
        }

        const cancelBtn = e.target.closest('[data-bm-cancel]');
        if (cancelBtn) {
            state.editingBookmarkId = null;
            renderBookmarksList();
            return;
        }

        const saveBtn = e.target.closest('[data-bm-save]');
        if (saveBtn) {
            const id = Number(saveBtn.dataset.bmSave);
            const titleEl = document.getElementById('bmEditTitle');
            const urlEl = document.getElementById('bmEditUrl');
            if (urlEl && urlEl.value.trim()) {
                sendIpc({
                    cmd: 'UpdateBookmark',
                    id: id,
                    title: titleEl ? titleEl.value.trim() : '',
                    url: urlEl.value.trim()
                });
            }
            state.editingBookmarkId = null;
            return;
        }

        const delBtn = e.target.closest('[data-bm-delete]');
        if (delBtn) {
            sendIpc({ cmd: 'DeleteBookmark', id: Number(delBtn.dataset.bmDelete) });
        }
    });

    document.getElementById('bookmarksList').addEventListener('keydown', function(e) {
        if (state.editingBookmarkId === null) return;
        if (e.key === 'Escape') {
            state.editingBookmarkId = null;
            renderBookmarksList();
        } else if (e.key === 'Enter') {
            e.preventDefault();
            const titleEl = document.getElementById('bmEditTitle');
            const urlEl = document.getElementById('bmEditUrl');
            if (urlEl && urlEl.value.trim()) {
                sendIpc({
                    cmd: 'UpdateBookmark',
                    id: state.editingBookmarkId,
                    title: titleEl ? titleEl.value.trim() : '',
                    url: urlEl.value.trim()
                });
            }
            state.editingBookmarkId = null;
        }
    });

    // =========================================================================
    // MENU CLIC DROIT SUR UN GROUPE
    // =========================================================================
    const ctxBackdrop = document.getElementById('groupCtxBackdrop');
    const ctxMenu = document.getElementById('groupCtxMenu');
    const ctxNameInput = document.getElementById('ctxGroupName');

    function renderColorSwatches() {
        document.getElementById('ctxColorSwatches').innerHTML = GROUP_COLORS.map(c => `
            <div class="color-swatch ${c === state.selectedColor ? 'selected' : ''}" style="background: ${c};" data-pick-color="${c}"></div>
        `).join('');
    }

    function openGroupMenu(groupId, x, y) {
        const group = state.groups.find(g => g.id === groupId);
        if (!group) return;
        state.editingGroup = group;
        state.selectedColor = group.color || '#2E7D47';
        ctxNameInput.value = group.name;
        renderColorSwatches();

        ctxMenu.style.left = `${Math.min(x, window.innerWidth - 198)}px`;
        ctxMenu.style.top = `${Math.min(y, window.innerHeight - 190)}px`;
        ctxBackdrop.classList.add('open');
        sendIpc({ cmd: 'OmniboxOverlay', visible: true });
        setTimeout(() => { ctxNameInput.focus(); ctxNameInput.select(); }, 10);
    }

    function commitAndCloseGroupMenu() {
        if (state.editingGroup) {
            sendIpc({
                cmd: 'UpdateGroup',
                group_id: state.editingGroup.id,
                name: ctxNameInput.value.trim() || state.editingGroup.name,
                color: state.selectedColor,
                icon: '◆'
            });
        }
        ctxBackdrop.classList.remove('open');
        state.editingGroup = null;
        sendIpc({ cmd: 'OmniboxOverlay', visible: false });
    }

    ctxBackdrop.addEventListener('click', function(e) {
        if (e.target === ctxBackdrop) commitAndCloseGroupMenu();
    });

    ctxMenu.addEventListener('click', function(e) {
        const swatch = e.target.closest('[data-pick-color]');
        if (swatch && state.editingGroup) {
            state.selectedColor = swatch.dataset.pickColor;
            renderColorSwatches();
            commitAndCloseGroupMenu();
        }
    });

    ctxNameInput.addEventListener('keydown', function(e) {
        if (e.key === 'Enter' || e.key === 'Escape') {
            e.preventDefault();
            commitAndCloseGroupMenu();
        }
    });

    document.getElementById('ctxMoveUpBtn').addEventListener('click', function() {
        if (!state.editingGroup) return;
        const id = state.editingGroup.id;
        commitAndCloseGroupMenu();
        sendIpc({ cmd: 'MoveGroup', group_id: id, direction: -1 });
    });

    document.getElementById('ctxMoveDownBtn').addEventListener('click', function() {
        if (!state.editingGroup) return;
        const id = state.editingGroup.id;
        commitAndCloseGroupMenu();
        sendIpc({ cmd: 'MoveGroup', group_id: id, direction: 1 });
    });

    document.getElementById('ctxHibernateGroupBtn').addEventListener('click', function() {
        if (!state.editingGroup) return;
        const id = state.editingGroup.id;
        commitAndCloseGroupMenu();
        sendIpc({ cmd: 'HibernateGroup', group_id: id });
    });

    document.getElementById('ctxDeleteKeepTabsBtn').addEventListener('click', function() {
        if (!state.editingGroup) return;
        const id = state.editingGroup.id;
        commitAndCloseGroupMenu();
        sendIpc({ cmd: 'DeleteGroup', group_id: id, keep_tabs: true });
    });

    document.getElementById('groupsContainer').addEventListener('contextmenu', function(e) {
        const header = e.target.closest('[data-group-id]');
        if (header) {
            e.preventDefault();
            openGroupMenu(Number(header.dataset.groupId), e.clientX, e.clientY);
        }
    });

    document.getElementById('groupsContainer').addEventListener('click', function(e) {
        const closeBtn = e.target.closest('[data-close-id]');
        if (closeBtn) {
            e.stopPropagation();
            sendIpc({ cmd: 'CloseTab', tab_id: Number(closeBtn.dataset.closeId) });
            return;
        }
        const addBtn = e.target.closest('[data-add-tab-group]');
        if (addBtn) {
            e.stopPropagation();
            sendIpc({ cmd: 'NewTab', group_id: Number(addBtn.dataset.addTabGroup) });
            return;
        }
        const groupHeader = e.target.closest('[data-group-toggle]');
        if (groupHeader) {
            sendIpc({ cmd: 'ToggleGroup', group_id: Number(groupHeader.dataset.groupToggle) });
            return;
        }
        const tabItem = e.target.closest('[data-tab-id]');
        if (tabItem) {
            sendIpc({ cmd: 'SwitchTab', tab_id: Number(tabItem.dataset.tabId) });
        }
    });

    // Double-clic gauche dans le vide de la barre latérale -> nouvel onglet "yoSE - Recherche"
    document.getElementById('groupsContainer').addEventListener('dblclick', function(e) {
        if (e.button !== 0) return;
        if (e.target.closest('[data-tab-id], [data-group-toggle], button')) return;
        sendIpc({ cmd: 'NewTab', group_id: state.activeGroupId });
        setTimeout(() => {
            const searchInput = document.getElementById('homeSearchInput');
            if (searchInput) searchInput.focus();
        }, 20);
    });

    function renderFtsHits(hits) {
        const html = hits.map(hit => `
            <div class="fts-item" data-navigate-url="${escapeHtml(hit.url)}">
                <span class="fts-item-title">${hit.is_bookmark ? '★ ' : ''}${escapeHtml(hit.title)}</span>
                <span class="fts-item-url">${escapeHtml(hit.url)}</span>
            </div>
        `).join('');
        document.getElementById('ftsResultsList').innerHTML = html;
    }

    document.getElementById('ftsResultsList').addEventListener('click', function(e) {
        const item = e.target.closest('[data-navigate-url]');
        if (!item) return;
        document.getElementById('ftsOverlay').classList.remove('visible');
        sendIpc({ cmd: 'OmniboxOverlay', visible: false });
        sendIpc({ cmd: 'Navigate', input: item.dataset.navigateUrl });
    });

    let ftsTimer = null;
    const omnibox = document.getElementById('omniboxInput');
    omnibox.addEventListener('focus', function() {
        omnibox.select();
        document.getElementById('ftsOverlay').classList.add('visible');
        sendIpc({ cmd: 'OmniboxOverlay', visible: true });
        sendIpc({ cmd: 'QueryFts', query: omnibox.value });
    });

    omnibox.addEventListener('input', function() {
        clearTimeout(ftsTimer);
        ftsTimer = setTimeout(() => sendIpc({ cmd: 'QueryFts', query: omnibox.value }), 40);
    });

    omnibox.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') {
            document.getElementById('ftsOverlay').classList.remove('visible');
            sendIpc({ cmd: 'OmniboxOverlay', visible: false });
            omnibox.blur();
        } else if (e.key === 'Enter') {
            e.preventDefault();
            document.getElementById('ftsOverlay').classList.remove('visible');
            sendIpc({ cmd: 'OmniboxOverlay', visible: false });
            omnibox.blur();
            sendIpc({ cmd: 'Navigate', input: omnibox.value });
        }
    });

    const homeInput = document.getElementById('homeSearchInput');
    document.getElementById('homeSearchForm').addEventListener('submit', function(e) {
        e.preventDefault();
        if (homeInput.value.trim()) {
            sendIpc({ cmd: 'Navigate', input: homeInput.value.trim() });
            homeInput.value = '';
        }
    });

    document.getElementById('btnNewTab').addEventListener('click', () => sendIpc({ cmd: 'NewTab', group_id: state.activeGroupId }));
    document.getElementById('btnNewGroup').addEventListener('click', () => {
        const idx = (state.groups ? state.groups.length : 0);
        sendIpc({ cmd: 'NewGroup', name: `Groupe ${idx + 1}`, color: GROUP_COLORS[idx % GROUP_COLORS.length], icon: '◆' });
    });
    document.getElementById('btnHomeBrand').addEventListener('click', () => sendIpc({ cmd: 'Navigate', input: 'yose://home' }));
    document.getElementById('btnHibernateNow').addEventListener('click', () => sendIpc({ cmd: 'HibernateActive' }));
    document.getElementById('btnBookmarksPage').addEventListener('click', () => {
        const isCurrentlyBookmarks = state.activeTab && state.activeTab.url === 'yose://bookmarks';
        sendIpc({ cmd: 'Navigate', input: isCurrentlyBookmarks ? 'yose://home' : 'yose://bookmarks' });
    });
    document.getElementById('cacheCounter').addEventListener('click', () => sendIpc({ cmd: 'PurgeAllCache' }));
    document.getElementById('btnBookmark').addEventListener('click', () => sendIpc({ cmd: 'ToggleBookmark' }));

    document.getElementById('btnThemeToggle').addEventListener('click', function() {
        const html = document.documentElement;
        html.setAttribute('data-os-theme', html.getAttribute('data-os-theme') === 'light' ? 'dark-forced' : 'light');
    });

    window.__yoseUI = {
        onSnapshot: renderSession,
        onFtsResults: payload => renderFtsHits(payload.hits || []),
        setOsTheme: theme => document.documentElement.setAttribute('data-os-theme', theme)
    };

    window.addEventListener('DOMContentLoaded', () => sendIpc({ cmd: 'InitShell' }));
})();
</script>
</body>
</html>
"#;
