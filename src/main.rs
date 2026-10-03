use std::time::{Duration, Instant};

use serde_json::json;
use tao::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

const UI_HTML: &str = include_str!("ui.html");
const ANA_SAYFA: &str = "https://duckduckgo.com";
const ARAC_CUBUGU_YUKSEKLIK: f64 = 80.0;
const UYKU_SURESI: Duration = Duration::from_secs(120);
const KONTROL_ARALIGI: Duration = Duration::from_secs(15);

/// Webview işleyicilerinden event loop'a giden olaylar
enum UserEvent {
    Ipc(String),
    Baslik(usize, String),
    Adres(usize, String),
}

struct Sekme {
    id: usize,
    url: String,
    baslik: String,
    /// None ise sekme uyuyor demektir
    view: Option<WebView>,
    son_aktif: Instant,
}

struct Uygulama {
    pencere: Window,
    proxy: EventLoopProxy<UserEvent>,
    arac_cubugu: WebView,
    arac_hazir: bool,
    sekmeler: Vec<Sekme>,
    aktif: usize,
    sonraki_id: usize,
}

/// Girilen metni URL'e çevirir; URL değilse DuckDuckGo'da arar
fn url_yap(girdi: &str) -> String {
    let g = girdi.trim();
    if g.contains("://") {
        g.to_string()
    } else if !g.contains(char::is_whitespace) && g.contains('.') {
        format!("https://{g}")
    } else {
        let mut q = String::new();
        for b in g.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => q.push(b as char),
                _ => q.push_str(&format!("%{b:02X}")),
            }
        }
        format!("https://duckduckgo.com/?q={q}")
    }
}

impl Uygulama {
    fn icerik_alani(&self) -> Rect {
        let boyut = self.pencere.inner_size().to_logical::<f64>(self.pencere.scale_factor());
        Rect {
            position: LogicalPosition::new(0.0, ARAC_CUBUGU_YUKSEKLIK).into(),
            size: LogicalSize::new(boyut.width, (boyut.height - ARAC_CUBUGU_YUKSEKLIK).max(1.0)).into(),
        }
    }

    fn arac_alani(&self) -> Rect {
        let boyut = self.pencere.inner_size().to_logical::<f64>(self.pencere.scale_factor());
        Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(boyut.width, ARAC_CUBUGU_YUKSEKLIK).into(),
        }
    }

    fn webview_olustur(&self, id: usize, url: &str) -> WebView {
        let (p1, p2) = (self.proxy.clone(), self.proxy.clone());
        WebViewBuilder::new()
            .with_url(url)
            .with_bounds(self.icerik_alani())
            .with_document_title_changed_handler(move |b| {
                let _ = p1.send_event(UserEvent::Baslik(id, b));
            })
            .with_navigation_handler(move |u| {
                let _ = p2.send_event(UserEvent::Adres(id, u));
                true
            })
            .build_as_child(&self.pencere)
            .expect("sekme webview'ı oluşturulamadı")
    }

    fn sekme_ac(&mut self, url: &str) {
        let id = self.sonraki_id;
        self.sonraki_id += 1;
        let view = self.webview_olustur(id, url);
        self.sekmeler.push(Sekme {
            id,
            url: url.to_string(),
            baslik: String::new(),
            view: Some(view),
            son_aktif: Instant::now(),
        });
        self.degistir(self.sekmeler.len() - 1);
    }

    /// Aktif sekmeyi değiştirir; uyuyorsa webview'ı yeniden oluşturur
    fn degistir(&mut self, yeni: usize) {
        if let Some(eski) = self.sekmeler.get_mut(self.aktif)
            && self.aktif != yeni
        {
            eski.son_aktif = Instant::now();
            if let Some(v) = &eski.view {
                let _ = v.set_visible(false);
            }
        }
        self.aktif = yeni;
        let alan = self.icerik_alani();
        let (id, url) = (self.sekmeler[yeni].id, self.sekmeler[yeni].url.clone());
        if self.sekmeler[yeni].view.is_none() {
            let v = self.webview_olustur(id, &url);
            self.sekmeler[yeni].view = Some(v);
        }
        if let Some(v) = &self.sekmeler[yeni].view {
            let _ = v.set_bounds(alan);
            let _ = v.set_visible(true);
            let _ = v.focus();
        }
        self.arayuzu_guncelle();
    }

    fn sekme_kapat(&mut self, id: usize) {
        if self.sekmeler.len() <= 1 {
            return;
        }
        let Some(i) = self.sekmeler.iter().position(|s| s.id == id) else {
            return;
        };
        let aktif_id = self.sekmeler[self.aktif].id;
        self.sekmeler.remove(i);
        if aktif_id == id {
            // Kapanan aktifse komşu sekmeye geç
            self.aktif = usize::MAX;
            self.degistir(i.min(self.sekmeler.len() - 1));
        } else {
            self.aktif = self.sekmeler.iter().position(|s| s.id == aktif_id).unwrap_or(0);
            self.arayuzu_guncelle();
        }
    }

    fn arayuzu_guncelle(&self) {
        if !self.arac_hazir {
            return;
        }
        let liste: Vec<_> = self
            .sekmeler
            .iter()
            .map(|s| json!({ "id": s.id, "title": s.baslik, "url": s.url, "sleeping": s.view.is_none() }))
            .collect();
        let js = format!("window.setTabs({}, {})", json!(liste), self.aktif);
        let _ = self.arac_cubugu.evaluate_script(&js);
    }

    fn ipc(&mut self, mesaj: &str) {
        let aktif_view = self.sekmeler[self.aktif].view.as_ref();
        match mesaj {
            "ready" => {
                self.arac_hazir = true;
                self.arayuzu_guncelle();
            }
            "new" => self.sekme_ac(ANA_SAYFA),
            "back" => {
                let _ = aktif_view.map(|v| v.evaluate_script("history.back()"));
            }
            "forward" => {
                let _ = aktif_view.map(|v| v.evaluate_script("history.forward()"));
            }
            "reload" => {
                let _ = aktif_view.map(|v| v.reload());
            }
            m => {
                if let Some(girdi) = m.strip_prefix("go:") {
                    let url = url_yap(girdi);
                    if let Some(v) = aktif_view {
                        let _ = v.load_url(&url);
                    }
                    self.sekmeler[self.aktif].url = url;
                    self.arayuzu_guncelle();
                } else if let Some(id) = m.strip_prefix("switch:").and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(i) = self.sekmeler.iter().position(|s| s.id == id) {
                        self.degistir(i);
                    }
                } else if let Some(id) = m.strip_prefix("close:").and_then(|s| s.parse::<usize>().ok()) {
                    self.sekme_kapat(id);
                }
            }
        }
    }

    /// Uzun süredir boşta duran arka plan sekmelerinin webview'ını yok eder
    fn uyut(&mut self) {
        let mut degisti = false;
        for (i, s) in self.sekmeler.iter_mut().enumerate() {
            if i != self.aktif && s.view.is_some() && s.son_aktif.elapsed() >= UYKU_SURESI {
                s.view = None;
                degisti = true;
            }
        }
        if degisti {
            self.arayuzu_guncelle();
        }
    }

    fn yeniden_boyutlandir(&self) {
        let _ = self.arac_cubugu.set_bounds(self.arac_alani());
        let alan = self.icerik_alani();
        for v in self.sekmeler.iter().filter_map(|s| s.view.as_ref()) {
            let _ = v.set_bounds(alan);
        }
    }
}

fn main() {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let pencere = WindowBuilder::new()
        .with_title("wolf")
        .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
        .build(&event_loop)
        .expect("pencere oluşturulamadı");

    let ipc_proxy = proxy.clone();
    let boyut = pencere.inner_size().to_logical::<f64>(pencere.scale_factor());
    let arac_cubugu = WebViewBuilder::new()
        .with_html(UI_HTML)
        .with_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(boyut.width, ARAC_CUBUGU_YUKSEKLIK).into(),
        })
        .with_ipc_handler(move |istek| {
            let _ = ipc_proxy.send_event(UserEvent::Ipc(istek.body().clone()));
        })
        .build_as_child(&pencere)
        .expect("araç çubuğu oluşturulamadı");

    let mut app = Uygulama {
        pencere,
        proxy,
        arac_cubugu,
        arac_hazir: false,
        sekmeler: Vec::new(),
        aktif: 0,
        sonraki_id: 0,
    };
    app.sekme_ac(ANA_SAYFA);

    event_loop.run(move |olay, _, kontrol| {
        *kontrol = ControlFlow::WaitUntil(Instant::now() + KONTROL_ARALIGI);
        match olay {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => app.uyut(),
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *kontrol = ControlFlow::Exit,
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => app.yeniden_boyutlandir(),
            Event::UserEvent(UserEvent::Ipc(m)) => app.ipc(&m),
            Event::UserEvent(UserEvent::Baslik(id, b)) => {
                if let Some(s) = app.sekmeler.iter_mut().find(|s| s.id == id) {
                    s.baslik = b;
                    app.arayuzu_guncelle();
                }
            }
            Event::UserEvent(UserEvent::Adres(id, u)) => {
                if let Some(s) = app.sekmeler.iter_mut().find(|s| s.id == id) {
                    s.url = u;
                    app.arayuzu_guncelle();
                }
            }
            _ => {}
        }
    });
}
