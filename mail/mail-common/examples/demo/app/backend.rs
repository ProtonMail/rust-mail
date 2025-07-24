use crate::app::events::{EndChallengeEvent, NewChallengeEvent, Proxy, UserEvent};
use crate::notifier::HvMessage;
use anyhow::Result;
use cfg_if::cfg_if;
use futures::FutureExt;
use proton_core_api::verification::{ChallengeLoader, ChallengeResponse};
use std::borrow::Cow;
use std::sync::mpsc;
use tao::dpi::{LogicalPosition, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use tokio::runtime::Handle;
use tracing::{error, trace};
use url::Url;
use wry::http::{Request, Response, StatusCode};
use wry::{Rect, WebView, WebViewBuilder, WebViewId};

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

        const INIT: &str = r"
            window.parent.postMessage = function(payload, origin) {
                window.ipc.postMessage(payload);
            };
        ";

        let on_req = move |_: WebViewId, req: Request<Vec<u8>>| -> Response<Cow<'static, [u8]>> {
            let url = Url::parse(&req.uri().to_string()).unwrap();
            let host = url.host_str().unwrap().to_owned();
            let base = format!("{}://{host}", url.scheme());

            let ((tx, rx), loader) = (mpsc::channel(), loader.clone());

            Handle::current().spawn(async move {
                let mut query = Vec::new();

                for (k, v) in url.query_pairs() {
                    let k = k.into_owned();
                    let v = v.into_owned();

                    query.push((k, Some(v)));
                }

                let mut header = Vec::new();

                for (k, v) in req.headers() {
                    let k = k.as_str().to_owned();
                    let v = v.to_str().unwrap_or_default();

                    header.push((k, rust_to_https(v)));
                }

                let _ = loader
                    .get(rust_to_https(&base), url.path(), query, header)
                    .map(|res| tx.send(res))
                    .await;
            });

            match rx.recv() {
                Ok(Ok(res)) => {
                    let mut b = Response::builder().status(res.status);

                    for (k, v) in res.headers {
                        if k.eq_ignore_ascii_case("content-security-policy") {
                            b = b.header(k, csp_to_rust(v, &host));
                        } else {
                            b = b.header(k, https_to_rust(v));
                        }
                    }

                    b.body(res.contents.to_vec().into()).unwrap()
                }

                Ok(Err(err)) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(format!("{err:?}").into_bytes().into())
                    .unwrap(),

                Err(_) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body("Request timeout".as_bytes().into())
                    .unwrap(),
            }
        };

        let on_ipc = move |req: Request<String>| {
            let res = match serde_json::from_str(req.body()) {
                Ok(HvMessage::HumanVerificationSuccess { ttype, token }) => EndChallengeEvent {
                    response: ChallengeResponse::Success { token, ttype },
                    tx: event.tx.clone(),
                },

                Ok(HvMessage::Error { .. }) => EndChallengeEvent {
                    response: ChallengeResponse::Failure,
                    tx: event.tx.clone(),
                },

                Ok(HvMessage::Close) => EndChallengeEvent {
                    response: ChallengeResponse::Cancelled,
                    tx: event.tx.clone(),
                },

                Ok(event) => {
                    trace!(?event, "unhandled IPC event");
                    return;
                }

                Err(e) => {
                    error!(?e, "invalid IPC event");
                    return;
                }
            };

            if proxy.send_event(UserEvent::EndChallenge(res)).is_err() {
                error!("failed to send event");
            }
        };

        let builder = WebViewBuilder::new()
            .with_initialization_script(INIT)
            .with_url(https_to_rust(event.payload.web_url))
            .with_custom_protocol("rust".to_owned(), on_req)
            .with_ipc_handler(on_ipc);

        self.webview = new_webview(&self.window, builder).ok();

        self.window.set_visible(true);
    }

    fn on_end_challenge_event(&mut self, event: EndChallengeEvent, _: &mut ControlFlow) {
        self.webview = None;

        self.window.set_visible(false);

        if event.tx.send(event.response).is_err() {
            error!("failed to send response");
        }
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

fn new_bounds(window: &Window, size: PhysicalSize<u32>) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: size.to_logical::<u32>(window.scale_factor()).into(),
    }
}

fn rust_to_https(url: impl AsRef<str>) -> String {
    url.as_ref().replace("rust://", "https://")
}

fn https_to_rust(url: impl AsRef<str>) -> String {
    url.as_ref().replace("https://", "rust://")
}

fn csp_to_rust(mut csp: String, host: &str) -> String {
    const FRAME_SRC: &str = "frame-src 'self' blob: ";

    if let Some(pos) = csp.find(FRAME_SRC) {
        csp.insert_str(pos + FRAME_SRC.len(), &format!("rust://{host} "));
    }

    for directive in [
        "script-src",
        "style-src",
        "img-src",
        "frame-src",
        "connect-src",
        "font-src",
        "media-src",
    ] {
        if let Some(pos) = csp.find(directive) {
            csp.insert_str(pos + directive.len(), " rust:");
        }
    }

    csp
}
