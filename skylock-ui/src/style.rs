use iced::{
    widget::{button, container, text},
    Background, Color, theme,
};

pub enum Text {
    Default,
    Info,
    Warning,
    Error,
    Critical,
    Subtle,
}

impl text::StyleSheet for Text {
    type Style = theme::Theme;

    fn appearance(&self, theme: &Self::Style) -> text::Appearance {
        match self {
            Text::Default => text::Appearance {
                color: Some(Color::from_rgb(0.1, 0.1, 0.1)),
            },
            Text::Info => text::Appearance {
                color: Some(Color::from_rgb(0.0, 0.5, 0.8)),
            },
            Text::Warning => text::Appearance {
                color: Some(Color::from_rgb(0.8, 0.6, 0.0)),
            },
            Text::Error => text::Appearance {
                color: Some(Color::from_rgb(0.8, 0.0, 0.0)),
            },
            Text::Critical => text::Appearance {
                color: Some(Color::from_rgb(0.9, 0.0, 0.0)),
            },
            Text::Subtle => text::Appearance {
                color: Some(Color::from_rgb(0.5, 0.5, 0.5)),
            },
        }
    }
}

pub enum Button {
    Primary,
    Secondary,
    Destructive,
    Transparent,
}

impl button::StyleSheet for Button {
    type Style = theme::Theme;

    fn active(&self, _style: &Self::Style) -> button::Appearance {
        match self {
            Button::Primary => button::Appearance {
                background: Some(Background::Color(Color::from_rgb(0.2, 0.5, 0.8))),
                border_radius: 4.0,
                text_color: Color::WHITE,
                ..Default::default()
            },
            Button::Secondary => button::Appearance {
                background: Some(Background::Color(Color::from_rgb(0.8, 0.8, 0.8))),
                border_radius: 4.0,
                text_color: Color::BLACK,
                ..Default::default()
            },
            Button::Destructive => button::Appearance {
                background: Some(Background::Color(Color::from_rgb(0.8, 0.2, 0.2))),
                border_radius: 4.0,
                text_color: Color::WHITE,
                ..Default::default()
            },
            Button::Transparent => button::Appearance {
                background: None,
                text_color: Color::from_rgb(0.5, 0.5, 0.5),
                ..Default::default()
            },
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let active = self.active(style);
        match self {
            Button::Transparent => button::Appearance {
                text_color: Color::from_rgb(0.7, 0.7, 0.7),
                ..active
            },
            _ => button::Appearance {
                background: active.background.map(|bg| match bg {
                    Background::Color(color) => Background::Color(Color {
                        a: color.a * 0.8,
                        ..color
                    }),
                }),
                ..active
            },
        }
    }
}

pub enum Container {
    Default,
    Card,
}

impl container::StyleSheet for Container {
    type Style = theme::Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        match self {
            Container::Default => container::Appearance {
                background: Some(Background::Color(Color::from_rgb(0.95, 0.95, 0.95))),
                border_radius: 0.0,
                ..Default::default()
            },
            Container::Card => container::Appearance {
                background: Some(Background::Color(Color::WHITE)),
                border_radius: 4.0,
                border_width: 1.0,
                border_color: Color::from_rgb(0.9, 0.9, 0.9),
            },
        }
    }
}
