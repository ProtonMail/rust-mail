#[derive(Debug)]
pub enum SessionEvent {
    LoadSessions,
    SelectSession(usize),
    NewSession,
}
