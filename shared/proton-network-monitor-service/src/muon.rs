use crate::{ConnectionMonitor, RequestNetworkStatus};
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::{ProtonRequest, ProtonResponse};

impl ConnectionMonitor {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> muon::Result<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        match inner.send(req).await {
            Ok(resp) => {
                self.on_recv_ok(&resp);
                Ok(resp)
            }

            Err(error) => {
                self.on_recv_err(&error);
                Err(error)
            }
        }
    }

    fn on_recv_err(&self, error: &muon::Error) {
        use muon::error::ErrorKind;

        match error.kind() {
            ErrorKind::Tls | ErrorKind::Resolve | ErrorKind::Dial | ErrorKind::Send => {
                self.update_request_status(RequestNetworkStatus::Offline);
            }

            ErrorKind::Connect => {
                self.update_request_status(RequestNetworkStatus::ServerUnreachable);
            }

            _ => {}
        }
    }

    fn on_recv_ok(&self, resp: &ProtonResponse) {
        if resp.is(429) || resp.status().is_server_error() {
            self.update_request_status(RequestNetworkStatus::ServerUnreachable);
        } else {
            self.update_request_status(RequestNetworkStatus::Offline);
        }
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for ConnectionMonitor {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, muon::Result<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
