use crate::events::session::SessionEvent;
use crate::events::AppEvent;
use crate::view::View;
use crate::views::AppViewContext;
use crate::widgets::{
    HelpCategory, HelpItem, ScrollableList, ScrollableListState, SessionWidget, WidgetList,
    WidgetListItem,
};
use crossterm::event::{Event, KeyCode};
use ratatui::layout::{Constraint, Flex, Margin, Rect};
use ratatui::prelude::Layout;
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

pub struct SessionsView {
    session_list_state: ScrollableListState,
}

impl SessionsView {
    pub fn new() -> Self {
        let mut sv = Self {
            session_list_state: ScrollableListState::new(2, Some(0)),
        };
        sv.session_list_state.set_focus_gained();
        sv
    }
}

impl View<AppViewContext, AppEvent> for SessionsView {
    fn on_enter(&mut self, ctx: &mut AppViewContext) {
        ctx.app_local_dispatcher()
            .queue_event(SessionEvent::LoadSessions)
    }
    fn draw(&mut self, ctx: &AppViewContext, frame: &mut Frame, area: Rect) {
        let area = area.inner(&Margin {
            horizontal: 10,
            vertical: 2,
        });
        let sessions = ctx.state().session_state.sessions();

        let [_, area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(40),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .areas(area);

        let list_sessions = sessions
            .iter()
            .map(|session| WidgetListItem::new(SessionWidget::new(session)))
            .collect::<Vec<_>>();
        self.session_list_state.set_len(sessions.len());

        frame.render_stateful_widget(
            ScrollableList::new(
                WidgetList::new(list_sessions)
                    .block(Block::new().title("Sessions").borders(Borders::all())),
            ),
            area,
            &mut self.session_list_state,
        );
    }

    fn help_items(&self) -> &[HelpCategory] {
        static ITEMS: [HelpCategory; 1] = [HelpCategory {
            name: "Session",
            items: &[
                HelpItem {
                    key: "▲",
                    description: "Previous Item",
                },
                HelpItem {
                    key: "▼",
                    description: "Next Item",
                },
                HelpItem {
                    key: "Enter",
                    description: "Select Session",
                },
                HelpItem {
                    key: "N",
                    description: "New Session",
                },
            ],
        }];

        &ITEMS
    }

    fn on_input(&mut self, ctx: &mut AppViewContext, event: &Event) {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('n') => {
                    ctx.app_local_dispatcher()
                        .queue_event(SessionEvent::NewSession);
                }
                KeyCode::Up => self.session_list_state.prev(),
                KeyCode::Down => self.session_list_state.next(),
                KeyCode::Enter => {
                    if let Some(index) = self.session_list_state.selected() {
                        ctx.app_local_dispatcher()
                            .queue_event(SessionEvent::SelectSession(index))
                    }
                }
                _ => {}
            }
        }
    }

    fn name(&self) -> &'static str {
        "Sessions"
    }
}
