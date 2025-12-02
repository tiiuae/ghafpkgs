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
use std::time::Duration;
use systemd_journal_logger::JournalLog;

const ID: &str = "ae.tii.CosmicAppletKillSwitch";

#[derive(Debug, Clone)]
pub enum Message {
    ToggleMicrophone(bool),
    ToggleCamera(bool),
    ToggleWiFi(bool),
    ToggleBT(bool),
    ToggleAll(bool),
    TogglePopup,
    RefreshStatus,
    ConfigLoaded(Config),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    microphone_enabled: bool,
    camera_enabled: bool,
    wifi_enabled: bool,
    bt_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            microphone_enabled: true,
            camera_enabled: true,
            wifi_enabled: true,
            bt_enabled: true,
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
            config: Self::get_config(),
            popup: None,
        };
        (app, cosmic::Task::none())
    }

    fn view(&self) -> Element<'_, Self::Message> {
        log::debug!("Rendering view");
        widget::button::icon(icon::from_name("security-high-symbolic"))
            .on_press(Message::TogglePopup)
            .padding(8)
            .into()
    }

    fn view_window(&self, id: cosmic::iced::window::Id) -> Element<'_, Self::Message> {
        log::debug!(
            "=== view_window called for id: {:?}, popup: {:?} ===",
            id,
            self.popup
        );

        // Check if this is our popup window
        if self.popup == Some(id) {
            let spacing = self.core.system_theme().cosmic().spacing;
            let all_disabled = !self.config.microphone_enabled
                && !self.config.camera_enabled
                && !self.config.wifi_enabled
                && !self.config.bt_enabled;

            let content = widget::column::with_capacity(6)
                .push(
                    widget::container(widget::text("Privacy Controls").size(14))
                        .width(Length::Fixed(280.0))
                        .padding([spacing.space_xs, spacing.space_m]),
                )
                .push(self.create_control_row(
                    "security-high-symbolic",
                    "Block All",
                    all_disabled,
                    Message::ToggleAll,
                    false,
                ))
                .push(
                    cosmic::iced::widget::container(cosmic::iced::widget::Rule::horizontal(1))
                        .width(Length::Fixed(280.0)),
                )
                .push(self.create_control_row(
                    "microphone-sensitivity-medium-symbolic",
                    "Microphone",
                    self.config.microphone_enabled,
                    Message::ToggleMicrophone,
                    true,
                ))
                .push(self.create_control_row(
                    "camera-photo-symbolic",
                    "Camera",
                    self.config.camera_enabled,
                    Message::ToggleCamera,
                    true,
                ))
                .push(self.create_control_row(
                    "network-wireless-symbolic",
                    "Wi-Fi",
                    self.config.wifi_enabled,
                    Message::ToggleWiFi,
                    true,
                ))
                .push(self.create_control_row(
                    "bluetooth-symbolic",
                    "Bluetooth",
                    self.config.bt_enabled,
                    Message::ToggleBT,
                    true,
                ))
                .spacing(1);

            return self.core.applet.popup_container(content).into();
        }

        // Return empty element for other windows
        widget::text("").into()
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        log::debug!("Update called with message: {message:?}");
        match message {
            Message::ToggleMicrophone(enabled) => {
                self.config.microphone_enabled = enabled;
                log::debug!("Microphone toggled: {enabled}");
                cosmic::Task::future(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        Self::run_killswitch_command("mic", enabled);
                    })
                    .await;
                    cosmic::Action::None
                })
            }
            Message::ToggleCamera(enabled) => {
                self.config.camera_enabled = enabled;
                log::debug!("Camera toggled: {enabled}");
                cosmic::Task::future(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        Self::run_killswitch_command("cam", enabled);
                    })
                    .await;
                    cosmic::Action::None
                })
            }
            Message::ToggleWiFi(enabled) => {
                self.config.wifi_enabled = enabled;
                log::debug!("WiFi toggled: {enabled}");
                cosmic::Task::future(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        Self::run_killswitch_command("net", enabled);
                    })
                    .await;
                    cosmic::Action::None
                })
            }
            Message::ToggleBT(enabled) => {
                self.config.bt_enabled = enabled;
                log::debug!("Bluetooth toggled: {enabled}");
                cosmic::Task::future(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        Self::run_killswitch_command("bluetooth", enabled);
                    })
                    .await;
                    cosmic::Action::None
                })
            }
            Message::ToggleAll(enabled_from_toggler) => {
                let enabled = !enabled_from_toggler;
                self.config.microphone_enabled = enabled;
                self.config.camera_enabled = enabled;
                self.config.wifi_enabled = enabled;
                self.config.bt_enabled = enabled;
                log::debug!("All devices toggled: {enabled}");
                cosmic::Task::future(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        Self::run_killswitch_command_all(enabled);
                    })
                    .await;
                    cosmic::Action::None
                })
            }
            Message::TogglePopup => {
                log::debug!("!!! Toggle popup clicked !!!");

                if let Some(p) = self.popup.take() {
                    log::debug!("Destroying popup");
                    destroy_popup(p)
                } else {
                    log::debug!("Creating popup");
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
                        .min_height(250.0)
                        .max_width(180.0)
                        .max_height(300.0);

                    get_popup(popup_settings)
                }
            }
            Message::RefreshStatus => {
                log::debug!("Request to get_config");

                cosmic::Task::perform(
                    tokio::task::spawn_blocking(Self::get_config),
                    |res| match res {
                        Ok(config) => Message::ConfigLoaded(config).into(),
                        Err(_) => {
                            log::error!("Failed to get config from background task");
                            cosmic::Action::None
                        }
                    },
                )
            }

            Message::ConfigLoaded(config) => {
                self.config = config;
                cosmic::Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // Refresh status every 2 seconds when popup is open
        if self.popup.is_some() {
            cosmic::iced::time::every(Duration::from_secs(2)).map(|_| Message::RefreshStatus)
        } else {
            Subscription::none()
        }
    }
}

impl KillSwitch {
    fn run_killswitch_command_all(enabled: bool) {
        let arg = if enabled { "unblock" } else { "block" };
        let output = Command::new("ghaf-killswitch")
            .arg(arg)
            .arg("--all")
            .output()
            .expect("Failed to execute ghaf-killswitch command");

        if output.status.success() {
            log::info!("ghaf-killswitch {arg} --all successful");
        } else {
            log::error!(
                "ghaf-killswitch {} --all failed: {}",
                arg,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    fn get_config() -> Config {
        let output = Command::new("ghaf-killswitch").arg("status").output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let mut config = Config::default();

                    for line in stdout.lines() {
                        let Some((device, status)) = line.split_once(':') else {
                            continue;
                        };

                        let device = device.trim();
                        let enabled = status.trim() == "unblocked";

                        match device {
                            "mic" => config.microphone_enabled = enabled,
                            "cam" => config.camera_enabled = enabled,
                            "net" => config.wifi_enabled = enabled,
                            "bluetooth" => config.bt_enabled = enabled,
                            _ => log::warn!(
                                "Unknown device in ghaf-killswitch status output: {device}"
                            ),
                        }
                    }
                    config
                } else {
                    log::error!(
                        "ghaf-killswitch status command failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    Config::default()
                }
            }
            Err(e) => {
                log::error!("Failed to execute ghaf-killswitch status command: {e}");
                Config::default()
            }
        }
    }

    fn run_killswitch_command(device: &str, enabled: bool) {
        let arg = if enabled { "unblock" } else { "block" };
        let output = Command::new("ghaf-killswitch")
            .arg(arg)
            .arg(device)
            .output()
            .expect("Failed to execute ghaf-killswitch command");

        if output.status.success() {
            log::info!("ghaf-killswitch {arg} {device} successful");
        } else {
            log::error!(
                "ghaf-killswitch {} {} failed: {}",
                arg,
                device,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    fn create_control_row(
        &self,
        icon_name: &'static str,
        label: &'static str,
        enabled: bool,
        on_toggle: fn(bool) -> Message,
        show_status_text: bool,
    ) -> Element<'static, Message> {
        let spacing = self.core.system_theme().cosmic().spacing;
        let status_text = if enabled { "Enabled" } else { "Disabled" };

        let icon_widget = widget::container(icon::from_name(icon_name).size(32))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center);

        let text_column = widget::column::with_capacity(2)
            .push(widget::text(label).size(14))
            .push_maybe(show_status_text.then(|| widget::text(status_text).size(12)))
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
            widget::text(format!("Control {label} functionality")),
            widget::tooltip::Position::Bottom,
        )
        .into()
    }
}

fn main() -> cosmic::iced::Result {
    // Initialize systemd journal logger
    log::set_max_level(log::LevelFilter::Info);
    JournalLog::new().unwrap().install().unwrap();
    cosmic::applet::run::<KillSwitch>(())
}
