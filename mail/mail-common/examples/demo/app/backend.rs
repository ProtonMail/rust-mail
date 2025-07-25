use crate::app::events::{EndChallengeEvent, NewChallengeEvent, Proxy, UserEvent};
use crate::notifier::HvMessage;
use anyhow::Result;
use cfg_if::cfg_if;
use futures::FutureExt;
use itertools::Itertools;
use proton_core_api::services::proton::muon::util::IntoIterExt;
use proton_core_api::verification::{ChallengeLoader, ChallengeResponse};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::mpsc;
use tao::dpi::{LogicalPosition, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use tokio::runtime::Handle;
use tracing::{error, trace};
use url::Url;
use wry::http::response::Builder;
use wry::http::{Request, Response};
use wry::{Rect, WebView, WebViewBuilder};

pub struct App {
    loader: ChallengeLoader,
    proxy: EventLoopProxy<UserEvent>,
    window: Window,
    webview: Option<WebView>,
}

impl Proxy for EventLoopProxy<UserEvent> {
    fn send_event(&self, event: UserEvent) -> Result<()> {
        Ok(self.send_event(event)?)
    }
}

impl App {
    pub fn new(events: &EventLoop<UserEvent>, loader: ChallengeLoader) -> Result<Self> {
        Ok(Self {
            loader,
            proxy: events.create_proxy(),
            window: new_window(events)?,
            webview: None,
        })
    }

    pub fn run(mut self, events: EventLoop<UserEvent>) -> ! {
        events.run(move |event, _, ctl| {
            *ctl = ControlFlow::Wait;

            match event {
                Event::WindowEvent { event, .. } => {
                    self.on_window_event(event, ctl);
                }

                Event::UserEvent(event) => {
                    self.on_user_event(event, ctl);
                }

                event => trace!(?event),
            }
        })
    }

    fn on_window_event(&mut self, event: WindowEvent, ctl: &mut ControlFlow) {
        match event {
            WindowEvent::Resized(size) => {
                self.on_window_resized_event(size, ctl);
            }

            event => trace!(?event),
        };
    }

    fn on_window_resized_event(&mut self, size: PhysicalSize<u32>, _: &mut ControlFlow) {
        if let Some(webview) = &mut self.webview {
            webview.set_bounds(new_bounds(&self.window, size)).unwrap();
        }
    }

    fn on_user_event(&mut self, event: UserEvent, ctl: &mut ControlFlow) {
        match event {
            UserEvent::Exit => {
                *ctl = ControlFlow::Exit;
            }

            UserEvent::NewChallenge(event) => {
                self.on_new_challenge_event(event, ctl);
            }

            UserEvent::EndChallenge(event) => {
                self.on_end_challenge_event(event, ctl);
            }
        }
    }

    fn on_new_challenge_event(&mut self, event: NewChallengeEvent, _: &mut ControlFlow) {
        let proxy = self.proxy.clone();
        let loader = self.loader.clone();
        let builder = new_webview_builder(loader, event, proxy);

        self.webview = new_webview(&self.window, builder).ok();
        self.window.set_visible(true);
    }

    fn on_end_challenge_event(&mut self, event: EndChallengeEvent, _: &mut ControlFlow) {
        self.webview = None;
        self.window.set_visible(false);

        if let Err(e) = event.tx.send(event.response) {
            error!(?e, "failed to send response");
        }
    }
}

fn new_bounds(window: &Window, size: PhysicalSize<u32>) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: size.to_logical::<u32>(window.scale_factor()).into(),
    }
}

fn new_window(events: &EventLoop<UserEvent>) -> std::result::Result<Window, tao::error::OsError> {
    let builder = WindowBuilder::new().with_visible(false);

    cfg_if! {
        if #[cfg(target_os = "linux")] {
            use tao::platform::unix::WindowBuilderExtUnix;

            builder.with_default_vbox(false).build(events)
        } else {
            builder.build(events)
        }
    }
}

fn new_webview(window: &Window, builder: WebViewBuilder) -> Result<WebView> {
    cfg_if! {
        if #[cfg(target_os = "linux")] {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;

            Ok(builder.build_gtk(window.gtk_window())?)
        } else {
            Ok(builder.build(window)?)
        }
    }
}

const WEBVIEW_INIT: &str = r"
    window.parent.postMessage = function(payload, origin) {
        window.ipc.postMessage(payload);
    };
";

fn new_webview_builder<'a>(
    c: ChallengeLoader,
    event: NewChallengeEvent,
    proxy: EventLoopProxy<UserEvent>,
) -> WebViewBuilder<'a> {
    WebViewBuilder::new()
        .with_initialization_script(WEBVIEW_INIT)
        .with_url(to_rust(event.payload.web_url))
        .with_custom_protocol("rust".to_owned(), move |_, req| on_web_req(c.clone(), req))
        .with_ipc_handler(move |req| on_ipc_req(&proxy, req, event.tx.clone()))
}

fn on_web_req(c: ChallengeLoader, req: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let (tx, rx) = mpsc::channel();

    Handle::current().spawn(async move {
        let url = Url::parse(&req.uri().to_string()).unwrap();
        let base = to_https(url.base());
        let path = url.path();
        let query = url.get_query();
        let header = req.get_header();

        c.get(base, path, query, header)
            .map(|res| tx.send(res))
            .await
    });

    let res = rx.recv().unwrap().unwrap();

    Response::builder()
        .status(res.status)
        .headers(res.headers.into_iter().map(header_to_rust))
        .body(res.contents.to_vec().into())
        .unwrap()
}

fn on_ipc_req(
    proxy: &EventLoopProxy<UserEvent>,
    req: Request<String>,
    tx: mpsc::Sender<ChallengeResponse>,
) {
    let response = match serde_json::from_str(req.body()) {
        Ok(HvMessage::Success { ttype, token }) => ChallengeResponse::Success { token, ttype },
        Ok(HvMessage::Error { .. }) => ChallengeResponse::Failure,
        Ok(HvMessage::Close) => ChallengeResponse::Cancelled,
        _ => return,
    };

    if let Err(e) = proxy.send_event(UserEvent::EndChallenge(EndChallengeEvent { response, tx })) {
        error!(?e, "failed to send event");
    }
}

fn to_https(v: impl AsRef<str>) -> String {
    v.as_ref().replace("rust://", "https://")
}

fn to_rust(v: impl AsRef<str>) -> String {
    v.as_ref().replace("https://", "rust://")
}

fn header_to_rust((k, v): (String, String)) -> (String, String) {
    if k.eq_ignore_ascii_case("content-security-policy") {
        (k, csp_to_rust(&v))
    } else {
        (k, v)
    }
}

fn csp_to_rust(csp: &str) -> String {
    let mut csp = parse_csp(csp);

    for v in csp.values_mut() {
        v.push("rust:");
    }

    render_csp(&csp)
}

fn parse_csp(v: &str) -> HashMap<&str, Vec<&str>> {
    let mut csp = HashMap::new();

    for line in v.split(';') {
        if let Some((head, tail)) = line.split_whitespace().into_head_tail() {
            csp.insert(head, tail.collect_vec());
        }
    }

    csp
}

fn render_csp(csp: &HashMap<&str, Vec<&str>>) -> String {
    csp.iter()
        .map(|(k, v)| (k, v.join(" ")))
        .map(|(k, v)| format!("{k} {v}"))
        .join(";")
}

trait RequestExt {
    fn get_header(&self) -> impl Iterator<Item = (String, String)>;
}

impl<T> RequestExt for Request<T> {
    fn get_header(&self) -> impl Iterator<Item = (String, String)> {
        self.headers()
            .into_iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default()))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
    }
}

trait UrlExt {
    fn base(&self) -> Self;
    fn get_query(&self) -> impl Iterator<Item = (String, Option<String>)>;
}

impl UrlExt for Url {
    fn base(&self) -> Self {
        let scheme = self.scheme();
        let host = self.host_str().unwrap();
        format!("{scheme}://{host}").parse().unwrap()
    }

    fn get_query(&self) -> impl Iterator<Item = (String, Option<String>)> {
        self.query_pairs().into_owned().map(|(k, v)| (k, Some(v)))
    }
}

trait BuilderExt {
    fn headers(self, headers: impl IntoIterator<Item = (String, String)>) -> Self;
}

impl BuilderExt for Builder {
    fn headers(mut self, headers: impl IntoIterator<Item = (String, String)>) -> Self {
        for (k, v) in headers {
            self = self.header(k, v);
        }

        self
    }
}
