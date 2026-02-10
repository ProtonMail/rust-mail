use crate::consts::CoreBundle::HumanVerificationRequired;
use crate::services::proton::HumanVerificationChallenge;
use crate::services::proton::common::ApiErrorInfo;
use crate::verification::notifier::ChallengeResponse;
use crate::verification::{ChallengePayload, ChallengeServer, DynChallengeNotifier};
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::util::ProtonRequestExt;
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult, Status};
use tracing::{debug, error, info, trace, warn};

/// A type that wraps a [`ChallengeObserver`] and to implement the [`SenderLayer`] trait.
pub struct ChallengeNotifierLayer {
    notifier: DynChallengeNotifier,
}

#[allow(clippy::similar_names)]
impl ChallengeNotifierLayer {
    #[must_use]
    pub fn new(notifier: DynChallengeNotifier) -> Self {
        Self { notifier }
    }

    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let res = match req.clone().send_with(inner).await? {
            res if res.is(Status::UNPROCESSABLE_ENTITY) => res,
            res => return Ok(res),
        };

        let Ok(err): MuonResult<ApiErrorInfo> = res.body_json() else {
            error!("failed to parse API error response");
            return Ok(res);
        };

        if err.code != HumanVerificationRequired as u32 {
            trace!("human verification not required");
            return Ok(res);
        }

        let ctl = req.get_timeout_ctl();

        if let Some(ctl) = &ctl {
            debug!("pausing timeout during human verification");
            ctl.pause();
        }

        let res = self.on_challenge(inner, req, res, err).await?;

        if let Some(ctl) = &ctl {
            debug!("resuming timeout after human verification");
            ctl.resume();
        }

        Ok(res)
    }

    async fn on_challenge<S>(
        &self,
        inner: &S,
        req: ProtonRequest,
        res: ProtonResponse,
        err: ApiErrorInfo,
    ) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        warn!(?err, "human verification required");

        let Some(details) = err.details else {
            error!("missing human verification challenge details");
            return Ok(res);
        };

        let Ok(challenge) = HumanVerificationChallenge::from_value(details) else {
            error!("failed to parse human verification challenge details");
            return Ok(res);
        };

        let Ok(payload) = ChallengePayload::try_from(challenge) else {
            error!("failed to convert human verification challenge to payload");
            return Ok(res);
        };

        let Some((token, ttype)) = self
            .notify(ChallengeServer::new(res.server(), res.name()), payload)
            .await
        else {
            error!("no challenge response");
            return Ok(res);
        };

        req.header(("x-pm-human-verification-token", token))
            .header(("x-pm-human-verification-token-type", ttype))
            .send_with(inner)
            .await
    }

    async fn notify(
        &self,
        server: ChallengeServer,
        payload: ChallengePayload,
    ) -> Option<(String, String)> {
        match self.notifier.on_challenge(server, payload).await {
            ChallengeResponse::Success { token, ttype } => {
                info!("challenge succeeded");
                Some((token, ttype))
            }

            ChallengeResponse::Failure => {
                error!("challenge failed");
                None
            }

            ChallengeResponse::Cancelled => {
                warn!("challenge cancelled");
                None
            }
        }
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for ChallengeNotifierLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
