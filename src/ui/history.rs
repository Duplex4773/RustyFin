use super::*;

#[derive(Default)]
pub(super) struct History {
    pub back: Vec<ViewState>,
    forward: Vec<ViewState>,
}

impl Screen {
    fn same_destination(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Discover, Self::Discover)
            | (Self::Downloads, Self::Downloads)
            | (Self::Explore, Self::Explore)
            | (Self::Anime, Self::Anime) => true,
            (Self::Library(at, aq), Self::Library(bt, bq)) => at == bt && aq == bq,
            (Self::Search(a), Self::Search(b)) => a == b,
            (Self::Settings(a), Self::Settings(b)) => a == b,
            (Self::Live(a), Self::Live(b)) | (Self::Timers(a), Self::Timers(b)) => a == b,
            (Self::AnimeTitle(a), Self::AnimeTitle(b)) => a == b,
            (Self::ExploreTitle(a), Self::ExploreTitle(b)) => {
                a.id == b.id && a.media_type == b.media_type
            }
            (Self::Guide(a), Self::Guide(b)) => a.id == b.id && a.origin == b.origin,
            (Self::Detail(a, _), Self::Detail(b, _)) => a.id == b.id && a.origin == b.origin,
            (
                Self::Copies {
                    item: a,
                    action: ap,
                },
                Self::Copies {
                    item: b,
                    action: bp,
                },
            ) => ap == bp && a.id == b.id && a.origin == b.origin,
            _ => false,
        }
    }
}

impl History {
    pub fn visit(&mut self, current: &mut ViewState, screen: Screen) {
        let next = ViewState::new(screen);
        let previous = std::mem::replace(current, next);
        if !previous.screen.same_destination(&current.screen) {
            self.clear_forward();
            // A cached library page can hold hundreds of posters.
            if self.back.len() == 32 {
                self.back.remove(0);
            }
            self.back.push(previous);
        }
    }

    pub fn destination(&self, direction: NavigationDirection) -> Option<&ViewState> {
        match direction {
            NavigationDirection::Back => self.back.last(),
            NavigationDirection::Forward => self.forward.last(),
        }
    }

    pub fn go(&mut self, current: &mut ViewState, direction: NavigationDirection) {
        let (from, to) = match direction {
            NavigationDirection::Back => (&mut self.back, &mut self.forward),
            NavigationDirection::Forward => (&mut self.forward, &mut self.back),
        };
        if let Some(destination) = from.pop() {
            to.push(std::mem::replace(current, destination));
        }
    }

    pub fn clear_forward(&mut self) {
        self.forward.clear();
    }

    pub fn clear(&mut self) {
        self.back.clear();
        self.clear_forward();
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ViewState> {
        self.back.iter_mut().chain(&mut self.forward)
    }
}

impl Library {
    // Capture at window level so side buttons also work over menus and focused inputs.
    pub(super) fn mouse_navigation(cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, w, _| {
                w.on_mouse_event(move |event: &MouseDownEvent, phase, w, cx| {
                    if phase == DispatchPhase::Capture
                        && let MouseButton::Navigate(direction) = event.button
                    {
                        cx.stop_propagation();
                        w.prevent_default();
                        let _ = view.update(cx, |s, cx| s.mouse_navigate(direction, w, cx));
                    }
                });
                w.on_mouse_event(|event: &MouseUpEvent, phase, w, cx| {
                    if phase == DispatchPhase::Capture
                        && matches!(event.button, MouseButton::Navigate(_))
                    {
                        cx.stop_propagation();
                        w.prevent_default();
                    }
                });
            },
        )
        .absolute()
        .size(px(0.))
    }

    fn mouse_navigate(
        &mut self,
        direction: NavigationDirection,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(player) = self.player.as_ref().filter(|_| !self.controls.mini) {
            if direction == NavigationDirection::Back {
                if player.panel_open {
                    self.back_player_menu(w, cx);
                } else if self.controls.remote_scrub {
                    self.end_preview();
                } else if self.credits_prompt().is_some() {
                    self.watch_credits(w, cx);
                } else {
                    self.stop();
                    self.focus.focus(w);
                }
                cx.notify();
            }
            return;
        }
        if self.server.is_none() {
            if direction == NavigationDirection::Back && self.code.is_some() {
                self.cancel_pairing(w, cx);
            }
            return;
        }
        match direction {
            NavigationDirection::Back => self.back(w, cx),
            NavigationDirection::Forward if self.state.detail_panel.is_none() => {
                self.restore_view(direction, w, cx)
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogQuery, History, Screen, Tab, ViewState};
    use gpui::NavigationDirection::{Back, Forward};

    #[test]
    fn revisiting_pages_restores_selection_and_new_destinations_replace_forward_history() {
        let mut history = History::default();
        let mut current = ViewState::new(Screen::Discover);
        history.visit(
            &mut current,
            Screen::Library(Tab::Browse, CatalogQuery::default()),
        );
        current.selection = 37;
        history.visit(&mut current, Screen::Downloads);
        history.go(&mut current, Back);
        assert!(matches!(current.screen, Screen::Library(..)));
        assert_eq!(current.selection, 37);
        history.go(&mut current, Back);
        assert!(matches!(current.screen, Screen::Discover));
        assert!(history.destination(Back).is_none());
        history.go(&mut current, Forward);
        assert_eq!(current.selection, 37);
        history.go(&mut current, Forward);
        assert!(matches!(current.screen, Screen::Downloads));
        history.go(&mut current, Back);
        history.visit(&mut current, Screen::Search("Fallout".into()));
        assert!(history.destination(Forward).is_none());
        history.go(&mut current, Back);
        assert_eq!(current.selection, 37);
        history.clear();
        assert!(history.destination(Forward).is_none());
        assert!(history.destination(Back).is_none());
    }

    #[test]
    fn reselecting_a_destination_does_not_add_a_back_step() {
        let mut history = History::default();
        let mut current = ViewState::new(Screen::Discover);
        history.visit(&mut current, Screen::Discover);
        assert!(history.destination(Back).is_none());
        history.visit(
            &mut current,
            Screen::Library(Tab::Browse, CatalogQuery::default()),
        );
        history.visit(
            &mut current,
            Screen::Library(Tab::Browse, CatalogQuery::favourites()),
        );
        history.visit(
            &mut current,
            Screen::Library(Tab::Browse, CatalogQuery::favourites()),
        );
        history.go(&mut current, Back);
        assert!(matches!(&current.screen, Screen::Library(_, q) if !q.favorites));
    }
}
