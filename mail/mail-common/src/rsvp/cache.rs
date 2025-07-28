use proton_calendar_api::{CalendarBootstrap, CalendarId};
use proton_calendar_common as cal;
use std::collections::HashMap;
use tokio::sync::Mutex;

/// Write-once cache for calendar bootstrap data.
///
/// Decryping events requires access to the calendar key which is provided by
/// the calendar bootstrap data - fetching this bootstrap data takes a moment,
/// hence we have this struct which is responsible for caching them bootstraps.
///
/// Note that this is a rudimentary implementation - in particular, if calendar
/// key gets rotated, we will continue to serve the old one until user restarts
/// the application, since we don't listen to server events in here.
///
/// Fortunately, calendar keys are almost never rotated (:fingers-crossed:) and
/// even if they are, restarting the application will reset this cache, causing
/// it to download the current key, no harm done.
///
/// This will be implemented properly over NGC-57, where we'll store the keys
/// into the local database and listen on the event loop - at the moment it's
/// more of a "good enough for the time being" kind of code.
///
/// TODO (NGC-57) implement support for offline-mode
#[derive(Debug, Default)]
pub(crate) struct RsvpCache {
    calendars: Mutex<HashMap<CalendarId, CalendarBootstrap>>,
}

impl cal::RsvpCache for RsvpCache {
    async fn get_calendar_bootstrap<E, Fn, Fut>(
        &self,
        id: &CalendarId,
        fetch: Fn,
    ) -> Result<CalendarBootstrap, E>
    where
        Fn: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<CalendarBootstrap, E>> + Send,
    {
        let mut calendars = self.calendars.lock().await;

        if let Some(calendar) = calendars.get(id) {
            Ok(calendar.clone())
        } else {
            let calendar = fetch().await?;

            calendars.insert(id.clone(), calendar.clone());

            Ok(calendar)
        }
    }
}
