# yoSE — yo Search Engine

Navigateur Web hybride ultra-minimaliste et moteur de recherche local **SQLite FTS5**, écrit en **Rust** (`tao` + `wry` + `rusqlite`).

## Caractéristiques
- **Zéro Electron / Zéro CEF** : S'appuie sur la WebView native de l'OS (`WKWebView` sur macOS, `WebView2` sur Windows, `WebKitGTK` sur Linux).
- **Cache Mémoire LRU (`0 ms`) & Hibernation** : Les onglets consultés sont gelés en cache (`0% CPU`) pour un retour instantané sans rechargement réseau, et déchargés en veille SQLite (`0 Mo RAM`) à la demande.
- **Moteur Local-First (`SQLite WAL + FTS5`)** : Recherche plein-texte instantanée dans l'historique et les favoris avant tout fallback externe (DuckDuckGo).
- **Groupes Personnalisables** : Clic droit sur un groupe pour modifier son nom, sa couleur, son ordre ou figer tous ses onglets.
- **Gestionnaire de Favoris (`yose://bookmarks`)** : Ajout, filtrage, renommage et suppression des favoris en direct.

## Téléchargement (Releases)
Rendez-vous dans l'onglet **[Releases](../../releases)** du dépôt GitHub pour télécharger :
- **Windows (x64)** : `yoSE-windows-x64.exe`
- **macOS (Universal Apple Silicon & Intel)** : `yoSE-macos-universal.dmg` (ou `yoSE-macos-universal.app.zip`)

## Compilation locale
```bash
cargo run --release
```
