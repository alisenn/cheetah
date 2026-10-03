# cheetah

A lightweight, open-source desktop browser that uses very little RAM. Runs on Windows, macOS and Linux.

cheetah has no rendering engine of its own. It uses [`tao`](https://github.com/tauri-apps/tao) for the window and
[`wry`](https://github.com/tauri-apps/wry) for the system WebView. One process, one window: a toolbar webview
(tab strip + address bar) on top and one webview per tab below it.

## Features

- Open, close and switch tabs (the last tab cannot be closed)
- Smart address bar: opens URLs, searches DuckDuckGo otherwise
- Back, forward, reload
- Tab titles and URLs follow the page
- RAM saving: background tabs idle for 120 s have their webview destroyed (URL and title are kept) and are
  recreated when you switch back. Sleeping tabs are shown dimmed and in italics
- Bookmark bar: star button to bookmark or unbookmark the current page, click to open, right-click to remove.
  Stored as JSON in the OS config directory (`cheetah/bookmarks.json`)
- Import: the Import button copies the bookmark bar from Chrome, Edge, Brave, Chromium and Vivaldi (all profiles)
- Dark theme

## Run

```bash
cargo run --release
```

**Linux:** needs `libwebkit2gtk-4.1-dev` and `libgtk-3-dev`. `build_as_child` may not work on Wayland:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
GDK_BACKEND=x11 cargo run --release
```

## Roadmap

- [ ] Import from Firefox and Safari, plus history
- [ ] Total memory budget (sleep the oldest tabs when exceeded)
- [ ] Pin a tab so it never sleeps
- [ ] Shortcuts (Ctrl/Cmd+T, W, L, R)
- [ ] Tab drag and drop
- [ ] Configurable search engine and sleep time
- [ ] Find in page, download manager
- [ ] Session restore

## License

MIT
