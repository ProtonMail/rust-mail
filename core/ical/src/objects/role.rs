/// Participation role.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.16>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Chair,
    ReqParticipant,
    OptParticipant,
    NonParticipant,
}
