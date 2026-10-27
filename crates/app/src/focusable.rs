//! Makes any widget reachable with Tab. Iced's buttons and checkboxes can't
//! take keyboard focus, so this wraps one: it joins the focus order, draws a
//! ring while focused, and turns key presses into messages.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::{
    Border, Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, mouse,
};

/// `content`, made reachable with Tab; while focused, `on_key` decides what a
/// key press sends, if anything.
pub fn focusable<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    on_key: impl Fn(&keyboard::Key) -> Option<M> + 'a,
) -> Element<'a, M> {
    Element::new(Wrapper {
        content: content.into(),
        on_key: Box::new(on_key),
        id: None,
    })
}

/// Like [`button`], with an id so it can be focused directly.
pub fn button_with_id<'a, M: Clone + 'a>(
    b: iced::widget::Button<'a, M>,
    msg: M,
    id: &'static str,
) -> Element<'a, M> {
    Element::new(Wrapper {
        content: b.on_press(msg.clone()).into(),
        on_key: Box::new(on_activate(msg)),
        id: Some(iced::advanced::widget::Id::new(id)),
    })
}

/// Enter or Space sends `msg`: how buttons and checkboxes respond.
pub fn on_activate<M: Clone>(msg: M) -> impl Fn(&keyboard::Key) -> Option<M> {
    move |key| {
        matches!(
            key.as_ref(),
            keyboard::Key::Named(Named::Enter | Named::Space)
        )
        .then(|| msg.clone())
    }
}

/// A button Tab can reach and Enter or Space can press; `msg` is sent either way.
pub fn button<'a, M: Clone + 'a>(b: iced::widget::Button<'a, M>, msg: M) -> Element<'a, M> {
    focusable(b.on_press(msg.clone()), on_activate(msg))
}

/// A dropdown Tab can reach. Iced's dropdown can't be opened from the
/// keyboard, so while focused, Up/Left and Down/Right step to the previous or
/// next option instead; the mouse still opens the full list.
pub fn pick_list<'a, T, M>(
    options: &'a [T],
    selected: T,
    on_select: impl Fn(T) -> M + Clone + 'a,
) -> Element<'a, M>
where
    T: ToString + PartialEq + Clone + 'a,
    M: Clone + 'a,
{
    let list = iced::widget::pick_list(options, Some(selected.clone()), on_select.clone());
    let current = options.iter().position(|o| *o == selected);
    focusable(list, move |key| {
        let step: isize = match key.as_ref() {
            keyboard::Key::Named(Named::ArrowUp | Named::ArrowLeft) => -1,
            keyboard::Key::Named(Named::ArrowDown | Named::ArrowRight) => 1,
            _ => return None,
        };
        let next = current?.checked_add_signed(step)?;
        options.get(next).cloned().map(&on_select)
    })
}

/// A checkbox Tab can reach; Space or Enter toggles it.
pub fn checkbox<'a, M: Clone + 'a>(
    checked: bool,
    label: &'a str,
    on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    let toggled = on_toggle(!checked);
    let checkbox = iced::widget::checkbox(checked)
        .label(label)
        .on_toggle(on_toggle)
        .size(16)
        .text_size(14);
    focusable(checkbox, on_activate(toggled))
}

/// An on/off switch Tab can reach; Space or Enter flips it.
pub fn toggler<'a, M: Clone + 'a>(
    on: bool,
    label: &'a str,
    on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    let toggled = on_toggle(!on);
    let toggler = iced::widget::toggler(on)
        .label(label)
        .on_toggle(on_toggle)
        .size(18)
        .text_size(14);
    focusable(toggler, on_activate(toggled))
}

/// Turns a key press into a message, if the key does anything.
type OnKey<'a, M> = Box<dyn Fn(&keyboard::Key) -> Option<M> + 'a>;

struct Wrapper<'a, M> {
    content: Element<'a, M>,
    on_key: OnKey<'a, M>,
    id: Option<iced::advanced::widget::Id>,
}

#[derive(Default)]
struct State {
    focused: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<M> Widget<M, Theme, Renderer> for Wrapper<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.focusable(
            self.id.as_ref(),
            layout.bounds(),
            tree.state.downcast_mut::<State>(),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        // Clicking anywhere moves focus: onto this button if it's the target,
        // away from it otherwise (as text fields do).
        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            tree.state.downcast_mut::<State>().focused = cursor.is_over(layout.bounds());
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }

        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
            && tree.state.downcast_ref::<State>().focused
            && let Some(msg) = (self.on_key)(key)
        {
            shell.publish(msg);
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );

        if tree.state.downcast_ref::<State>().focused {
            use iced::advanced::Renderer as _;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds().expand(2.0),
                    border: Border {
                        color: theme.extended_palette().primary.strong.color,
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
