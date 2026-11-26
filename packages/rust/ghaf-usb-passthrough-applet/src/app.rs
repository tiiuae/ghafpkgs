/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use std::collections::HashSet;

use cosmic::app::Core;
use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::advanced::text;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::iced::{Length, Limits, Subscription, window};
use cosmic::widget::{self, divider, icon};
use cosmic::{Application, Element, Task};

use crate::{api, config};

const ICON: &str = "drive-harddisk-usb-symbolic";
const ICON_ATTENTION: &str = "dialog-warning-symbolic";
const POPUP_WIDTH: f32 = 300.0;
/// How long the new-device notification stays on screen.
const NOTIFICATION_TIMEOUT_MS: u32 = 10_000;
/// Line heights of `text::body` and `text::caption`, the two lines of a device row.
const BODY_LINE_HEIGHT: f32 = 21.0;
const CAPTION_LINE_HEIGHT: f32 = 17.0;
/// Closed device rows shown before the list starts scrolling.
const MAX_VISIBLE_ROWS: f32 = 5.0;
/// Vertical padding of a VM option (`menu_button` padding `[8, 48]`).
const OPTION_PADDING: f32 = 8.0;

pub fn run(port: u32) -> cosmic::iced::Result {
    cosmic::applet::run::<UsbPassthrough>(port)
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(window::Id),
    Loaded(Result<Vec<api::Entry>, String>),
    /// Open or close a device's VM choices.
    Toggle(String),
    /// (device node, option index)
    Select(String, usize),
    Attached(Result<(), String>),
    Event(api::Event),
}

pub struct UsbPassthrough {
    core: Core,
    port: u32,
    popup: Option<window::Id>,
    devices: Vec<api::Entry>,
    /// Device nodes that the host asked the user to assign to a VM.
    pending: HashSet<String>,
    /// Device node whose VM choices are shown.
    expanded: Option<String>,
    load_error: Option<String>,
    attach_error: Option<String>,
}

impl Application for UsbPassthrough {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = u32;
    type Message = Message;
    const APP_ID: &'static str = config::APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, port: Self::Flags) -> (Self, Task<cosmic::Action<Message>>) {
        let app = Self {
            core,
            port,
            popup: None,
            devices: Vec::new(),
            pending: HashSet::new(),
            expanded: None,
            load_error: None,
            attach_error: None,
        };
        let task = Task::perform(api::list_devices_retry(port), |r| {
            cosmic::Action::App(Message::Loaded(r))
        });
        (app, task)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Message> {
        let icon_name = if self.pending.is_empty() {
            ICON
        } else {
            ICON_ATTENTION
        };
        self.core
            .applet
            .icon_button(icon_name)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let spacing = cosmic::theme::spacing();

        // Devices waiting for a VM choice come first, always open; the rest
        // are sorted by name and open one at a time.
        let mut devices: Vec<&api::Entry> = self.devices.iter().collect();
        devices.sort_by_key(|e| e.name.to_lowercase());
        let (pending, attached): (Vec<_>, Vec<_>) = devices
            .into_iter()
            .partition(|e| self.pending.contains(&e.device_node));

        // The open row's VM options add to the list height instead of scrolling.
        let open_options = attached
            .iter()
            .find(|e| self.expanded.as_ref() == Some(&e.device_node))
            .map_or(0, |e| e.options.len());

        let mut list = widget::column::with_capacity(attached.len().max(1));
        if self.devices.is_empty() {
            list = list.push(padded_control(widget::text::body(
                "No USB devices detected",
            )));
        }
        for entry in attached {
            let open = self.expanded.as_ref() == Some(&entry.device_node);
            list = list.push(device_row(entry, open, false));
        }

        let row_height =
            2.0 * f32::from(spacing.space_xxs) + BODY_LINE_HEIGHT + CAPTION_LINE_HEIGHT;
        let separator = || {
            padded_control(divider::horizontal::default())
                .padding([spacing.space_xxs, spacing.space_s])
        };
        let errors = self.load_error.iter().chain(&self.attach_error);
        let content = widget::column::with_capacity(4)
            .extend(errors.map(|e| padded_control(widget::text::body(e.as_str())).into()))
            .extend(pending.iter().map(|e| device_row(e, true, true)))
            .push_maybe((!pending.is_empty()).then(separator))
            .push(widget::container(widget::scrollable(list)).max_height(
                row_height * MAX_VISIBLE_ROWS
                    + open_options as f32 * (2.0 * OPTION_PADDING + BODY_LINE_HEIGHT),
            ))
            .padding([8, 0, 8, 0])
            .width(Length::Fixed(POPUP_WIDTH));

        self.core.applet.popup_container(content).into()
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
                let open = cosmic::surface::action::app_popup(
                    |_| Default::default(),
                    |app: &mut Self| {
                        let new_id = window::Id::unique();
                        app.popup = Some(new_id);
                        let mut popup_settings = app.core.applet.get_popup_settings(
                            app.core.main_window_id().unwrap(),
                            new_id,
                            None,
                            None,
                            None,
                        );
                        popup_settings.positioner.size_limits = Limits::NONE
                            .min_width(POPUP_WIDTH)
                            .max_width(POPUP_WIDTH)
                            .min_height(100.0);
                        popup_settings
                    },
                    None,
                );
                self.attach_error = None;
                self.expanded = None;
                return Task::batch([cosmic::surface::surface_task(open), self.refresh()]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Toggle(node) => {
                self.expanded = (self.expanded.as_ref() != Some(&node)).then_some(node);
            }
            Message::Loaded(Ok(devices)) => {
                self.pending
                    .retain(|node| devices.iter().any(|d| &d.device_node == node));
                self.devices = devices;
                self.load_error = None;
            }
            Message::Loaded(Err(e)) => {
                tracing::error!("{e}");
                self.load_error = Some(e);
            }
            Message::Select(node, i) => {
                let Some(entry) = self.devices.iter_mut().find(|e| e.device_node == node) else {
                    return Task::none();
                };
                let Some(vm) = entry.options.get(i).cloned() else {
                    return Task::none();
                };
                if vm == entry.vm {
                    return Task::none();
                }
                tracing::info!("Passing {} ({}) to {vm}", entry.name, entry.device_node);
                entry.vm.clone_from(&vm);
                self.pending.remove(&entry.device_node);
                self.expanded = None;
                self.attach_error = None;
                return Task::perform(api::attach(self.port, entry.device_node.clone(), vm), |r| {
                    cosmic::Action::App(Message::Attached(r))
                });
            }
            Message::Attached(Ok(())) => {}
            Message::Attached(Err(e)) => {
                tracing::error!("Device error: {e}");
                self.attach_error = Some(format!("Device error: {e}"));
                // Reload so the row shows the device's real state again.
                return self.refresh();
            }
            Message::Event(event) => {
                tracing::debug!("vhotplug event: {event:?}");
                if let Some((node, name)) = api::pending_device(&event) {
                    self.pending.insert(node);
                    return Task::batch([notify_new_device(name), self.refresh()]);
                }
                return self.refresh();
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(self.port, api::events).map(Message::Event)
    }
}

impl UsbPassthrough {
    fn refresh(&self) -> Task<cosmic::Action<Message>> {
        Task::perform(api::list_devices(self.port), |r| {
            cosmic::Action::App(Message::Loaded(r))
        })
    }
}

/// A device and its current VM; when open, followed by the VMs it can be passed to.
fn device_row(entry: &api::Entry, open: bool, pending: bool) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();
    let vm = if entry.vm.eq_ignore_ascii_case("none") {
        "Not attached"
    } else {
        entry.vm.as_str()
    };
    let head = menu_button(
        widget::row::with_capacity(2)
            .push_maybe(pending.then(|| icon::from_name(ICON_ATTENTION).size(16)))
            .push(
                widget::column::with_capacity(2)
                    .push(
                        widget::text::body(entry.name.clone())
                            .wrapping(text::Wrapping::None)
                            .ellipsize(text::Ellipsize::End(text::EllipsizeHeightLimit::Lines(1)))
                            .width(Length::Fill),
                    )
                    .push(widget::text::caption(vm.to_string()))
                    .width(Length::Fill),
            )
            .spacing(spacing.space_xs)
            .align_y(Vertical::Center),
    )
    .on_press(Message::Toggle(entry.device_node.clone()));
    // Full name on hover. The tooltip wraps the whole button: libcosmic's button
    // offsets overlays of its content by its own position a second time.
    let head = widget::tooltip(
        head,
        widget::text::body(entry.name.clone()),
        widget::tooltip::Position::Bottom,
    );

    let mut row = widget::column::with_capacity(1 + entry.options.len()).push(head);
    if open {
        let current = entry.options.iter().position(|o| *o == entry.vm);
        for (i, option) in entry.options.iter().enumerate() {
            let node = entry.device_node.clone();
            let select = Message::Select(node.clone(), i);
            row = row.push(
                menu_button(widget::radio(
                    widget::text::body(option.clone()),
                    i,
                    current,
                    move |i| Message::Select(node, i),
                ))
                .padding([OPTION_PADDING as u16, 48])
                .on_press(select),
            );
        }
    }
    row.into()
}

fn notify_new_device(name: String) -> Task<cosmic::Action<Message>> {
    Task::future(async move {
        let shown = tokio::task::spawn_blocking(move || {
            notify_rust::Notification::new()
                .appname("USB Passthrough")
                .summary(&format!("New USB device: {name}"))
                .body("Open the USB passthrough applet to assign it to a VM")
                .icon(ICON)
                .timeout(notify_rust::Timeout::Milliseconds(NOTIFICATION_TIMEOUT_MS))
                .show()
        })
        .await;
        if let Ok(Err(e)) = shown {
            tracing::warn!("Failed to show notification: {e}");
        }
        cosmic::Action::None
    })
}
