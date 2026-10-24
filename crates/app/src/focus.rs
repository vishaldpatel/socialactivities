//! Tab and Shift+Tab: like Iced's `focus_next`/`focus_previous`, but the text
//! of the field gaining focus is selected (so typing replaces it, as in most
//! apps), its scroll area scrolls to show it, and focus wraps around at
//! either end.

use iced::advanced::widget::operation::focusable::{self, Count, Focusable};
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, Scrollable};
use iced::advanced::widget::operation::{self, Operation, Outcome, TextInput};
use iced::advanced::widget::{Id, operate};
use iced::{Rectangle, Task, Vector};

/// Space kept between a focused widget and the edge it was scrolled to.
const SCROLL_MARGIN: f32 = 24.0;

pub fn step<T: Send + 'static>(forward: bool) -> Task<T> {
    // `then` takes plain functions, so each direction gets its own. Selecting
    // happens in a second pass because focusing a field moves its cursor to
    // the end, which would clear a selection made first.
    let moved = if forward {
        operation::then(focusable::count(), |count| Step::new(count, true))
    } else {
        operation::then(focusable::count(), |count| Step::new(count, false))
    };
    let selected = operation::then(moved, SelectAt::new);
    operate(operation::then(selected, ScrollTo::new))
}

/// The index of the widget to focus, given which one (if any) has focus now.
fn target(count: Count, forward: bool) -> Option<usize> {
    let total = count.total;
    if total == 0 {
        return None;
    }
    Some(match (count.focused, forward) {
        (None, true) => 0,
        (None, false) => total - 1,
        (Some(i), true) => (i + 1) % total,
        (Some(i), false) => (i + total - 1) % total,
    })
}

/// Focuses the target widget and unfocuses the rest, reporting the target.
struct Step {
    target: Option<usize>,
    /// Index of the next focusable widget to be visited.
    current: usize,
}

impl Step {
    fn new(count: Count, forward: bool) -> Self {
        Self {
            target: target(count, forward),
            current: 0,
        }
    }
}

impl Operation<Option<usize>> for Step {
    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.target == Some(self.current) {
            state.focus();
        } else if state.is_focused() {
            state.unfocus();
        }
        self.current += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<usize>>)) {
        operate(self);
    }

    fn finish(&self) -> Outcome<Option<usize>> {
        Outcome::Some(self.target)
    }
}

/// Selects the text of the focusable widget at `target`, if it's a text field.
/// Also works out how to scroll the target into view, for [`ScrollTo`].
struct SelectAt {
    target: Option<usize>,
    current: usize,
    /// Scroll areas seen so far: (visible bounds, content bounds, offset).
    scrollables: Vec<(Rectangle, Rectangle, Vector)>,
    plan: Option<ScrollPlan>,
}

/// Scroll the `index`th scroll area to vertical offset `y`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollPlan {
    index: usize,
    y: f32,
}

impl SelectAt {
    fn new(target: Option<usize>) -> Self {
        Self {
            target,
            current: 0,
            scrollables: Vec::new(),
            plan: None,
        }
    }
}

impl Operation<Option<ScrollPlan>> for SelectAt {
    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.scrollables.push((bounds, content_bounds, translation));
    }

    // A text field reports its text state just before its focus state, so
    // `current` is its index here too.
    fn text_input(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn TextInput) {
        if self.target == Some(self.current) {
            state.select_all();
        }
    }

    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if self.target == Some(self.current) {
            // The innermost scroll area holding the widget is the last one seen
            // whose content contains it.
            self.plan = self
                .scrollables
                .iter()
                .enumerate()
                .rev()
                .find(|(_, (_, content, _))| content.contains(bounds.position()))
                .and_then(|(index, &(view, content, offset))| {
                    scroll_needed(view, content, offset, bounds).map(|y| ScrollPlan { index, y })
                });
        }
        self.current += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<ScrollPlan>>)) {
        operate(self);
    }

    fn finish(&self) -> Outcome<Option<ScrollPlan>> {
        Outcome::Some(self.plan)
    }
}

/// The vertical offset that brings `widget` into view, if it isn't already.
/// `widget` and `content` are in unscrolled coordinates; `offset` is how far
/// the area is scrolled.
fn scroll_needed(
    view: Rectangle,
    content: Rectangle,
    offset: Vector,
    widget: Rectangle,
) -> Option<f32> {
    let top = widget.y - content.y;
    let bottom = top + widget.height;
    let max = (content.height - view.height).max(0.0);
    if top < offset.y {
        Some((top - SCROLL_MARGIN).clamp(0.0, max))
    } else if bottom > offset.y + view.height {
        Some((bottom - view.height + SCROLL_MARGIN).clamp(0.0, max))
    } else {
        None
    }
}

/// Carries out a [`ScrollPlan`].
struct ScrollTo {
    plan: Option<ScrollPlan>,
    current: usize,
}

impl ScrollTo {
    fn new(plan: Option<ScrollPlan>) -> Self {
        Self { plan, current: 0 }
    }
}

impl<T> Operation<T> for ScrollTo {
    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        _bounds: Rectangle,
        _content_bounds: Rectangle,
        _translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if let Some(plan) = self.plan.filter(|p| p.index == self.current) {
            state.scroll_to(AbsoluteOffset {
                x: None,
                y: Some(plan.y),
            });
        }
        self.current += 1;
    }

    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(focused: Option<usize>, total: usize) -> Count {
        Count { focused, total }
    }

    #[test]
    fn scrolls_only_as_far_as_needed() {
        let view = Rectangle::new((0.0, 100.0).into(), (300.0, 200.0).into());
        let content = Rectangle::new((0.0, 100.0).into(), (300.0, 1000.0).into());
        let at = |y: f32| Rectangle::new((0.0, 100.0 + y).into(), (100.0, 30.0).into());
        let scrolled = |y: f32| Vector::new(0.0, y);

        assert_eq!(
            scroll_needed(view, content, scrolled(0.0), at(50.0)),
            None,
            "visible"
        );
        assert_eq!(
            scroll_needed(view, content, scrolled(0.0), at(400.0)),
            Some(400.0 + 30.0 - 200.0 + SCROLL_MARGIN),
            "below: just into view at the bottom"
        );
        assert_eq!(
            scroll_needed(view, content, scrolled(500.0), at(100.0)),
            Some(100.0 - SCROLL_MARGIN),
            "above: just into view at the top"
        );
        assert_eq!(
            scroll_needed(view, content, scrolled(500.0), at(10.0)),
            Some(0.0),
            "never past the start"
        );
    }

    #[test]
    fn wraps_in_both_directions() {
        assert_eq!(target(count(None, 3), true), Some(0));
        assert_eq!(target(count(None, 3), false), Some(2));
        assert_eq!(target(count(Some(2), 3), true), Some(0));
        assert_eq!(target(count(Some(0), 3), false), Some(2));
        assert_eq!(target(count(Some(1), 3), true), Some(2));
        assert_eq!(target(count(None, 0), true), None);
    }
}
