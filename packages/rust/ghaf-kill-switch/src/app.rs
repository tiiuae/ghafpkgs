/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use cosmic::app::Core;
use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{Length, Limits, Subscription, window};
use cosmic::widget::{self, divider, icon, toggler};
use cosmic::{Application, Element, Task};

use crate::config;
use crate::killswitch::{self, Device, Status};

const ICON: &str = "security-high-symbolic";
const POPUP_WIDTH: f32 = 290.0;
const ICON_SIZE: u16 = 32;

pub fn run(port: u32) -> cosmic::iced::Result {
    cosmic::applet::run::<KillSwitch>(port)
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(window::Id),
    Toggle(Device, bool),
    /// Block (`true`) or unblock (`false`) every device.
    ToggleAll(bool),
    /// A device was attached to or detached from a VM.
    Changed,
    /// Device state read at the given generation.
    Loaded(u64, Status),
    /// A block or unblock command for these devices finished; `Err` carries
    /// its message.
    Applied(Vec<Device>, Result<(), String>),
}

pub struct KillSwitch {
    core: Core,
    /// Device manager vsock port.
    port: u32,
    status: Status,
    /// Devices with a block or unblock command still running.
    busy: Vec<Device>,
    /// Bumped by every toggle and status read, so that only the latest read
    /// is applied, and only if no toggle happened since it started.
    generation: u64,
    popup: Option<window::Id>,
}

impl Application for KillSwitch {
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
        let mut app = Self {
            core,
            port,
            status: Status::default(),
            busy: Vec::new(),
            generation: 0,
            popup: None,
        };
        let task = app.refresh();
        (app, task)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(ICON)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let spacing = cosmic::theme::spacing();

        if self.status.present().next().is_none() {
            return self
                .core
                .applet
                .popup_container(
                    padded_control(widget::text::body("No devices to block"))
                        .width(Length::Fixed(POPUP_WIDTH))
                        .padding([spacing.space_m, spacing.space_m]),
                )
                .into();
        }

        // A button rather than a toggle: it has no on state to contradict the
        // device toggles, and it still makes sense when only some are blocked.
        let all_blocked = self.status.all_blocked();
        let block_all = widget::row::with_capacity(2)
            .push(row_icon(ICON))
            .push(widget::text::body(if all_blocked {
                "Unblock all"
            } else {
                "Block all"
            }))
            .spacing(spacing.space_s)
            .align_y(Vertical::Center);

        let mut content = widget::column::with_capacity(3 + Device::ALL.len())
            .push(
                menu_button(block_all).on_press_maybe(
                    self.busy
                        .is_empty()
                        .then_some(Message::ToggleAll(!all_blocked)),
                ),
            )
            .push(
                padded_control(divider::horizontal::default())
                    .padding([spacing.space_xxs, spacing.space_s]),
            );

        for (device, enabled) in self.status.present() {
            let busy = self.busy.contains(&device);
            let status_text = match (busy, enabled) {
                (true, true) => "Unblocking…",
                (true, false) => "Blocking…",
                (false, true) => "Allowed",
                (false, false) => "Blocked",
            };
            content = content.push(control_row(
                device.icon(),
                device.label(),
                status_text,
                enabled,
                (!busy).then_some(move |enabled| Message::Toggle(device, enabled)),
            ));
        }

        self.core
            .applet
            .popup_container(
                content
                    .width(Length::Fixed(POPUP_WIDTH))
                    // Rows are padded by space_xxs vertically and space_m
                    // horizontally; pad the ends so the popup edges match.
                    .padding([spacing.space_s, 0]),
            )
            .into()
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                }
                let new_id = window::Id::unique();
                self.popup = Some(new_id);

                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                );
                popup_settings.positioner.size_limits = Limits::NONE
                    .min_width(POPUP_WIDTH)
                    .max_width(POPUP_WIDTH)
                    .max_height(300.0);

                // Notifications keep the state current, but may have been
                // missed while the listener was reconnecting.
                return Task::batch([get_popup(popup_settings), self.refresh()]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Toggle(device, enabled) => {
                tracing::info!("{} {}", device.label(), block_word(enabled));
                self.status.set(device, Some(enabled));
                self.busy.push(device);
                self.generation += 1;
                return apply(vec![device], killswitch::set(self.port, device, enabled));
            }
            Message::ToggleAll(block) => {
                let enabled = !block;
                tracing::info!("All devices {}", block_word(enabled));
                self.status.set_all(enabled);
                self.busy = self.status.present().map(|(device, _)| device).collect();
                self.generation += 1;
                return apply(self.busy.clone(), killswitch::set_all(self.port, enabled));
            }
            Message::Changed => return self.refresh(),
            Message::Loaded(generation, status) => {
                if generation != self.generation {
                    return Task::none();
                }
                // Keep the requested state of devices whose command is still
                // running; the read may predate it.
                for device in Device::ALL {
                    if !self.busy.contains(&device) {
                        self.status.set(device, status.get(device));
                    }
                }
            }
            Message::Applied(devices, result) => {
                if let Err(e) = result {
                    tracing::error!("{e}");
                }
                self.busy.retain(|d| !devices.contains(d));
                // Read the real state back, so a row never keeps showing a
                // toggle the device did not follow.
                return self.refresh();
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run_with(self.port, killswitch::changes).map(|()| Message::Changed)
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl KillSwitch {
    /// Read the current device state in the background.
    fn refresh(&mut self) -> Task<cosmic::Action<Message>> {
        self.generation += 1;
        let generation = self.generation;
        let port = self.port;
        Task::future(async move {
            match killswitch::status(port).await {
                Ok(status) => cosmic::Action::App(Message::Loaded(generation, status)),
                Err(e) => {
                    tracing::error!("{e}");
                    cosmic::Action::None
                }
            }
        })
    }
}

/// Run a block or unblock `command` for `devices` and report the outcome.
fn apply(
    devices: Vec<Device>,
    command: impl Future<Output = Result<(), String>> + Send + 'static,
) -> Task<cosmic::Action<Message>> {
    Task::future(async move { cosmic::Action::App(Message::Applied(devices, command.await)) })
}

fn block_word(enabled: bool) -> &'static str {
    if enabled { "unblocked" } else { "blocked" }
}

/// The icon at the start of a popup row.
fn row_icon(name: &'static str) -> Element<'static, Message> {
    widget::container(icon::from_name(name).size(ICON_SIZE))
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .into()
}

/// An icon, a label with a status line, and a toggle. The toggle ignores
/// clicks while `on_toggle` is `None`.
fn control_row(
    icon_name: &'static str,
    label: &'static str,
    status_text: &'static str,
    toggled: bool,
    on_toggle: Option<impl Fn(bool) -> Message + 'static>,
) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();

    let text_column = widget::column::with_capacity(2)
        .push(widget::text::body(label))
        .push(widget::text::caption(status_text))
        // Filling the space keeps every toggle on the right edge, however
        // long the label is.
        .width(Length::Fill);

    let row = widget::row::with_capacity(3)
        .push(row_icon(icon_name))
        .push(text_column)
        .push(toggler(toggled).on_toggle_maybe(on_toggle))
        .spacing(spacing.space_s)
        .align_y(Vertical::Center);

    padded_control(row).into()
}
