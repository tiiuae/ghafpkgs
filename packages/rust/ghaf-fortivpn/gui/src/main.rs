/*
 * SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

use cosmic::app::{Core, Settings};
use cosmic::iced::{Alignment, Length, Size};
use cosmic::{Application, Element, executor, widget};
use std::fs;
use std::path::{Path, PathBuf};
use zbus::blocking::{Connection, Proxy};
use zeroize::Zeroizing;

const APP_ID: &str = "org.ghaf.FortiVpn";
const BUS_NAME: &str = "org.ghaf.FortiVpn";
const OBJECT_PATH: &str = "/org/ghaf/FortiVpn";
const INTERFACE_NAME: &str = "org.ghaf.FortiVpn1";
const MAX_BUNDLE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CertificateMode {
    None,
    Pkcs12,
    Separate,
}

#[derive(Clone, Copy, Debug)]
enum FileKind {
    Pkcs12,
    ClientCertificate,
    PrivateKey,
    CaCertificate,
}

#[derive(Clone, Debug)]
enum Message {
    Name(String),
    Gateway(String),
    Port(String),
    Realm(String),
    Username(String),
    VpnPassword(String),
    TrustedCertificate(String),
    ToggleVpnPassword,
    ModeSelected(CertificateMode),
    ToggleAdvanced,
    ToggleCertificate,
    PickFile(FileKind),
    FileSelected(FileKind, Option<PathBuf>),
    Pkcs12Password(String),
    KeyPassword(String),
    TogglePkcs12Password,
    ToggleKeyPassword,
    Create,
    CreateFinished(Result<String, String>),
    DismissStatus,
}

struct FortiVpnApp {
    core: Core,
    name: String,
    gateway: String,
    port: String,
    realm: String,
    username: String,
    vpn_password: Zeroizing<String>,
    trusted_certificate: String,
    certificate_mode: CertificateMode,
    pkcs12: Option<PathBuf>,
    client_certificate: Option<PathBuf>,
    private_key: Option<PathBuf>,
    ca_certificate: Option<PathBuf>,
    pkcs12_password: Zeroizing<String>,
    key_password: Zeroizing<String>,
    hide_vpn_password: bool,
    hide_pkcs12_password: bool,
    hide_key_password: bool,
    show_advanced: bool,
    show_certificate: bool,
    busy: bool,
    status: Option<Result<String, String>>,
}

impl FortiVpnApp {
    fn reset_form(&mut self) {
        self.name = "Fortinet VPN".into();
        self.gateway.clear();
        self.port = "443".into();
        self.realm.clear();
        self.username.clear();
        self.vpn_password.clear();
        self.trusted_certificate.clear();
        self.certificate_mode = CertificateMode::None;
        self.pkcs12 = None;
        self.client_certificate = None;
        self.private_key = None;
        self.ca_certificate = None;
        self.pkcs12_password.clear();
        self.key_password.clear();
        self.hide_vpn_password = true;
        self.hide_pkcs12_password = true;
        self.hide_key_password = true;
        self.show_advanced = false;
        self.show_certificate = false;
    }
}

impl Application for FortiVpnApp {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

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
        (
            Self {
                core,
                name: "Fortinet VPN".into(),
                gateway: String::new(),
                port: "443".into(),
                realm: String::new(),
                username: String::new(),
                vpn_password: Zeroizing::new(String::new()),
                trusted_certificate: String::new(),
                certificate_mode: CertificateMode::None,
                pkcs12: None,
                client_certificate: None,
                private_key: None,
                ca_certificate: None,
                pkcs12_password: Zeroizing::new(String::new()),
                key_password: Zeroizing::new(String::new()),
                hide_vpn_password: true,
                hide_pkcs12_password: true,
                hide_key_password: true,
                show_advanced: false,
                show_certificate: false,
                busy: false,
                status: None,
            },
            cosmic::Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> cosmic::Task<cosmic::Action<Self::Message>> {
        match message {
            Message::Name(value) => self.name = value,
            Message::Gateway(value) => self.gateway = value,
            Message::Port(value) => self.port = value,
            Message::Realm(value) => self.realm = value,
            Message::Username(value) => self.username = value,
            Message::VpnPassword(value) => self.vpn_password = Zeroizing::new(value),
            Message::TrustedCertificate(value) => self.trusted_certificate = value,
            Message::ToggleVpnPassword => self.hide_vpn_password = !self.hide_vpn_password,
            Message::ModeSelected(mode) => {
                self.certificate_mode = mode;
                self.status = None;
            }
            Message::ToggleAdvanced => {
                self.show_advanced = !self.show_advanced;
                self.status = None;
            }
            Message::ToggleCertificate => {
                self.show_certificate = !self.show_certificate;
                if self.show_certificate {
                    if self.certificate_mode == CertificateMode::None {
                        self.certificate_mode = CertificateMode::Pkcs12;
                    }
                } else {
                    self.certificate_mode = CertificateMode::None;
                    self.pkcs12 = None;
                    self.client_certificate = None;
                    self.private_key = None;
                    self.pkcs12_password.clear();
                    self.key_password.clear();
                }
                self.status = None;
            }
            Message::PickFile(kind) => return pick_file(kind),
            Message::FileSelected(kind, path) => match kind {
                FileKind::Pkcs12 => self.pkcs12 = path,
                FileKind::ClientCertificate => self.client_certificate = path,
                FileKind::PrivateKey => self.private_key = path,
                FileKind::CaCertificate => self.ca_certificate = path,
            },
            Message::Pkcs12Password(value) => {
                self.pkcs12_password = Zeroizing::new(value);
            }
            Message::KeyPassword(value) => self.key_password = Zeroizing::new(value),
            Message::TogglePkcs12Password => {
                self.hide_pkcs12_password = !self.hide_pkcs12_password;
            }
            Message::ToggleKeyPassword => self.hide_key_password = !self.hide_key_password,
            Message::Create => {
                let port = match self.port.trim().parse::<u16>() {
                    Ok(0) | Err(_) => {
                        self.status = Some(Err("Enter a valid port from 1 to 65535".into()));
                        return cosmic::Task::none();
                    }
                    Ok(port) => port,
                };
                let request = CreateRequest {
                    name: self.name.clone(),
                    gateway: self.gateway.clone(),
                    port,
                    realm: self.realm.clone(),
                    username: self.username.clone(),
                    vpn_password: Zeroizing::new(self.vpn_password.to_string()),
                    trusted_certificate: self.trusted_certificate.clone(),
                    mode: self.certificate_mode,
                    pkcs12: self.pkcs12.clone(),
                    client_certificate: self.client_certificate.clone(),
                    private_key: self.private_key.clone(),
                    ca_certificate: self.ca_certificate.clone(),
                    pkcs12_password: Zeroizing::new(self.pkcs12_password.to_string()),
                    key_password: Zeroizing::new(self.key_password.to_string()),
                };
                self.busy = true;
                self.status = None;
                return cosmic::Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || create_profile(request))
                            .await
                            .map_err(|_| "VPN profile creation stopped unexpectedly".to_string())?
                    },
                    |result| Message::CreateFinished(result).into(),
                );
            }
            Message::CreateFinished(result) => {
                self.busy = false;
                match result {
                    Ok(_) => {
                        let profile_name = self.name.trim().to_owned();
                        self.reset_form();
                        self.status = Some(Ok(format!(
                            "“{profile_name}” was added. Connect from Network & Wireless → VPN."
                        )));
                    }
                    Err(error) => {
                        self.vpn_password.clear();
                        self.pkcs12_password.clear();
                        self.key_password.clear();
                        self.status = Some(Err(error));
                    }
                }
            }
            Message::DismissStatus => self.status = None,
        }
        cosmic::Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = self.core.system_theme().cosmic().spacing;
        let connection = widget::column::with_capacity(3)
            .push(labeled_input(
                "Connection name",
                "Example: Work VPN",
                &self.name,
                Message::Name,
            ))
            .push(labeled_input(
                "Gateway",
                "Example: vpn.example.com",
                &self.gateway,
                Message::Gateway,
            ))
            .push(labeled_input(
                "Port",
                "Example: 443",
                &self.port,
                Message::Port,
            ))
            .spacing(spacing.space_s);

        let account = widget::column::with_capacity(2)
            .push(labeled_input(
                "Username",
                "Example: firstname.lastname",
                &self.username,
                Message::Username,
            ))
            .push(
                widget::column::with_capacity(2)
                    .push(widget::text::body("VPN password"))
                    .push(
                        widget::text_input::secure_input(
                            "Enter password",
                            self.vpn_password.as_str(),
                            Some(Message::ToggleVpnPassword),
                            self.hide_vpn_password,
                        )
                        .on_input(Message::VpnPassword)
                        .style(dim_placeholder_style()),
                    )
                    .spacing(4),
            )
            .spacing(spacing.space_s);

        let mut content = widget::column::with_capacity(12)
            .push(widget::text::title3("Add a Fortinet VPN"))
            .push(widget::text::body(
                "Enter the connection details. The VPN will appear in Network & Wireless when it is ready.",
            ))
            .push(form_card("Connection", connection.into()))
            .push(form_card("Account", account.into()))
            .push(
                widget::button::standard(if self.show_advanced {
                    "Hide advanced options"
                } else {
                    "Advanced options"
                })
                .on_press(Message::ToggleAdvanced)
                .width(Length::Fill),
            );

        if self.show_advanced {
            let advanced = widget::column::with_capacity(3)
                .push(labeled_input(
                    "Realm",
                    "Example: company",
                    &self.realm,
                    Message::Realm,
                ))
                .push(labeled_input(
                    "Trusted gateway certificate",
                    "SHA-256 fingerprint",
                    &self.trusted_certificate,
                    Message::TrustedCertificate,
                ))
                .push(file_row(
                    "Gateway CA certificate",
                    self.ca_certificate.as_deref(),
                    Message::PickFile(FileKind::CaCertificate),
                ))
                .spacing(spacing.space_s);
            content = content.push(form_card("Advanced options", advanced.into()));
        }

        content = content.push(
            widget::button::standard(if self.show_certificate {
                "Remove client certificate"
            } else {
                "Add client certificate"
            })
            .on_press(Message::ToggleCertificate)
            .width(Length::Fill),
        );

        if self.show_certificate {
            let selector = widget::column::with_capacity(2)
                .push(widget::radio(
                    widget::text::body("PKCS#12 / PFX bundle"),
                    CertificateMode::Pkcs12,
                    Some(self.certificate_mode),
                    Message::ModeSelected,
                ))
                .push(widget::radio(
                    widget::text::body("Certificate and private key"),
                    CertificateMode::Separate,
                    Some(self.certificate_mode),
                    Message::ModeSelected,
                ))
                .spacing(spacing.space_s);

            let mut certificate = widget::column::with_capacity(6)
                .push(widget::text::body("Certificate format"))
                .push(selector)
                .spacing(spacing.space_s);

            match self.certificate_mode {
                CertificateMode::None => {}
                CertificateMode::Pkcs12 => {
                    certificate = certificate
                        .push(file_row(
                            "PKCS#12 or PFX bundle",
                            self.pkcs12.as_deref(),
                            Message::PickFile(FileKind::Pkcs12),
                        ))
                        .push(
                            widget::column::with_capacity(2)
                                .push(widget::text::body("Bundle password"))
                                .push(
                                    widget::text_input::secure_input(
                                        "Bundle password",
                                        self.pkcs12_password.as_str(),
                                        Some(Message::TogglePkcs12Password),
                                        self.hide_pkcs12_password,
                                    )
                                    .on_input(Message::Pkcs12Password)
                                    .style(dim_placeholder_style()),
                                )
                                .spacing(4),
                        );
                }
                CertificateMode::Separate => {
                    certificate = certificate
                        .push(file_row(
                            "Client certificate (PEM or DER)",
                            self.client_certificate.as_deref(),
                            Message::PickFile(FileKind::ClientCertificate),
                        ))
                        .push(file_row(
                            "Private key (PEM, DER, or PKCS#8)",
                            self.private_key.as_deref(),
                            Message::PickFile(FileKind::PrivateKey),
                        ))
                        .push(
                            widget::column::with_capacity(2)
                                .push(widget::text::body("Private-key password (optional)"))
                                .push(
                                    widget::text_input::secure_input(
                                        "Private-key password",
                                        self.key_password.as_str(),
                                        Some(Message::ToggleKeyPassword),
                                        self.hide_key_password,
                                    )
                                    .on_input(Message::KeyPassword)
                                    .style(dim_placeholder_style()),
                                )
                                .spacing(4),
                        );
                }
            }
            content = content.push(form_card("Client certificate", certificate.into()));
        }

        if let Some(status) = &self.status {
            content = content.push(status_panel(status));
        }

        content = content.push(
            widget::button::suggested(if self.busy {
                "Adding VPN…"
            } else {
                "Add VPN"
            })
            .on_press_maybe((!self.busy).then_some(Message::Create))
            .width(Length::Fill),
        );

        widget::container(widget::scrollable(
            content
                .spacing(spacing.space_s)
                .padding(spacing.space_l)
                .width(Length::Fill),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .max_width(760.0)
        .into()
    }
}

fn form_card<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    widget::container(
        widget::column::with_capacity(2)
            .push(widget::text::heading(title))
            .push(body)
            .spacing(12),
    )
    .class(cosmic::theme::Container::Card)
    .padding(16)
    .width(Length::Fill)
    .into()
}

fn status_panel<'a>(status: &'a Result<String, String>) -> Element<'a, Message> {
    let (title, message, icon_name, success) = match status {
        Ok(message) => (
            "VPN profile added",
            message.as_str(),
            "emblem-ok-symbolic",
            true,
        ),
        Err(message) => (
            "Could not add VPN profile",
            message.as_str(),
            "dialog-error-symbolic",
            false,
        ),
    };

    let message = widget::row::with_capacity(3)
        .push(widget::icon::from_name(icon_name).size(24))
        .push(
            widget::column::with_capacity(2)
                .push(widget::text::heading(title))
                .push(widget::text::body(message))
                .spacing(4)
                .width(Length::Fill),
        )
        .push(
            widget::button::icon(widget::icon::from_name("window-close-symbolic").size(16))
                .on_press(Message::DismissStatus),
        )
        .spacing(12)
        .align_y(Alignment::Center);

    widget::container(message)
        .class(cosmic::theme::Container::custom(move |theme| {
            let cosmic = theme.cosmic();
            let colors = if success {
                &cosmic.success
            } else {
                &cosmic.destructive
            };
            widget::container::Style {
                icon_color: Some(colors.on.into()),
                text_color: Some(colors.on.into()),
                background: Some(cosmic::iced::Background::Color(colors.base.into())),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .padding(12)
        .width(Length::Fill)
        .into()
}

fn labeled_input<'a>(
    label: &'a str,
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .push(widget::text::body(label))
        .push(
            widget::text_input(placeholder, value)
                .on_input(on_input)
                .style(dim_placeholder_style()),
        )
        .spacing(4)
        .into()
}

fn dim_placeholder(
    mut appearance: widget::text_input::Appearance,
) -> widget::text_input::Appearance {
    appearance.placeholder_color.a *= 0.45;
    appearance
}

fn dim_input_active(theme: &cosmic::Theme) -> widget::text_input::Appearance {
    dim_placeholder(<cosmic::Theme as widget::text_input::StyleSheet>::active(
        theme,
        &cosmic::theme::TextInput::Default,
    ))
}

fn dim_input_error(theme: &cosmic::Theme) -> widget::text_input::Appearance {
    dim_placeholder(<cosmic::Theme as widget::text_input::StyleSheet>::error(
        theme,
        &cosmic::theme::TextInput::Default,
    ))
}

fn dim_input_hovered(theme: &cosmic::Theme) -> widget::text_input::Appearance {
    dim_placeholder(<cosmic::Theme as widget::text_input::StyleSheet>::hovered(
        theme,
        &cosmic::theme::TextInput::Default,
    ))
}

fn dim_input_focused(theme: &cosmic::Theme) -> widget::text_input::Appearance {
    dim_placeholder(<cosmic::Theme as widget::text_input::StyleSheet>::focused(
        theme,
        &cosmic::theme::TextInput::Default,
    ))
}

fn dim_input_disabled(theme: &cosmic::Theme) -> widget::text_input::Appearance {
    dim_placeholder(<cosmic::Theme as widget::text_input::StyleSheet>::disabled(
        theme,
        &cosmic::theme::TextInput::Default,
    ))
}

fn dim_placeholder_style() -> cosmic::theme::TextInput {
    cosmic::theme::TextInput::Custom {
        active: Box::new(dim_input_active),
        error: Box::new(dim_input_error),
        hovered: Box::new(dim_input_hovered),
        focused: Box::new(dim_input_focused),
        disabled: Box::new(dim_input_disabled),
    }
}

fn file_row<'a>(label: &'a str, path: Option<&Path>, message: Message) -> Element<'a, Message> {
    let selected = path
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "No file selected".into());
    widget::column::with_capacity(2)
        .push(widget::text::body(label))
        .push(
            widget::row::with_capacity(2)
                .push(widget::text::caption(selected).width(Length::Fill))
                .push(widget::button::standard("Choose file…").on_press(message))
                .spacing(12)
                .align_y(Alignment::Center),
        )
        .spacing(4)
        .into()
}

fn pick_file(kind: FileKind) -> cosmic::Task<cosmic::Action<Message>> {
    cosmic::Task::perform(
        async move {
            let dialog = match kind {
                FileKind::Pkcs12 => {
                    rfd::AsyncFileDialog::new().add_filter("PKCS#12 bundle", &["p12", "pfx"])
                }
                FileKind::ClientCertificate | FileKind::CaCertificate => {
                    rfd::AsyncFileDialog::new()
                        .add_filter("X.509 certificate", &["pem", "crt", "cer", "der"])
                }
                FileKind::PrivateKey => rfd::AsyncFileDialog::new()
                    .add_filter("Private key", &["pem", "key", "der", "p8", "pk8"]),
            };
            (
                kind,
                dialog
                    .pick_file()
                    .await
                    .map(|file| file.path().to_path_buf()),
            )
        },
        |(kind, path)| Message::FileSelected(kind, path).into(),
    )
}

struct CreateRequest {
    name: String,
    gateway: String,
    port: u16,
    realm: String,
    username: String,
    vpn_password: Zeroizing<String>,
    trusted_certificate: String,
    mode: CertificateMode,
    pkcs12: Option<PathBuf>,
    client_certificate: Option<PathBuf>,
    private_key: Option<PathBuf>,
    ca_certificate: Option<PathBuf>,
    pkcs12_password: Zeroizing<String>,
    key_password: Zeroizing<String>,
}

fn read_limited(path: Option<&Path>, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let metadata = fs::metadata(path).map_err(|_| format!("{label} cannot be read"))?;
    if !metadata.is_file() {
        return Err(format!("{label} is not a regular file"));
    }
    if metadata.len() > limit {
        return Err(format!("{label} is too large"));
    }
    fs::read(path).map_err(|_| format!("{label} cannot be read"))
}

fn create_profile(request: CreateRequest) -> Result<String, String> {
    let pkcs12 = match request.mode {
        CertificateMode::Pkcs12 => Zeroizing::new(read_limited(
            request.pkcs12.as_deref(),
            MAX_BUNDLE_SIZE,
            "The PKCS#12 bundle",
        )?),
        _ => Zeroizing::new(Vec::new()),
    };
    let client_certificate = match request.mode {
        CertificateMode::Separate => read_limited(
            request.client_certificate.as_deref(),
            MAX_FILE_SIZE,
            "The client certificate",
        )?,
        _ => Vec::new(),
    };
    let private_key = match request.mode {
        CertificateMode::Separate => Zeroizing::new(read_limited(
            request.private_key.as_deref(),
            MAX_FILE_SIZE,
            "The private key",
        )?),
        _ => Zeroizing::new(Vec::new()),
    };
    let ca_certificate = read_limited(
        request.ca_certificate.as_deref(),
        MAX_FILE_SIZE,
        "The CA certificate",
    )?;

    if request.mode == CertificateMode::Pkcs12 && pkcs12.is_empty() {
        return Err("Select a PKCS#12 or PFX bundle".into());
    }
    if request.mode == CertificateMode::Separate
        && (client_certificate.is_empty() || private_key.is_empty())
    {
        return Err("Select both a client certificate and its private key".into());
    }

    let connection = Connection::system().map_err(|_| "The Fortinet service is unavailable")?;
    let proxy = Proxy::new(&connection, BUS_NAME, OBJECT_PATH, INTERFACE_NAME)
        .map_err(|_| "The Fortinet service is unavailable")?;
    proxy
        .call::<_, _, String>(
            "CreateProfile",
            &(
                request.name,
                request.gateway,
                request.port,
                request.realm,
                request.username,
                request.vpn_password.as_str(),
                request.trusted_certificate,
                pkcs12.as_slice(),
                request.pkcs12_password.as_str(),
                client_certificate,
                private_key.as_slice(),
                request.key_password.as_str(),
                ca_certificate,
            ),
        )
        .map_err(|error| dbus_error(&error))
}

fn dbus_error(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(_, Some(message), _) => message.clone(),
        _ => "The Fortinet service request failed".into(),
    }
}

fn main() -> cosmic::iced::Result {
    cosmic::app::run::<FortiVpnApp>(Settings::default().size(Size::new(760.0, 760.0)), ())
}
