use crate::app::events::{EndChallengeEvent, NewChallengeEvent, Proxy, UserEvent};
use crate::notifier::HvMessage;
use anyhow::Result;
use cfg_if::cfg_if;
use proton_core_api::verification::ChallengeResponse;
use tao::dpi::{LogicalPosition, PhysicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
use tao::window::{Window, WindowBuilder};
use wry::http::Request;
use wry::{Rect, WebView, WebViewBuilder};

pub struct App {
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
    pub fn new(events: &EventLoop<UserEvent>) -> Result<Self> {
        Ok(Self {
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

        const INIT: &str = r"
            window.parent.postMessage = function(payload, origin) {
                window.ipc.postMessage(payload);
            };
        ";

        let handler = move |req: Request<String>| {
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
            .with_url(event.payload.web_url)
            .with_ipc_handler(handler);

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
