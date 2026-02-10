use anyhow::Result;
use futures_timer::Delay;
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::test::server::Server;
use muon::util::{DurationExt, ProtonRequestExt};
use muon::{Error, ProtonRequest, ProtonResponse, GET};
use std::sync::Arc;

#[muon::test]
async fn test_timeout_total(s: Arc<Server>) -> Result<()> {
    let c = s.client();

    // Set the total timeout to 0 seconds: fail immediately.
    if let Ok(t) = GET!("/tests/ping").allowed_time(0.s()).send_with(&c).await {
        panic!("expected error, got: {t:?}");
    }

    // Set the total timeout to 999 seconds: succeed.
    if let Err(e) = GET!("/tests/ping")
        .allowed_time(999.s())
        .send_with(&c)
        .await
    {
        panic!("expected success, got: {e:?}");
    }

    Ok(())
}

#[muon::test]
async fn test_timeout_pause_resume(s: Arc<Server>) -> Result<()> {
    struct PauseLayer;

    impl SenderLayer<ProtonRequest, ProtonResponse> for PauseLayer {
        fn on_send<'a>(
            &'a self,
            inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
            req: ProtonRequest,
        ) -> BoxFut<'a, Result<ProtonResponse, Error>> {
            Box::pin(async move {
                let ctl = req.get_timeout_ctl();

                if let Some(ctl) = &ctl {
                    ctl.pause();
                }

                let res = inner.send(req).await?;

                if let Some(ctl) = &ctl {
                    ctl.resume();
                }

                Ok(res)
            })
        }
    }

    struct DelayLayer;

    impl SenderLayer<ProtonRequest, ProtonResponse> for DelayLayer {
        fn on_send<'a>(
            &'a self,
            inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
            req: ProtonRequest,
        ) -> BoxFut<'a, Result<ProtonResponse, Error>> {
            Box::pin(async move {
                Delay::new(2.s()).await;
                inner.send(req).await
            })
        }
    }

    // Delay takes longer than total time: should fail.
    if let Ok(t) = s
        .builder()
        .layer_back(DelayLayer)
        .build()?
        .send(GET!("/tests/ping").allowed_time(1.s()))
        .await
    {
        panic!("expected error, got: {t:?}");
    }

    // Delay takes longer than total time, but pause layer pauses the timeout:
    // should succeed.
    if let Err(e) = s
        .builder()
        .layer_front(PauseLayer)
        .layer_back(DelayLayer)
        .build()?
        .send(GET!("/tests/ping").allowed_time(1.s()))
        .await
    {
        panic!("expected success, got: {e:?}");
    }

    Ok(())
}
