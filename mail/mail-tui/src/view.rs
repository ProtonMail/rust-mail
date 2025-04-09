use crate::widgets::HelpCategory;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;

pub trait View<S, E> {
    fn on_enter(&mut self, _: &mut S) {}
    fn on_exit(&mut self, _: &mut S) {}

    fn draw(&mut self, state: &S, frame: &mut Frame, area: Rect);

    fn help_items(&self) -> &[HelpCategory];

    fn on_input(&mut self, state: &mut S, event: &Event);

    #[allow(unused)]
    fn name(&self) -> &'static str;
}

enum ViewStackOp<S, E> {
    Push(Box<dyn View<S, E>>),
    Pop,
    PopAll,
}

pub struct ViewStack<T, E> {
    pending: Vec<ViewStackOp<T, E>>,
    stack: Vec<Box<dyn View<T, E>>>,
}

impl<T, E> ViewStack<T, E> {
    pub fn new() -> Self {
        Self {
            pending: Vec::with_capacity(4),
            stack: Vec::with_capacity(4),
        }
    }

    pub fn push_view<V: View<T, E> + 'static>(&mut self, view: V) {
        self.pending.push(ViewStackOp::Push(Box::new(view)));
    }

    pub fn pop_view(&mut self) {
        self.pending.push(ViewStackOp::Pop);
    }

    pub fn pop_all(&mut self) {
        self.pending.push(ViewStackOp::PopAll);
    }
    pub fn process(&mut self, state: &mut T) {
        let pending = std::mem::take(&mut self.pending);
        for op in pending {
            match op {
                ViewStackOp::Push(mut v) => {
                    if let Some(top) = self.stack.last_mut() {
                        top.on_exit(state);
                    }
                    v.on_enter(state);
                    self.stack.push(v);
                }
                ViewStackOp::Pop => {
                    if let Some(mut view) = self.stack.pop() {
                        view.on_exit(state);
                    }
                    if let Some(view) = self.stack.last_mut() {
                        view.on_enter(state);
                    }
                }
                ViewStackOp::PopAll => {
                    if let Some(mut view) = self.stack.pop() {
                        view.on_exit(state);
                    }
                    self.stack.clear();
                }
            }
        }
    }

    pub fn top_mut(&mut self) -> Option<&mut Box<dyn View<T, E>>> {
        self.stack.last_mut()
    }
}
