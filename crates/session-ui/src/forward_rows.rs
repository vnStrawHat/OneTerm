//! The "Port forwards" rows of the session dialog (IN-0023 / US-0059): one
//! editable row per forward plus an Add button; rows are parsed and validated
//! on Save.

use std::cell::{Cell, RefCell};
use std::net::IpAddr;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Entity, IntoElement, ParentElement as _, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    select::{Select, SelectState},
    v_flex,
};
use oneterm_core::{PortForward, loopback};
use oneterm_state::form_dialog::{FieldRequirement, labelled_field};

const KINDS: [&str; 3] = ["Local", "Remote", "Dynamic"];

struct ForwardRow {
    id: usize,
    kind: Entity<SelectState<Vec<String>>>,
    bind: Entity<InputState>,
    bind_port: Entity<InputState>,
    target_host: Entity<InputState>,
    target_port: Entity<InputState>,
}

impl ForwardRow {
    fn new(id: usize, forward: Option<&PortForward>, window: &mut Window, cx: &mut App) -> Self {
        let (kind_index, bind, bind_port, target_host, target_port) = match forward {
            Some(PortForward::Local {
                bind,
                bind_port,
                target_host,
                target_port,
            }) => (
                0,
                bind.to_string(),
                bind_port.to_string(),
                target_host.clone(),
                target_port.to_string(),
            ),
            Some(PortForward::Remote {
                bind_host,
                bind_port,
                target_host,
                target_port,
            }) => (
                1,
                bind_host.clone(),
                bind_port.to_string(),
                target_host.clone(),
                target_port.to_string(),
            ),
            Some(PortForward::Dynamic { bind, bind_port }) => (
                2,
                bind.to_string(),
                bind_port.to_string(),
                String::new(),
                String::new(),
            ),
            None => (
                0,
                loopback().to_string(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        let input =
            |placeholder: &'static str, value: String, window: &mut Window, cx: &mut App| {
                cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder(placeholder);
                    if !value.is_empty() {
                        state.set_value(value, window, cx);
                    }
                    state
                })
            };
        let kinds: Vec<String> = KINDS.iter().map(|kind| (*kind).to_string()).collect();
        let kind = cx.new(|cx| {
            SelectState::new(
                kinds,
                Some(IndexPath::default().row(kind_index)),
                window,
                cx,
            )
        });
        Self {
            id,
            kind,
            bind: input("127.0.0.1", bind, window, cx),
            bind_port: input("port", bind_port, window, cx),
            target_host: input("target host", target_host, window, cx),
            target_port: input("port", target_port, window, cx),
        }
    }

    fn kind_index(&self, cx: &App) -> usize {
        self.kind
            .read(cx)
            .selected_index(cx)
            .map(|index| index.row)
            .unwrap_or(0)
    }

    fn parse(&self, cx: &App) -> Result<PortForward, String> {
        let text = |state: &Entity<InputState>| state.read(cx).value().trim().to_string();
        let port = |state: &Entity<InputState>, name: &str| -> Result<u16, String> {
            text(state)
                .parse::<u16>()
                .map_err(|_| format!("Port forward: the {name} port must be a number 0..65535."))
        };
        let bind_ip = || -> Result<IpAddr, String> {
            text(&self.bind)
                .parse::<IpAddr>()
                .map_err(|_| "Port forward: the bind address must be an IP address.".to_string())
        };
        let forward = match self.kind_index(cx) {
            0 => PortForward::Local {
                bind: bind_ip()?,
                bind_port: port(&self.bind_port, "listening")?,
                target_host: text(&self.target_host),
                target_port: port(&self.target_port, "target")?,
            },
            1 => PortForward::Remote {
                bind_host: text(&self.bind),
                bind_port: port(&self.bind_port, "listening")?,
                target_host: text(&self.target_host),
                target_port: port(&self.target_port, "target")?,
            },
            _ => PortForward::Dynamic {
                bind: bind_ip()?,
                bind_port: port(&self.bind_port, "listening")?,
            },
        };
        forward
            .validate()
            .map_err(|error| format!("Port forward {}: {error}.", forward.summary()))?;
        Ok(forward)
    }
}

/// The editable forward list of one session dialog.
#[derive(Clone)]
pub(crate) struct PortForwardRows {
    rows: Rc<RefCell<Vec<ForwardRow>>>,
    next_id: Rc<Cell<usize>>,
}

impl PortForwardRows {
    pub(crate) fn new(forwards: &[PortForward], window: &mut Window, cx: &mut App) -> Self {
        let rows = forwards
            .iter()
            .enumerate()
            .map(|(id, forward)| ForwardRow::new(id, Some(forward), window, cx))
            .collect();
        Self {
            rows: Rc::new(RefCell::new(rows)),
            next_id: Rc::new(Cell::new(forwards.len())),
        }
    }

    /// Parse every row; the first invalid row or duplicate listener is the error.
    pub(crate) fn take(&self, cx: &App) -> Result<Vec<PortForward>, String> {
        let forwards: Vec<PortForward> = self
            .rows
            .borrow()
            .iter()
            .map(|row| row.parse(cx))
            .collect::<Result<_, _>>()?;
        for (index, forward) in forwards.iter().enumerate() {
            if forwards[..index]
                .iter()
                .any(|earlier| earlier.bind_key() == forward.bind_key())
            {
                return Err(format!(
                    "Port forward {}: the same listener is used twice.",
                    forward.summary()
                ));
            }
        }
        Ok(forwards)
    }

    pub(crate) fn render(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let warning = theme.warning;
        let rows_for_add = self.rows.clone();
        let next_id = self.next_id.clone();
        let add = Button::new("add-port-forward")
            .ghost()
            .small()
            .label("Add")
            .icon(Icon::new(IconName::Plus))
            .on_click(move |_, window, cx| {
                let id = next_id.get();
                next_id.set(id + 1);
                rows_for_add
                    .borrow_mut()
                    .push(ForwardRow::new(id, None, window, cx));
                window.refresh();
            });
        let rows = self.rows.borrow();
        let list = v_flex().gap_2().w_full().children(rows.iter().map(|row| {
            let kind_index = row.kind_index(cx);
            let has_target = kind_index != 2;
            let bind_text = row.bind.read(cx).value().trim().to_string();
            let exposed = !bind_text.is_empty()
                && !matches!(bind_text.as_str(), "127.0.0.1" | "::1" | "localhost")
                && bind_text
                    .parse::<IpAddr>()
                    .map(|ip| !ip.is_loopback())
                    .unwrap_or(true);
            let remove_id = row.id;
            let rows_for_remove = self.rows.clone();
            v_flex()
                .gap_1()
                .w_full()
                .child(
                    h_flex()
                        .gap_1()
                        .w_full()
                        .items_center()
                        .child(div().w(px(104.)).child(Select::new(&row.kind).small()))
                        .child(Input::new(&row.bind).small().flex_1())
                        .child(div().w(px(64.)).child(Input::new(&row.bind_port).small()))
                        .when(has_target, |row_el| {
                            row_el
                                .child(div().text_color(muted).child("->"))
                                .child(Input::new(&row.target_host).small().flex_1())
                                .child(div().w(px(64.)).child(Input::new(&row.target_port).small()))
                        })
                        .when(!has_target, |row_el| {
                            row_el.child(div().text_sm().text_color(muted).child("SOCKS5"))
                        })
                        .child(
                            Button::new(("remove-port-forward", remove_id))
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(IconName::Close))
                                .on_click(move |_, window, _cx| {
                                    rows_for_remove
                                        .borrow_mut()
                                        .retain(|candidate| candidate.id != remove_id);
                                    window.refresh();
                                }),
                        ),
                )
                .when(exposed, |row_el| {
                    row_el.child(
                        div()
                            .text_sm()
                            .text_color(warning)
                            .child("This listener is reachable from other machines."),
                    )
                })
        }));
        labelled_field(
            "Port forwards",
            FieldRequirement::Optional,
            v_flex()
                .gap_2()
                .w_full()
                .child(list)
                .child(h_flex().child(add)),
            cx,
        )
    }
}
