mod bookmarks;

use std::time::{Duration, Instant};

use bookmarks::Bookmark;
use serde_json::json;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

const UI_HTML: &str = include_str!("ui.html");
const HOME_URL: &str = "https://duckduckgo.com";
const TOOLBAR_HEIGHT: f64 = 108.0;
const SLEEP_AFTER: Duration = Duration::from_secs(120);
const SLEEP_CHECK_INTERVAL: Duration = Duration::from_secs(15);

enum UserEvent {
    Ipc(String),
    Title(usize, String),
    Address(usize, String),
}

struct Tab {
    id: usize,
    url: String,
    title: String,
    view: Option<WebView>,
    asleep: bool,
    last_active: Instant,
}

struct App {
    window: Window,
    proxy: EventLoopProxy<UserEvent>,
    toolbar: WebView,
    toolbar_ready: bool,
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
    bookmarks: Vec<Bookmark>,
}

fn resolve_input(input: &str) -> String {
    let text = input.trim();
    if text.contains("://") {
        text.to_string()
    } else if !text.contains(char::is_whitespace) && text.contains('.') {
        format!("https://{text}")
    } else {
        let mut query = String::new();
        for byte in text.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => query.push(byte as char),
                _ => query.push_str(&format!("%{byte:02X}")),
            }
        }
        format!("https://duckduckgo.com/?q={query}")
    }
}

impl App {
    fn window_size(&self) -> tao::dpi::LogicalSize<f64> {
        self.window.inner_size().to_logical(self.window.scale_factor())
    }

    fn content_bounds(&self) -> Rect {
        let size = self.window_size();
        Rect {
            position: LogicalPosition::new(0.0, TOOLBAR_HEIGHT).into(),
            size: LogicalSize::new(size.width, (size.height - TOOLBAR_HEIGHT).max(1.0)).into(),
        }
    }

    fn toolbar_bounds(&self) -> Rect {
        Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(self.window_size().width, TOOLBAR_HEIGHT).into(),
        }
    }

    fn create_view(&self, id: usize, url: &str) -> WebView {
        let (title_proxy, nav_proxy) = (self.proxy.clone(), self.proxy.clone());
        WebViewBuilder::new()
            .with_url(url)
            .with_bounds(self.content_bounds())
            .with_document_title_changed_handler(move |title| {
                let _ = title_proxy.send_event(UserEvent::Title(id, title));
            })
            .with_navigation_handler(move |url| {
                if url != "about:blank" {
                    let _ = nav_proxy.send_event(UserEvent::Address(id, url));
                }
                true
            })
            .build_as_child(&self.window)
            .expect("failed to create tab webview")
    }

    fn open_tab(&mut self, url: &str) {
        let id = self.next_id;
        self.next_id += 1;
        let view = self.create_view(id, url);
        self.tabs.push(Tab {
            id,
            url: url.to_string(),
            title: String::new(),
            view: Some(view),
            asleep: false,
            last_active: Instant::now(),
        });
        self.activate(self.tabs.len() - 1);
    }

    fn activate(&mut self, index: usize) {
        if let Some(previous) = self.tabs.get_mut(self.active)
            && self.active != index
        {
            previous.last_active = Instant::now();
            if let Some(view) = &previous.view {
                let _ = view.set_visible(false);
            }
        }
        self.active = index;
        let bounds = self.content_bounds();
        let (id, url) = (self.tabs[index].id, self.tabs[index].url.clone());
        if self.tabs[index].asleep {
            match &self.tabs[index].view {
                Some(view) => {
                    let _ = view.load_url(&url);
                }
                None => self.tabs[index].view = Some(self.create_view(id, &url)),
            }
            self.tabs[index].asleep = false;
        }
        if let Some(view) = &self.tabs[index].view {
            let _ = view.set_bounds(bounds);
            let _ = view.set_visible(true);
            let _ = view.focus();
        }
        self.push_state();
    }

    fn close_tab(&mut self, id: usize) {
        if self.tabs.len() <= 1 {
            return;
        }
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let active_id = self.tabs[self.active].id;
        self.tabs.remove(index);
        if active_id == id {
            self.active = usize::MAX;
            self.activate(index.min(self.tabs.len() - 1));
        } else {
            self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(0);
            self.push_state();
        }
    }

    fn push_state(&self) {
        if !self.toolbar_ready {
            return;
        }
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .map(|t| json!({ "id": t.id, "title": t.title, "url": t.url, "sleeping": t.asleep }))
            .collect();
        let script = format!("window.setTabs({}, {})", json!(tabs), self.active);
        let _ = self.toolbar.evaluate_script(&script);
    }

    fn push_bookmarks(&self) {
        let script = format!("window.setBookmarks({})", json!(self.bookmarks));
        let _ = self.toolbar.evaluate_script(&script);
    }

    fn push_status(&self, text: &str) {
        let script = format!("window.setStatus({})", json!(text));
        let _ = self.toolbar.evaluate_script(&script);
    }

    fn toggle_bookmark(&mut self) {
        let tab = &self.tabs[self.active];
        if let Some(index) = self.bookmarks.iter().position(|b| b.url == tab.url) {
            self.bookmarks.remove(index);
        } else {
            let title = if tab.title.is_empty() { tab.url.clone() } else { tab.title.clone() };
            self.bookmarks.push(Bookmark { title, url: tab.url.clone() });
        }
        bookmarks::save(&self.bookmarks);
        self.push_bookmarks();
    }

    fn import_bookmarks(&mut self) {
        let result = bookmarks::import_all(&mut self.bookmarks);
        let mut message = if result.browsers.is_empty() {
            "No bookmarks found".to_string()
        } else {
            bookmarks::save(&self.bookmarks);
            self.push_bookmarks();
            format!("Imported {} new from {}", result.added, result.browsers.join(", "))
        };
        if !result.blocked.is_empty() {
            message.push_str(&format!(". {} needs Full Disk Access", result.blocked.join(", ")));
        }
        self.push_status(&message);
    }

    fn navigate(&mut self, url: String) {
        if let Some(view) = &self.tabs[self.active].view {
            let _ = view.load_url(&url);
        }
        self.tabs[self.active].url = url;
        self.push_state();
    }

    fn handle_ipc(&mut self, message: &str) {
        let active_view = self.tabs[self.active].view.as_ref();
        match message {
            "ready" => {
                self.toolbar_ready = true;
                self.push_state();
                self.push_bookmarks();
            }
            "new" => self.open_tab(HOME_URL),
            "star" => self.toggle_bookmark(),
            "import" => self.import_bookmarks(),
            "back" => {
                let _ = active_view.map(|v| v.evaluate_script("history.back()"));
            }
            "forward" => {
                let _ = active_view.map(|v| v.evaluate_script("history.forward()"));
            }
            "reload" => {
                let _ = active_view.map(|v| v.reload());
            }
            other => {
                if let Some(input) = other.strip_prefix("go:") {
                    self.navigate(resolve_input(input));
                } else if let Some(url) = other.strip_prefix("open:") {
                    self.navigate(url.to_string());
                } else if let Some(url) = other.strip_prefix("unbookmark:") {
                    self.bookmarks.retain(|b| b.url != url);
                    bookmarks::save(&self.bookmarks);
                    self.push_bookmarks();
                } else if let Some(id) = other.strip_prefix("switch:").and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                        self.activate(index);
                    }
                } else if let Some(id) = other.strip_prefix("close:").and_then(|s| s.parse::<usize>().ok()) {
                    self.close_tab(id);
                }
            }
        }
    }

    fn sleep_idle_tabs(&mut self) {
        let mut changed = false;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if index != self.active && !tab.asleep && tab.last_active.elapsed() >= SLEEP_AFTER {
                tab.asleep = true;
                if let Some(view) = &tab.view {
                    let _ = view.load_url("about:blank");
                }
                if !cfg!(target_os = "macos") {
                    tab.view = None;
                }
                changed = true;
            }
        }
        if changed {
            self.push_state();
        }
    }

    fn resize(&self) {
        let _ = self.toolbar.set_bounds(self.toolbar_bounds());
        let bounds = self.content_bounds();
        for view in self.tabs.iter().filter_map(|t| t.view.as_ref()) {
            let _ = view.set_bounds(bounds);
        }
    }
}

fn main() {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("cheetah")
        .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
        .build(&event_loop)
        .expect("failed to create window");

    let ipc_proxy = proxy.clone();
    let width = window.inner_size().to_logical::<f64>(window.scale_factor()).width;
    let toolbar = WebViewBuilder::new()
        .with_html(UI_HTML)
        .with_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(width, TOOLBAR_HEIGHT).into(),
        })
        .with_ipc_handler(move |request| {
            let _ = ipc_proxy.send_event(UserEvent::Ipc(request.body().clone()));
        })
        .build_as_child(&window)
        .expect("failed to create toolbar");

    let mut app = App {
        window,
        proxy,
        toolbar,
        toolbar_ready: false,
        tabs: Vec::new(),
        active: 0,
        next_id: 0,
        bookmarks: bookmarks::load(),
    };
    let mut urls: Vec<String> = std::env::args().skip(1).map(|arg| resolve_input(&arg)).collect();
    if urls.is_empty() {
        urls.push(HOME_URL.to_string());
    }
    for url in &urls {
        app.open_tab(url);
    }

    let mut last_sleep_check = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        if last_sleep_check.elapsed() >= SLEEP_CHECK_INTERVAL {
            last_sleep_check = Instant::now();
            app.sleep_idle_tabs();
        }
        *control_flow = ControlFlow::WaitUntil(last_sleep_check + SLEEP_CHECK_INTERVAL);
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => app.resize(),
            Event::UserEvent(UserEvent::Ipc(message)) => app.handle_ipc(&message),
            Event::UserEvent(UserEvent::Title(id, title)) => {
                if let Some(tab) = app.tabs.iter_mut().find(|t| t.id == id && !t.asleep) {
                    tab.title = title;
                    app.push_state();
                }
            }
            Event::UserEvent(UserEvent::Address(id, url)) => {
                if let Some(tab) = app.tabs.iter_mut().find(|t| t.id == id && !t.asleep) {
                    tab.url = url;
                    app.push_state();
                }
            }
            _ => {}
        }
    });
}
