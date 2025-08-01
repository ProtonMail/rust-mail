use crate::{RsvpError, RsvpResult};
use proton_calendar_api::{CalendarBootstrap, CalendarEvent, CalendarEventPayload};
use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_calendar::{
    CalendarEventDecryptor, EncryptedIcsRef, KeyPackets, LockedCalendarKey, Result as CryptoResult,
    UnlockedKeys,
};
use proton_ical as ical;
use std::borrow::Cow;
use tracing::debug;

pub trait CalendarEventPayloadExt {
    /// Decrypts this event, returning the *.ics.
    ///
    /// If this event was actually encrypted, returns `Cow::Owned` with the
    /// decrypted contents; for events stored in plain-text, this function
    /// returns `Cow::Borrowed`.
    fn decrypt<'a, P>(
        &'a self,
        pgp: &P,
        decryptor: &CalendarEventDecryptor<P>,
    ) -> CryptoResult<Cow<'a, [u8]>>
    where
        P: PGPProviderSync;

    /// Decrypts this event (if it's encrypted) and parses it.
    fn decrypt_and_parse<P>(
        &self,
        pgp: &P,
        decryptor: &CalendarEventDecryptor<P>,
    ) -> RsvpResult<ical::VEvent>
    where
        P: PGPProviderSync,
    {
        let ics = self.decrypt(pgp, decryptor)?;
        let out = ical::VCalendar::from_bytes(&ics)?;

        // Since clients are not necessarily 100% RFC-compliant, it's expected
        // that we'll get some parser or validator messages here.
        //
        // Those messages are not "errors-errors", because if we got to this
        // point, we were able to successfully recover some useful information
        // from the *.ics, so there's no point in bailing out now.
        for msg in out.msgs {
            debug!("ics-parser said: {msg}");
        }
        for viol in out.viols {
            debug!("ics-validator said: {viol}");
        }

        let cal = out.cal;

        if cal.events.len() > 1 {
            return Err(RsvpError::IcsContainsMoreThanOneEvent);
        }

        cal.events
            .into_iter()
            .next()
            .ok_or(RsvpError::IcsContainsNoEvents)
    }
}

impl CalendarEventPayloadExt for CalendarEventPayload {
    fn decrypt<'a, P>(
        &'a self,
        pgp: &P,
        decryptor: &CalendarEventDecryptor<P>,
    ) -> CryptoResult<Cow<'a, [u8]>>
    where
        P: PGPProviderSync,
    {
        if self.ty.is_encrypted() {
            let data = {
                let data = EncryptedIcsRef::from_base64(&self.data);

                // We deliberately ignore `self.signature` here - that's because
                // validating signatures is somewhat of an awkward chore that
                // doesn't bring much security.
                //
                // Events are already encrypted with either the address key or
                // the calendar key, both of them known - in principle - only to
                // the user, so an adversary can't spoof events, because they
                // wouldn't be able to encrypt them, signatures or not.
                let sign = None;

                decryptor.decrypt(pgp, data, sign)?.into_bytes()
            };

            Ok(Cow::Owned(data))
        } else {
            Ok(Cow::Borrowed(self.data.as_bytes()))
        }
    }
}

pub trait CalendarBootstrapExt {
    fn create_decryptor<'a, P>(
        &self,
        pgp: &'a P,
        keys: &'a UnlockedKeys<P>,
        event: &CalendarEvent,
    ) -> RsvpResult<CalendarEventDecryptor<'a, P>>
    where
        P: PGPProviderSync;
}

impl CalendarBootstrapExt for CalendarBootstrap {
    fn create_decryptor<'a, P>(
        &self,
        pgp: &'a P,
        keys: &'a UnlockedKeys<P>,
        event: &CalendarEvent,
    ) -> RsvpResult<CalendarEventDecryptor<'a, P>>
    where
        P: PGPProviderSync,
    {
        let calendar_key = LockedCalendarKey::from_bootstrap(self)?.import(pgp, keys)?;
        let key_packets = KeyPackets::from_event(event);

        CalendarEventDecryptor::new(pgp, &keys.address_keys, &calendar_key, key_packets)
            .map_err(Into::into)
    }
}
