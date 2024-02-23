use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::Frame;

pub trait View<S, E> {
    fn on_enter(&mut self, _: &mut S) {}
    fn on_exit(&mut self, _: &mut S) {}

    fn draw(&mut self, state: &S, frame: &mut Frame, area: Rect);
    fn draw_help(&self, _: &S, _: &mut Frame, _: Rect) {}

    fn on_event(&mut self, state: &mut S, event: E) -> Option<E>;

    fn on_input(&mut self, state: &mut S, event: &Event);

    fn name(&self) -> &'static str;
}

enum ViewStackOp<S, E> {
    Push(Box<dyn View<S, E>>),
    PopAndPush(Box<dyn View<S, E>>),
    Pop,
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

    pub fn pop_and_push_view<V: View<T, E> + 'static>(&mut self, view: V) {
        self.pending.push(ViewStackOp::PopAndPush(Box::new(view)));
    }
    pub fn pop_view(&mut self) {
        self.pending.push(ViewStackOp::Pop);
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
                ViewStackOp::PopAndPush(mut v) => {
                    if let Some(mut top) = self.stack.pop() {
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
            }
        }
    }

    pub fn top_mut(&mut self) -> Option<&mut Box<dyn View<T, E>>> {
        self.stack.last_mut()
    }

    pub fn on_event(&mut self, state: &mut T, mut event: E) {
        for view in self.stack.iter_mut().rev() {
            match view.on_event(state, event) {
                None => return,
                Some(e) => event = e,
            }
        }
    }
}
