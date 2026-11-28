use accesskit::{
    Action, ActionData, ActionHandler, ActionRequest, ActivationHandler, Live, Node, NodeId, Rect,
    Role, Toggled, Tree, TreeId, TreeUpdate,
};
use gpui::{prelude::*, *};
use gpui_component::input::{Input, InputState};
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    rc::{Rc, Weak},
};

const ROOT: NodeId = NodeId(0);

thread_local! {
    static WINDOW: RefCell<Weak<RefCell<Bridge>>> = RefCell::default();
}

enum Message {
    Activated,
    Action(ActionRequest),
}

struct Events(async_channel::Sender<Message>);

impl ActivationHandler for Events {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self.0.try_send(Message::Activated);
        None
    }
}

impl ActionHandler for Events {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.0.try_send(Message::Action(request));
    }
}

pub(super) struct Accessibility {
    _bridge: Rc<RefCell<Bridge>>,
}

struct Bridge {
    adapter: accesskit_windows::SubclassingAdapter,
    active: bool,
    generation: u64,
    entries: Vec<Entry>,
    repeated_ids: HashMap<u64, u64>,
    handles: HashMap<ElementId, FocusHandle>,
    visible: HashMap<NodeId, Entry>,
    virtual_focus: Option<(NodeId, Option<FocusHandle>)>,
    playback_focus: Option<FocusHandle>,
    last: Option<(NodeId, Vec<(NodeId, Node)>)>,
}

#[derive(Clone)]
struct Entry {
    id: NodeId,
    key: ElementId,
    node: Node,
    bounds: Bounds<Pixels>,
    focus: Option<FocusHandle>,
    target: Target,
    focused: bool,
}

#[derive(Clone)]
enum Target {
    None,
    Button,
    Poster(Poster),
    Input(Entity<InputState>),
    Seek(f64),
    Volume,
    ShowPlaybackControls,
}

#[derive(Clone, Copy)]
pub(super) enum Poster {
    Browse(usize),
    Home { row: usize, index: usize },
}

fn bridge() -> Option<Rc<RefCell<Bridge>>> {
    WINDOW.with(|window| window.borrow().upgrade())
}

pub(super) fn active() -> bool {
    bridge().is_some_and(|bridge| bridge.borrow().active)
}

pub(super) fn playback_focused(w: &Window, cx: &App, keyboard: bool) -> bool {
    let Some(bridge) = bridge() else { return false };
    let mut state = bridge.borrow_mut();
    if !keyboard || !w.is_window_active() || state.playback_focus != w.focused(cx) {
        state.playback_focus = None;
    }
    state.playback_focus.is_some()
}

impl Accessibility {
    pub(super) fn attach(w: &Window, cx: &mut Context<super::Library>) -> Option<Self> {
        let hwnd = crate::player::parent(w).ok()?;
        let (send, recv) = async_channel::unbounded();
        let bridge = Rc::new(RefCell::new(Bridge {
            adapter: accesskit_windows::SubclassingAdapter::new(
                accesskit_windows::HWND(hwnd.0),
                Events(send.clone()),
                Events(send),
            ),
            active: false,
            generation: 0,
            entries: Vec::new(),
            repeated_ids: HashMap::new(),
            handles: HashMap::new(),
            visible: HashMap::new(),
            virtual_focus: None,
            playback_focus: None,
            last: None,
        }));
        let weak = Rc::downgrade(&bridge);
        WINDOW.with(|window| *window.borrow_mut() = weak.clone());
        cx.spawn_in(w, async move |view, cx| {
            while let Ok(message) = recv.recv().await {
                let Some(bridge) = weak.upgrade() else { break };
                if cx
                    .update(|w, cx| match message {
                        Message::Activated => {
                            bridge.borrow_mut().active = true;
                            let _ = view.update(cx, |_, cx| cx.notify());
                        }
                        Message::Action(request) => {
                            let entry = bridge.borrow().visible.get(&request.target_node).cloned();
                            let Some(entry) = entry else { return };
                            match request.action {
                                Action::Focus => {
                                    if let Some(focus) = &entry.focus {
                                        focus.focus(w);
                                    }
                                    let playback = view
                                        .update(cx, |s, cx| {
                                            if let Target::Poster(poster) = entry.target {
                                                match poster {
                                                    Poster::Browse(index)
                                                        if matches!(
                                                            s.state.screen,
                                                            super::Screen::Library(..)
                                                        ) =>
                                                    {
                                                        s.state.selection = index;
                                                    }
                                                    Poster::Home { row, index }
                                                        if matches!(
                                                            s.state.screen,
                                                            super::Screen::Discover
                                                        ) =>
                                                    {
                                                        s.state.shelf = Some(row);
                                                        s.state.selection = index;
                                                    }
                                                    _ => return false,
                                                }
                                                s.keyboard = true;
                                                s.focus.focus(w);
                                            }
                                            let player = s.player.as_mut().filter(|_| {
                                                !s.controls.mini
                                                    && !matches!(
                                                        entry.target,
                                                        Target::ShowPlaybackControls
                                                    )
                                            });
                                            let playback = player.is_some();
                                            if let Some(player) = player {
                                                player.use_keyboard_controls();
                                            }
                                            cx.notify();
                                            playback
                                        })
                                        .unwrap_or(false);
                                    let focus = w.focused(cx);
                                    let mut state = bridge.borrow_mut();
                                    state.virtual_focus = Some((entry.id, focus.clone()));
                                    state.playback_focus = focus.filter(|_| playback);
                                }
                                Action::Click
                                    if matches!(entry.target, Target::ShowPlaybackControls) =>
                                {
                                    let _ = view.update(cx, |s, cx| {
                                        s.step_player_focus(true, w, cx);
                                    });
                                    bridge.borrow_mut().playback_focus = w.focused(cx);
                                }
                                Action::Click
                                    if matches!(
                                        entry.target,
                                        Target::Button | Target::Poster(_)
                                    ) =>
                                {
                                    click(entry.bounds.center(), w);
                                }
                                Action::SetValue => match (entry.target, request.data) {
                                    (Target::Input(input), Some(ActionData::Value(value))) => {
                                        input.update(cx, |state, cx| {
                                            state.set_value(value.into_string(), w, cx)
                                        });
                                    }
                                    (
                                        Target::Seek(duration),
                                        Some(ActionData::NumericValue(seconds)),
                                    ) if seconds.is_finite() => {
                                        let _ = view.update(cx, |s, cx| {
                                            s.seek_to(seconds.clamp(0., duration), cx)
                                        });
                                    }
                                    (Target::Volume, Some(ActionData::NumericValue(percent)))
                                        if percent.is_finite() =>
                                    {
                                        let _ = view
                                            .update(cx, |s, cx| s.volume(Some(percent / 100.), cx));
                                    }
                                    _ => {}
                                },
                                _ => {}
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Some(Self { _bridge: bridge })
    }
}

fn click(position: Point<Pixels>, w: &Window) {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE},
    };
    let Ok(hwnd) = crate::player::parent(w) else {
        return;
    };
    let coordinate = |value: Pixels| (f32::from(value) * w.scale_factor()).round() as u16;
    let point = LPARAM(
        (u32::from(coordinate(position.y)) << 16 | u32::from(coordinate(position.x))) as isize,
    );
    // Dispatch to our own GPUI window. This reuses the control's normal hit test and handler.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), point);
    }
}

pub(super) fn frame() -> Option<impl IntoElement> {
    let bridge = bridge().filter(|bridge| bridge.borrow().active)?;
    Some(
        canvas(
            |_, _, _| {},
            move |_, _, w, cx| {
                let generation = {
                    let mut state = bridge.borrow_mut();
                    state.entries.clear();
                    state.repeated_ids.clear();
                    state.generation += 1;
                    state.generation
                };
                let bridge = bridge.clone();
                w.defer(cx, move |w, cx| {
                    let mut state = bridge.borrow_mut();
                    if generation != state.generation {
                        return;
                    }
                    let events = state.publish(w, cx);
                    drop(state);
                    if let Some(events) = events {
                        events.raise();
                    }
                });
            },
        )
        .absolute()
        .size_0(),
    )
}

impl Bridge {
    fn publish(&mut self, w: &Window, cx: &App) -> Option<accesskit_windows::QueuedEvents> {
        let mut entries = std::mem::take(&mut self.entries);
        entries.sort_by(|a, b| {
            f32::from(a.bounds.top())
                .total_cmp(&f32::from(b.bounds.top()))
                .then_with(|| f32::from(a.bounds.left()).total_cmp(&f32::from(b.bounds.left())))
        });
        let focus = self
            .virtual_focus
            .as_ref()
            .and_then(|(id, handle)| {
                (*handle == w.focused(cx)
                    && entries.iter().any(|entry| {
                        entry.id == *id
                            && (!matches!(entry.target, Target::Poster(_)) || entry.focused)
                    }))
                .then_some(*id)
            })
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.focused)
                    .map(|entry| entry.id)
            })
            .unwrap_or(ROOT);
        let mut root = Node::new(Role::Window);
        root.set_label("RustyFin");
        root.set_children(entries.iter().map(|entry| entry.id).collect::<Vec<_>>());
        let nodes: Vec<_> = std::iter::once((ROOT, root))
            .chain(entries.iter().map(|entry| (entry.id, entry.node.clone())))
            .collect();
        self.visible = entries.into_iter().map(|entry| (entry.id, entry)).collect();
        if self
            .last
            .as_ref()
            .is_some_and(|(old_focus, old_nodes)| *old_focus == focus && old_nodes == &nodes)
        {
            return None;
        }
        let previous = self.last.replace((focus, nodes.clone()));
        self.adapter.update_if_active(move || TreeUpdate {
            nodes,
            tree: previous.is_none().then(|| Tree::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus,
        })
    }
}

fn marker(
    key: ElementId,
    mut node: Node,
    focus: Option<FocusHandle>,
    target: Target,
    selected: bool,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, w, _| {
            let bounds = bounds.intersect(&w.content_mask().bounds);
            if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
                return;
            }
            let Some(bridge) = bridge() else { return };
            let scale = f64::from(w.scale_factor());
            let at = |value: Pixels| f64::from(f32::from(value)) * scale;
            node.set_bounds(Rect::new(
                at(bounds.left()),
                at(bounds.top()),
                at(bounds.right()),
                at(bounds.bottom()),
            ));
            let mut hasher = DefaultHasher::new();
            key.hash(&mut hasher);
            node.label().hash(&mut hasher);
            let hash = hasher.finish();
            let mut state = bridge.borrow_mut();
            let ordinal = state.repeated_ids.entry(hash).or_insert(0);
            let id = NodeId(hash.wrapping_add(*ordinal).max(1));
            *ordinal += 1;
            let focused = selected || focus.as_ref().is_some_and(|f| f.is_focused(w));
            state.entries.push(Entry {
                id,
                key,
                node,
                bounds,
                focus,
                target,
                focused,
            });
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

pub(super) fn button(
    control: Stateful<Div>,
    name: impl Into<SharedString>,
    focus: Option<FocusHandle>,
    selected: bool,
) -> Stateful<Div> {
    control_button(control, name.into(), focus, Target::Button, selected, None)
}

pub(super) fn switch(control: Stateful<Div>, name: SharedString, on: bool) -> Stateful<Div> {
    control_button(control, name, None, Target::Button, false, Some(on))
}

fn control_button(
    control: Stateful<Div>,
    name: SharedString,
    focus: Option<FocusHandle>,
    target: Target,
    selected: bool,
    toggled: Option<bool>,
) -> Stateful<Div> {
    if !active() {
        return control;
    }
    let key = Element::id(&control).expect("accessible control has an ID");
    let mut node = Node::new(if toggled.is_some() {
        Role::Switch
    } else {
        Role::Button
    });
    node.set_label(name.as_str());
    if let Some(on) = toggled {
        node.set_toggled(if on { Toggled::True } else { Toggled::False });
    }
    node.add_action(Action::Focus);
    node.add_action(Action::Click);
    control.child(marker(key, node, focus, target, selected))
}

pub(super) fn title_button(
    control: Stateful<Div>,
    item: &crate::jellyfin::Item,
    focus: Option<FocusHandle>,
    selected: bool,
) -> Stateful<Div> {
    if !active() {
        return control;
    }
    button(control, title_name(item), focus, selected)
}

pub(super) fn poster_button(
    control: Stateful<Div>,
    item: &crate::jellyfin::Item,
    poster: Poster,
    selected: bool,
) -> Stateful<Div> {
    if !active() {
        return control;
    }
    control_button(
        control,
        title_name(item),
        None,
        Target::Poster(poster),
        selected,
        None,
    )
}

pub(super) fn heading(view: Div, name: SharedString, level: usize) -> Div {
    if !active() {
        return view;
    }
    let mut node = Node::new(Role::Heading);
    node.set_label(name.as_str());
    node.set_level(level - 1);
    view.relative()
        .child(marker(name.into(), node, None, Target::None, false))
}

pub(super) fn readable_text(view: Div, id: &'static str, text: &str, announce: bool) -> Div {
    if !active() {
        return view;
    }
    let mut node = Node::new(Role::Label);
    node.set_value(text);
    if announce {
        node.set_live(Live::Polite);
    }
    view.relative()
        .child(marker(id.into(), node, None, Target::None, false))
}

fn title_name(item: &crate::jellyfin::Item) -> SharedString {
    let caption = item.caption();
    if caption.is_empty() {
        item.name.clone().into()
    } else {
        format!("{}, {caption}", item.name).into()
    }
}

pub(super) fn focus(control: Stateful<Div>, handle: &FocusHandle) -> Stateful<Div> {
    let control = control.track_focus(handle);
    if !active() {
        return control;
    }
    let key = Element::id(&control).expect("accessible control has an ID");
    let handle = handle.clone();
    control.child(
        canvas(
            |_, _, _| {},
            move |_, _, w, _| {
                if let Some(bridge) = bridge()
                    && let Some(entry) = bridge
                        .borrow_mut()
                        .entries
                        .iter_mut()
                        .rev()
                        .find(|entry| entry.key == key)
                {
                    entry.focused = handle.is_focused(w);
                    entry.focus = Some(handle);
                }
            },
        )
        .absolute()
        .inset_0()
        .size_full(),
    )
}

pub(super) fn keyboard_focus(control: Stateful<Div>, cx: &App) -> Stateful<Div> {
    let Some(bridge) = bridge().filter(|bridge| bridge.borrow().active) else {
        return control;
    };
    let key = Element::id(&control).expect("accessible control has an ID");
    let handle = bridge
        .borrow_mut()
        .handles
        .entry(key)
        .or_insert_with(|| cx.focus_handle())
        .clone();
    focus(control, &handle)
}

pub(super) fn menu_item(name: impl Into<SharedString>) -> gpui_component::menu::PopupMenuItem {
    let name = name.into();
    gpui_component::menu::PopupMenuItem::element(move |_, _| {
        let mut row = div().relative().w_full().truncate().child(name.clone());
        if active() {
            let mut node = Node::new(Role::MenuItem);
            node.set_label(name.as_str());
            node.add_action(Action::Click);
            row = row.child(marker(
                name.clone().into(),
                node,
                None,
                Target::Button,
                false,
            ));
        }
        row
    })
}

pub(super) fn field(
    input: Input,
    state: &Entity<InputState>,
    name: &'static str,
    masked: bool,
    cx: &App,
) -> Div {
    let mut field = div().relative().w_full().child(input);
    if active() {
        let value = state.read(cx);
        let mut node = Node::new(if masked {
            Role::PasswordInput
        } else {
            Role::TextInput
        });
        node.set_label(name);
        node.add_action(Action::Focus);
        if !masked {
            node.set_value(value.value().as_str());
            node.add_action(Action::SetValue);
        }
        field = field.child(marker(
            name.into(),
            node,
            Some(value.focus_handle(cx)),
            if masked {
                Target::None
            } else {
                Target::Input(state.clone())
            },
            false,
        ));
    }
    field
}

pub(super) fn status(text: &str) -> Option<impl IntoElement> {
    active().then(|| {
        let mut node = Node::new(Role::Status);
        node.set_value(text);
        node.set_live(Live::Polite);
        marker("live-status".into(), node, None, Target::None, false)
    })
}

pub(super) fn show_playback_controls() -> Option<impl IntoElement> {
    active().then(|| {
        let name = "Show playback controls";
        let mut node = Node::new(Role::Button);
        node.set_label(name);
        node.add_action(Action::Focus);
        node.add_action(Action::Click);
        marker(name.into(), node, None, Target::ShowPlaybackControls, false)
    })
}

pub(super) fn timeline(
    control: Stateful<Div>,
    duration: f64,
    position: f64,
    focus: &FocusHandle,
) -> Stateful<Div> {
    slider(
        control,
        "Playback position",
        duration,
        position,
        focus,
        Target::Seek(duration),
    )
}

pub(super) fn volume(control: Stateful<Div>, value: f64, focus: &FocusHandle) -> Stateful<Div> {
    slider(control, "Volume", 100., value * 100., focus, Target::Volume)
}

fn slider(
    control: Stateful<Div>,
    name: &'static str,
    maximum: f64,
    value: f64,
    focus: &FocusHandle,
    target: Target,
) -> Stateful<Div> {
    if !active() || maximum <= 0. || !maximum.is_finite() {
        return control;
    }
    let mut node = Node::new(Role::Slider);
    node.set_label(name);
    node.set_min_numeric_value(0.);
    node.set_max_numeric_value(maximum);
    node.set_numeric_value(value.clamp(0., maximum));
    node.add_action(Action::Focus);
    node.add_action(Action::SetValue);
    control.child(marker(
        name.into(),
        node,
        Some(focus.clone()),
        target,
        false,
    ))
}
