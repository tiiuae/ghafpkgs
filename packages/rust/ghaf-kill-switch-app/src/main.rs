/*
 * SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use cosmic::app::Core;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::{self, icon, toggler};
use cosmic::{Application, Element};
use serde::{Deserialize, Serialize};
use std::process::Command;

const ID: &str = "ae.tii.CosmicAppletKillSwitch";

#[derive(Debug, Clone)]
pub enum Message {
    ToggleMicrophone(bool),
    ToggleCamera(bool),
    ToggleWiFi(bool),
    TogglePopup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    microphone_enabled: bool,
    camera_enabled: bool,
    wifi_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            microphone_enabled: true,
            camera_enabled: true,
            wifi_enabled: true,
        }
    }
}

pub struct KillSwitch {
    core: Core,
    config: Config,
    popup: Option<window::Id>,
}

impl Application for KillSwitch {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::Task<cosmic::Action<Self::Message>>) {
        let app = Self {
            core,
            config: Config::default(),
            popup: None,
        };
        (app, cosmic::Task::none())
    }

    fn view(&self) -> Element<'_, Self::Message> {
        tracing::info!("Rendering view");
        widget::button::icon(icon::from_name("security-high-symbolic"))
            .on_press(Message::TogglePopup)
            .padding(8)
            .into()
    }

    fn view_window(&self, id: cosmic::iced::window::Id) -> Element<'_, Self::Message> {
        tracing::info!(
            "=== view_window called for id: {:?}, popup: {:?} ===",
            id,
            self.popup
        );

        // Check if this is our popup window
        if self.popup == Some(id) {
            let spacing = self.core.system_theme().cosmic().spacing;

            let content = widget::column::with_capacity(4)
                .push(
                    widget::container(widget::text("Privacy Controls").size(14))
                        .width(Length::Fixed(280.0))
                        .padding([spacing.space_xs, spacing.space_m]),
                )
                .push(self.create_control_row(
                    "microphone-sensitivity-medium-symbolic",
                    "Microphone",
                    self.config.microphone_enabled,
                    Message::ToggleMicrophone,
                ))
                .push(self.create_control_row(
                    "camera-photo-symbolic",
                    "Camera",
                    self.config.camera_enabled,
                    Message::ToggleCamera,
                ))
                .push(self.create_control_row(
                    "network-wireless-symbolic",
                    "Wi-Fi",
                    self.config.wifi_enabled,
                    Message::ToggleWiFi,
                ))
                .spacing(1);

            return self.core.applet.popup_container(content).into();
        }

        // Return empty element for other windows
        widget::text("").into()
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        tracing::info!("Update called with message: {:?}", message);
        match message {
            Message::ToggleMicrophone(enabled) => {
                self.config.microphone_enabled = enabled;
                tracing::info!("Microphone toggled: {}", enabled);
                // TODO: Implement actual microphone control via system APIs
                cosmic::Task::none()
            }
            Message::ToggleCamera(enabled) => {
                self.config.camera_enabled = enabled;
                tracing::info!("Camera toggled: {}", enabled);
                // TODO: Implement actual camera control via system APIs
                cosmic::Task::none()
            }
            Message::ToggleWiFi(enabled) => {
                self.config.wifi_enabled = enabled;
                tracing::info!("WiFi toggled: {}", enabled);
                // TODO: Implement actual wifi control via system APIs
                cosmic::Task::none()
            }
            Message::TogglePopup => {
                tracing::info!("!!! Toggle popup clicked !!!");

                if let Some(p) = self.popup.take() {
                    tracing::info!("Destroying popup");
                    return destroy_popup(p);
                } else {
                    tracing::info!("Creating popup");
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
                        .min_width(180.0)
                        .min_height(200.0)
                        .max_width(180.0)
                        .max_height(200.0);

                    return get_popup(popup_settings);
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

impl KillSwitch {
    fn create_control_row<'a>(
        &self,
        icon_name: &'a str,
        label: &'a str,
        enabled: bool,
        on_toggle: fn(bool) -> Message,
    ) -> Element<'a, Message> {
        let spacing = self.core.system_theme().cosmic().spacing;
        let status_text = if enabled { "Enabled" } else { "Disabled" };

        let icon_widget = widget::container(icon::from_name(icon_name).size(32))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center);

        let text_column = widget::column::with_capacity(2)
            .push(widget::text(label).size(14))
            .push(widget::text(status_text).size(12))
            .spacing(2);

        let toggle = toggler(enabled).on_toggle(on_toggle);

        let content = widget::container(
            widget::row::with_capacity(3)
                .push(icon_widget)
                .push(text_column)
                .push(widget::horizontal_space())
                .push(toggle)
                .spacing(spacing.space_s),
        )
        .padding([spacing.space_xs, spacing.space_m])
        .width(Length::Fixed(280.0));

        widget::tooltip(
            content,
            widget::text(format!("Control the {} device", label)),
            widget::tooltip::Position::Bottom,
        )
        .into()
    }
}

fn main() -> cosmic::iced::Result {
    // Initialize tracing for debugging
    tracing_subscriber::fmt::init();

    cosmic::applet::run::<KillSwitch>(())
}
