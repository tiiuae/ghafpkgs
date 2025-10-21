use iced::{
    widget::{column, container, row, svg, text, toggler},
    window, Alignment, Background, Element, Length, Size, Task, Theme,
};
use std::path::PathBuf;

struct KillSwitch {
    microphone_enabled: bool,
    camera_enabled: bool,
    wifi_enabled: bool,
    icons_path: PathBuf,
}

#[derive(Debug)]
enum Message {
    ToggleMicrophone(bool),
    ToggleCamera(bool),
    ToggleWifi(bool),
}

impl Default for KillSwitch {
    fn default() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        Self {
            microphone_enabled: false,
            camera_enabled: false,
            wifi_enabled: false,
            // You can change this path to wherever your icons are
            icons_path: exe_dir.join("../icons"),
        }
    }
}

impl KillSwitch {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleMicrophone(enabled) => {
                self.microphone_enabled = enabled;
                // This is placeholder for microphone implementation
            }
            Message::ToggleCamera(enabled) => {
                self.camera_enabled = enabled;
            }
            Message::ToggleWifi(enabled) => {
                self.wifi_enabled = enabled;
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        // Header above controls
        let header = container(
            text("Enable / Disable controls")
                .size(20)
                .color(iced::Color::WHITE),
        )
        .width(Length::Fill)
        .padding(20)
        .align_x(iced::alignment::Horizontal::Center)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(iced::Color::from_rgb(0.05, 0.05, 0.05))),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        });

        // Microphone row
        let microphone_row = create_control_row(
            &self.icons_path.join("microphone.svg"),
            "Microphone",
            if self.microphone_enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            self.microphone_enabled,
            Message::ToggleMicrophone,
            iced::Color::from_rgb(0.85, 0.9, 1.0),
        );

        // Camera row
        let camera_row = create_control_row(
            &self.icons_path.join("camera.svg"),
            "Camera",
            if self.camera_enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            self.camera_enabled,
            Message::ToggleCamera,
            iced::Color::from_rgb(0.85, 1.0, 0.92),
        );

        // Wifi row
        let wifi_row = create_control_row(
            &self.icons_path.join("wifi.svg"),
            "WiFi",
            if self.wifi_enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            self.wifi_enabled,
            Message::ToggleWifi,
            iced::Color::from_rgb(0.95, 0.92, 1.0),
        );

        // Main content
        let content = column![header, microphone_row, camera_row, wifi_row]
            .spacing(0)
            .width(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(iced::Color::from_rgb(0.96, 0.96, 0.96))),
                ..Default::default()
            })
            .into()
    }
}

fn create_control_row<F>(
    icon_path: &PathBuf,
    title: &'static str,
    status: &'static str,
    enabled: bool,
    on_toggle: F,
    icon_bg: iced::Color,
) -> Element<'static, Message>
where
    F: 'static + Fn(bool) -> Message,
{
    // Try to load the SVG icon from file, fallback to text if not found
    let icon_element: Element<'static, Message> = if icon_path.exists() {
        let svg_handle = svg::Handle::from_path(icon_path);
        svg(svg_handle)
            .width(24)
            .height(24)
            .style(|_theme, _status| svg::Style {
                color: Some(iced::Color::from_rgb(0.2, 0.2, 0.2)),
            })
            .into()
    } else {
        text(title.chars().next().unwrap_or('?')).size(24).into()
    };

    // Icon container with colored background
    let icon_container = container(icon_element)
        .width(50)
        .height(50)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced::Background::Color(icon_bg)),
            border: iced::Border {
                radius: 25.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    // Text column
    let text_col = column![
        text(title).size(18),
        text(status)
            .size(13)
            .color(iced::Color::from_rgb(0.45, 0.45, 0.45))
    ]
    .spacing(2)
    .width(Length::Fill);

    // Toggle switch
    let toggle = toggler(enabled).on_toggle(on_toggle).width(Length::Shrink);

    // Row content
    let row_content = row![icon_container, text_col, toggle]
        .spacing(16)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    container(row_content)
        .padding(20)
        .width(Length::Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(iced::Color::WHITE)),
            border: iced::Border {
                color: iced::Color::from_rgb(0.92, 0.92, 0.92),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn main() -> iced::Result {
    iced::application("Kill Switch", KillSwitch::update, KillSwitch::view)
        .theme(|_| Theme::Light)
        .window(window::Settings {
            size: Size::new(300.0, 350.0),
            min_size: Some(Size::new(350.0, 350.0)), // Set min window size (width, height)
            max_size: Some(Size::new(600.0, 800.0)), // Set max size
            resizable: true,                         // Allow resizing
            decorations: true,
            ..Default::default()
        })
        .run()
}
