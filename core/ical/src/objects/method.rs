/// Method.
///
/// <https://www.rfc-editor.org/rfc/rfc5546.html#section-3.2>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Publish,
    Request,
    Reply,
    Add,
    Cancel,
    Refresh,
    Counter,
    DeclineCounter,
}
