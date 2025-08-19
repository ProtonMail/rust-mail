use crate::app::Command;
use crate::app_model::Popup;
use crate::app_model::mailbox::ComposerMessage;
use crate::messages::Messages;
use crate::widgets::utils::ScrollableState;
use crate::widgets::{ScrollableList, ScrollableListState};
use anyhow::anyhow;
use crossterm::event::{Event, KeyCode};
use proton_core_common::models::Address;
use proton_mail_common::draft::Draft;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::widgets::{List, ListItem};

pub struct AddressListPopup {
    addresses: Vec<Address>,
    scrollable_list_state: ScrollableListState,
}

impl AddressListPopup {
    fn new(addresses: Vec<Address>) -> Self {
        Self {
            addresses,
            scrollable_list_state: ScrollableListState::new(Some(0)),
        }
    }

    pub fn open(draft: Draft) -> Command<Messages> {
        Command::task(async move {
            match draft.sender_addresses().await {
                Ok(address) => Command::message(Messages::RaisePopup(Box::new(Self::new(address)))),
                Err(e) => Command::message(Messages::DisplayError(
                    None,
                    anyhow!("Failed to load addresses: {e:?}"),
                )),
            }
        })
    }
}
impl Popup for AddressListPopup {
    fn title(&self) -> Option<String> {
        Some("Change Sender Address".to_owned())
    }

    fn handle_event(&mut self, event: Event) -> Command<Messages> {
        let Event::Key(key) = event else {
            return Command::none();
        };
        if self.scrollable_list_state.handle_event(key.code) {
            return Command::none();
        }
        match key.code {
            KeyCode::Enter => {
                self.scrollable_list_state
                    .selected()
                    .map_or(Command::none(), |index| {
                        Command::batch([
                            Command::message(Messages::DismissPopup),
                            Command::message(ComposerMessage::StartChangeAddress((
                                self.addresses[index].email.clone(),
                                self.addresses[index]
                                    .remote_id
                                    .clone()
                                    .expect("should be set"),
                            ))),
                        ])
                    })
            }
            _ => Command::none(),
        }
    }

    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin::new(1, 1));

        let list = ScrollableList::new(List::new(
            self.addresses
                .iter()
                .map(|address| ListItem::new(address.email.clone())),
        ));

        frame.render_stateful_widget(list, area, &mut self.scrollable_list_state);
    }
}
