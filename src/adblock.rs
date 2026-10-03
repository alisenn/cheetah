const TRACKER_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "google-analytics.com",
    "googletagservices.com",
    "adservice.google.com",
    "adnxs.com",
    "adsrvr.org",
    "amazon-adsystem.com",
    "criteo.com",
    "criteo.net",
    "taboola.com",
    "outbrain.com",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "casalemedia.com",
    "moatads.com",
    "scorecardresearch.com",
    "quantserve.com",
    "hotjar.com",
    "mixpanel.com",
    "segment.io",
    "adform.net",
    "smartadserver.com",
    "teads.tv",
    "media.net",
    "advertising.com",
    "yieldmo.com",
    "sharethrough.com",
    "3lift.com",
    "bidswitch.net",
    "contextweb.com",
    "lijit.com",
    "krxd.net",
    "bluekai.com",
    "demdex.net",
    "everesttech.net",
    "serving-sys.com",
    "zedo.com",
    "connect.facebook.net",
    "ads.twitter.com",
    "analytics.twitter.com",
    "ads.linkedin.com",
    "px.ads.linkedin.com",
    "ads.pinterest.com",
    "analytics.tiktok.com",
    "ads.yahoo.com",
    "adsystem.com",
    "chartbeat.net",
    "newrelic.com",
];

const HIDDEN_SELECTORS: &str = ".adsbygoogle, ins.adsbygoogle, [id^='google_ads_iframe'], [id^='div-gpt-ad'], .ad-banner, .ad-container, .advert, .advertisement, .sponsored-ad";

pub fn rules_json() -> String {
    let mut rules: Vec<String> = TRACKER_DOMAINS
        .iter()
        .map(|domain| {
            let pattern = domain.replace('.', "\\\\.");
            format!(
                r#"{{"trigger":{{"url-filter":"^https?://([^/]+\\.)?{pattern}[:/]","load-type":["third-party"]}},"action":{{"type":"block"}}}}"#
            )
        })
        .collect();
    rules.push(format!(
        r#"{{"trigger":{{"url-filter":".*"}},"action":{{"type":"css-display-none","selector":"{HIDDEN_SELECTORS}"}}}}"#
    ));
    format!("[{}]", rules.join(","))
}

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::RefCell;

    use block2::RcBlock;
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_foundation::{NSError, NSString};
    use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore};
    use wry::{WebView, WebViewExtMacOS};

    thread_local! {
        static RULES: RefCell<Option<Retained<WKContentRuleList>>> = const { RefCell::new(None) };
    }

    pub fn compile(on_done: impl Fn() + 'static) {
        let mtm = MainThreadMarker::new().expect("rules must be compiled on the main thread");
        // SAFETY: called on the main thread with valid arguments
        let Some(store) = (unsafe { WKContentRuleListStore::defaultStore(mtm) }) else {
            on_done();
            return;
        };
        let block = RcBlock::new(move |list: *mut WKContentRuleList, _error: *mut NSError| {
            // SAFETY: WebKit passes either null or a valid rule list
            if let Some(list) = unsafe { Retained::retain(list) } {
                RULES.with(|rules| *rules.borrow_mut() = Some(list));
            }
            on_done();
        });
        let identifier = NSString::from_str("cheetah-adblock-v1");
        let json = NSString::from_str(&super::rules_json());
        // SAFETY: all arguments are valid and the block outlives the call
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&identifier),
                Some(&json),
                Some(&block),
            );
        }
    }

    pub fn apply(view: &WebView) {
        RULES.with(|rules| {
            if let Some(list) = rules.borrow().as_ref() {
                // SAFETY: the web view and its configuration are valid on the main thread
                unsafe { view.webview().configuration().userContentController().addContentRuleList(list) };
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use wry::WebView;

    pub fn compile(on_done: impl Fn() + 'static) {
        on_done();
    }

    pub fn apply(_view: &WebView) {}
}

pub use platform::{apply, compile};
