use super::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

type Target = (FocusHandle, Bounds<Pixels>);

#[derive(Default)]
pub(in crate::ui) struct SettingsNavigation {
    handles: RefCell<HashMap<ElementId, FocusHandle>>,
    painted: Rc<RefCell<Vec<Target>>>,
    return_to: Option<FocusHandle>,
}

impl SettingsNavigation {
    pub(super) fn begin_layout(&self) {
        self.painted.borrow_mut().clear();
    }

    fn marker(&self, handle: &FocusHandle) -> AnyElement {
        let painted = self.painted.clone();
        let handle = handle.clone();
        canvas(
            move |bounds, _, _| painted.borrow_mut().push((handle, bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .size_full()
        .into_any_element()
    }

    pub(in crate::ui) fn remember(&mut self, w: &Window, cx: &App) {
        self.return_to = w
            .focused(cx)
            .filter(|h| self.painted.borrow().iter().any(|(f, _)| f == h));
    }

    pub(in crate::ui) fn restore(&mut self, w: &mut Window) -> bool {
        let Some(handle) = self.return_to.take() else {
            return false;
        };
        handle.focus(w);
        true
    }
}

// Buttons may wrap into a new row when the window is resized. Pick the nearest
// control in the requested direction from the bounds of the current layout.
fn distance(key: &str, from: Bounds<Pixels>, to: Bounds<Pixels>) -> Option<(Pixels, Pixels)> {
    let row_overlap = from.top() < to.bottom() && to.top() < from.bottom();
    let horizontal_gap = (to.left() - from.right())
        .max(from.left() - to.right())
        .max(px(0.));
    match key {
        "left" if row_overlap && to.right() <= from.left() => {
            Some((from.left() - to.right(), px(0.)))
        }
        "right" if row_overlap && to.left() >= from.right() => {
            Some((to.left() - from.right(), px(0.)))
        }
        "up" if to.bottom() <= from.top() => Some((from.top() - to.bottom(), horizontal_gap)),
        "down" if to.top() >= from.bottom() => Some((to.top() - from.bottom(), horizontal_gap)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::distance;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn directional_focus_prefers_a_same_column_control_in_the_next_row() {
        let bounds = |x, y| Bounds::new(point(px(x), px(y)), size(px(40.), px(30.)));
        let here = bounds(50., 0.);
        let adjacent = bounds(100., 0.);
        let below = bounds(50., 40.);
        let diagonal = bounds(0., 40.);
        assert!(distance("right", here, adjacent).is_some());
        assert!(distance("down", here, adjacent).is_none());
        assert!(distance("left", here, diagonal).is_none());
        assert!(distance("down", here, below).unwrap() < distance("down", here, diagonal).unwrap());
    }
}

impl Library {
    pub(super) fn service_button(
        &self,
        cx: &App,
        id: &'static str,
        text: &'static str,
        glyph: Option<&'static str>,
        active: bool,
    ) -> Stateful<Div> {
        self.settings_button(pill(id, text, glyph, active), cx)
    }
    pub(super) fn settings_button(&self, button: Stateful<Div>, cx: &App) -> Stateful<Div> {
        if !matches!(self.state.screen, Screen::Settings(_)) {
            return button;
        }
        self.page_button(button, cx)
    }

    pub(super) fn page_button(&self, button: Stateful<Div>, cx: &App) -> Stateful<Div> {
        let nav = &self.state.settings_nav;
        let id = Element::id(&button).expect("page controls have stable IDs");
        let handle = nav
            .handles
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        accessibility::focus(button, &handle)
            .relative()
            .child(nav.marker(&handle))
    }

    pub(super) fn focus_page_button(&self, id: impl Into<ElementId>, w: &mut Window) {
        if let Some(handle) = self.state.settings_nav.handles.borrow().get(&id.into()) {
            handle.focus(w);
        }
    }

    pub(super) fn settings_input(
        &self,
        input: &Entity<InputState>,
        name: &'static str,
        masked: bool,
        cx: &App,
    ) -> Div {
        let field = accessibility::field(Input::new(input), input, name, masked, cx);
        field.when(matches!(self.state.screen, Screen::Settings(_)), |d| {
            d.child(
                self.state
                    .settings_nav
                    .marker(&input.read(cx).focus_handle(cx)),
            )
        })
    }

    pub(in crate::ui) fn settings_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(key, "up" | "down" | "left" | "right") {
            return false;
        }
        let discover = matches!(self.state.screen, Screen::Discover)
            && self.popup_menu.is_none()
            && !self.navigation_focus.contains_focused(w, cx);
        if !discover && !matches!(self.state.screen, Screen::Settings(_)) {
            return false;
        }
        let painted = self.state.settings_nav.painted.borrow();
        let current = painted.iter().find(|(h, _)| h.is_focused(w));
        if discover && current.is_none() && (key != "up" || self.state.shelf.is_some()) {
            return false;
        }
        let target = match current {
            // Subpages start with a Back arrow above the first setting.
            None if matches!(self.state.screen, Screen::Settings(page) if page != SettingsPage::Overview) => {
                painted.get(1).or(painted.first())
            }
            None => painted.first(),
            Some((_, from)) => painted
                .iter()
                .filter_map(|target| {
                    distance(key, *from, target.1).map(|distance| (distance, target))
                })
                .min_by_key(|(distance, _)| *distance)
                .map(|(_, target)| target),
        };
        if let Some((handle, bounds)) = target {
            handle.focus(w);
            let scroll = &self.state.scroll;
            let viewport = scroll.bounds();
            let offset = scroll.offset();
            let delta = if bounds.top() < viewport.top() {
                viewport.top() - bounds.top()
            } else if bounds.bottom() > viewport.bottom() {
                viewport.bottom() - bounds.bottom()
            } else {
                px(0.)
            };
            scroll.set_offset(point(
                offset.x,
                (offset.y + delta).clamp(-scroll.max_offset().height, px(0.)),
            ));
        } else if discover && key == "down" {
            return false;
        } else if key == "up" && current.is_some() {
            self.navigation_focus.focus(w);
            w.focus_next();
        }
        self.keyboard = true;
        cx.notify();
        true
    }
}
