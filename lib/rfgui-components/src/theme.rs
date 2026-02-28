use rfgui::ui::{GlobalState, global_state};
use rfgui::{
    Border, BorderRadius, BoxShadow, Color, ColorLike, FontSize, Length, Padding, TransitionTiming,
};

#[derive(Clone)]
pub struct Theme {
    pub color: ColorTheme,
    pub typography: TypographyTheme,
    pub spacing: SpacingTheme,
    pub radius: RadiusTheme,
    pub shadow: ShadowTheme,
    pub motion: MotionTheme,
    pub component: ComponentTheme,
}

#[derive(Clone)]
pub struct ColorTheme {
    pub primary: ColorSet,
    pub secondary: ColorSet,
    pub background: ColorSet,
    pub surface: ColorSet,
    pub layer: SurfaceLayerTheme,
    pub text: TextColorSet,
    pub border: Box<dyn ColorLike>,
    pub divider: Box<dyn ColorLike>,
    pub state: StateColorSet,
}

#[derive(Clone)]
pub struct SurfaceLayerTheme {
    pub app: Box<dyn ColorLike>,
    pub surface: Box<dyn ColorLike>,
    pub raised: Box<dyn ColorLike>,
    pub inverse: Box<dyn ColorLike>,
    pub on_inverse: Box<dyn ColorLike>,
}

#[derive(Clone)]
pub struct ColorSet {
    pub base: Box<dyn ColorLike>,
    pub on: Box<dyn ColorLike>,
}

#[derive(Clone)]
pub struct TextColorSet {
    pub primary: Box<dyn ColorLike>,
    pub secondary: Box<dyn ColorLike>,
    pub disabled: Box<dyn ColorLike>,
}

#[derive(Clone)]
pub struct StateColorSet {
    pub hover: Box<dyn ColorLike>,
    pub active: Box<dyn ColorLike>,
    pub focus: Box<dyn ColorLike>,
    pub disabled: Box<dyn ColorLike>,
}

#[derive(Clone)]
pub struct TypographyTheme {
    pub font_family: String,
    pub size: FontSizeScale,
    pub weight: FontWeightScale,
    pub line_height: LineHeightScale,
}

#[derive(Clone)]
pub struct FontSizeScale {
    pub xs: FontSize,
    pub sm: FontSize,
    pub md: FontSize,
    pub lg: FontSize,
    pub xl: FontSize,
}

#[derive(Clone)]
pub struct FontWeightScale {
    pub regular: u16,
    pub medium: u16,
    pub bold: u16,
}

#[derive(Clone)]
pub struct LineHeightScale {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

#[derive(Clone)]
pub struct SpacingTheme {
    pub unit: Length,
    pub xs: Length,
    pub sm: Length,
    pub md: Length,
    pub lg: Length,
    pub xl: Length,
}

#[derive(Clone)]
pub struct RadiusTheme {
    pub sm: Length,
    pub md: Length,
    pub lg: Length,
}

#[derive(Clone)]
pub struct ShadowTheme {
    pub level_0: BoxShadow,
    pub level_1: BoxShadow,
    pub level_2: BoxShadow,
    pub level_3: BoxShadow,
}

#[derive(Clone)]
pub struct MotionTheme {
    pub duration: DurationScale,
    pub easing: EasingTheme,
}

#[derive(Clone)]
pub struct DurationScale {
    pub fast: u32,
    pub normal: u32,
    pub slow: u32,
}

#[derive(Clone)]
pub struct EasingTheme {
    pub standard: TransitionTiming,
    pub enter: TransitionTiming,
    pub exit: TransitionTiming,
}

#[derive(Clone)]
pub struct ComponentTheme {
    pub button: ButtonTheme,
    pub input: InputTheme,
    pub card: CardTheme,
    pub select: SelectTheme,
    pub slider: SliderTheme,
}

#[derive(Clone)]
pub struct ButtonTheme {
    pub padding: Padding,
    pub radius: BorderRadius,
    pub border: Border,
}

#[derive(Clone)]
pub struct InputTheme {
    pub padding: Padding,
    pub radius: BorderRadius,
    pub border: Border,
}

#[derive(Clone)]
pub struct CardTheme {
    pub padding: Padding,
    pub radius: BorderRadius,
    pub border: Border,
}

#[derive(Clone)]
pub struct SelectTheme {
    pub trigger_hover_background: Box<dyn ColorLike>,
    pub option_hover_background: Box<dyn ColorLike>,
    pub option_selected_background: Box<dyn ColorLike>,
    pub option_disabled_background: Box<dyn ColorLike>,
    pub option_selected_text: Box<dyn ColorLike>,
    pub option_disabled_text: Box<dyn ColorLike>,
}

#[derive(Clone)]
pub struct SliderTheme {
    pub width: f32,
    pub height: f32,
    pub frame_radius: BorderRadius,
    pub frame_background: Box<dyn ColorLike>,
    pub frame_hover_background: Box<dyn ColorLike>,
    pub frame_active_background: Box<dyn ColorLike>,
    pub frame_disabled_background: Box<dyn ColorLike>,
    pub grab_padding: f32,
    pub grab_width: f32,
    pub grab_radius: BorderRadius,
    pub grab_background: Box<dyn ColorLike>,
    pub grab_hover_background: Box<dyn ColorLike>,
    pub grab_active_background: Box<dyn ColorLike>,
    pub grab_disabled_background: Box<dyn ColorLike>,
}

impl Theme {
    pub fn light() -> Self {
        let border_color = Color::rgb(220, 223, 230);

        Self {
            color: ColorTheme {
                primary: ColorSet {
                    base: rgb(64, 120, 242),
                    on: rgb(255, 255, 255),
                },
                secondary: ColorSet {
                    base: rgb(166, 38, 164),
                    on: rgb(255, 255, 255),
                },
                background: ColorSet {
                    base: rgb(250, 250, 250),
                    on: rgb(56, 58, 66),
                },
                surface: ColorSet {
                    base: rgb(255, 255, 255),
                    on: rgb(56, 58, 66),
                },
                layer: SurfaceLayerTheme {
                    app: rgb(250, 250, 250),
                    surface: rgb(255, 255, 255),
                    raised: rgb(240, 240, 240),
                    inverse: rgb(56, 58, 66),
                    on_inverse: rgb(250, 250, 250),
                },
                text: TextColorSet {
                    primary: rgb(56, 58, 66),
                    secondary: rgb(105, 108, 119),
                    disabled: rgb(160, 161, 167),
                },
                border: Box::new(border_color),
                divider: rgb(229, 229, 230),
                state: StateColorSet {
                    hover: rgba(56, 58, 66, 16),
                    active: rgba(56, 58, 66, 28),
                    focus: rgb(64, 120, 242),
                    disabled: rgba(160, 161, 167, 128),
                },
            },
            typography: TypographyTheme {
                font_family: String::from("SF Pro Text, PingFang TC, sans-serif"),
                size: FontSizeScale {
                    xs: FontSize::px(12.0),
                    sm: FontSize::px(14.0),
                    md: FontSize::px(16.0),
                    lg: FontSize::px(20.0),
                    xl: FontSize::px(24.0),
                },
                weight: FontWeightScale {
                    regular: 400,
                    medium: 500,
                    bold: 700,
                },
                line_height: LineHeightScale {
                    xs: 1.25,
                    sm: 1.35,
                    md: 1.5,
                    lg: 1.4,
                    xl: 1.3,
                },
            },
            spacing: SpacingTheme {
                unit: Length::px(8.0),
                xs: Length::px(4.0),
                sm: Length::px(8.0),
                md: Length::px(12.0),
                lg: Length::px(16.0),
                xl: Length::px(24.0),
            },
            radius: RadiusTheme {
                sm: Length::px(4.0),
                md: Length::px(8.0),
                lg: Length::px(12.0),
            },
            shadow: ShadowTheme {
                level_0: BoxShadow::new().color(Color::transparent()),
                level_1: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 77))
                    .offset_y(1.0)
                    .blur(4.0),
                level_2: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 102))
                    .offset_y(4.0)
                    .blur(12.0),
                level_3: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 128))
                    .offset_y(10.0)
                    .blur(24.0),
            },
            motion: MotionTheme {
                duration: DurationScale {
                    fast: 120,
                    normal: 180,
                    slow: 280,
                },
                easing: EasingTheme {
                    standard: TransitionTiming::EaseInOut,
                    enter: TransitionTiming::EaseOut,
                    exit: TransitionTiming::EaseIn,
                },
            },
            component: ComponentTheme {
                button: ButtonTheme {
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    radius: BorderRadius::uniform(Length::px(4.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                input: InputTheme {
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    radius: BorderRadius::uniform(Length::px(4.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                card: CardTheme {
                    padding: Padding::uniform(Length::px(4.0)),
                    radius: BorderRadius::uniform(Length::px(12.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                select: SelectTheme {
                    trigger_hover_background: rgba(56, 58, 66, 16),
                    option_hover_background: rgba(56, 58, 66, 16),
                    option_selected_background: rgba(56, 58, 66, 28),
                    option_disabled_background: rgba(160, 161, 167, 128),
                    option_selected_text: rgb(64, 120, 242),
                    option_disabled_text: rgb(160, 161, 167),
                },
                slider: SliderTheme {
                    width: 240.0,
                    height: 18.0,
                    frame_radius: BorderRadius::uniform(Length::px(4.0)),
                    frame_background: rgb(229, 229, 230),
                    frame_hover_background: rgb(220, 223, 230),
                    frame_active_background: rgb(210, 214, 224),
                    frame_disabled_background: rgba(160, 161, 167, 96),
                    grab_padding: 2.0,
                    grab_width: 14.0,
                    grab_radius: BorderRadius::uniform(Length::px(3.0)),
                    grab_background: rgb(64, 120, 242),
                    grab_hover_background: rgb(77, 134, 247),
                    grab_active_background: rgb(48, 103, 227),
                    grab_disabled_background: rgba(160, 161, 167, 192),
                },
            },
        }
    }

    pub fn dark() -> Self {
        let border_color = Color::rgb(62, 68, 81);

        Self {
            color: ColorTheme {
                primary: ColorSet {
                    base: rgb(97, 175, 239),
                    on: rgb(40, 44, 52),
                },
                secondary: ColorSet {
                    base: rgb(198, 120, 221),
                    on: rgb(40, 44, 52),
                },
                background: ColorSet {
                    base: rgb(40, 44, 52),
                    on: rgb(171, 178, 191),
                },
                surface: ColorSet {
                    base: rgb(33, 37, 43),
                    on: rgb(171, 178, 191),
                },
                layer: SurfaceLayerTheme {
                    app: rgb(40, 44, 52),
                    surface: rgb(33, 37, 43),
                    raised: rgb(44, 49, 60),
                    inverse: rgb(171, 178, 191),
                    on_inverse: rgb(40, 44, 52),
                },
                text: TextColorSet {
                    primary: hex("#b1b9c9"),
                    secondary: rgb(127, 132, 142),
                    disabled: hex("#7c8189"),
                },
                border: Box::new(border_color),
                divider: rgb(44, 49, 60),
                state: StateColorSet {
                    hover: rgba(171, 178, 191, 26),
                    active: rgba(171, 178, 191, 44),
                    focus: rgb(97, 175, 239),
                    disabled: rgba(92, 99, 112, 128),
                },
            },
            typography: TypographyTheme {
                font_family: String::from("SF Pro Text, PingFang TC, sans-serif"),
                size: FontSizeScale {
                    xs: FontSize::px(12.0),
                    sm: FontSize::px(14.0),
                    md: FontSize::px(16.0),
                    lg: FontSize::px(20.0),
                    xl: FontSize::px(24.0),
                },
                weight: FontWeightScale {
                    regular: 400,
                    medium: 500,
                    bold: 700,
                },
                line_height: LineHeightScale {
                    xs: 1.25,
                    sm: 1.35,
                    md: 1.5,
                    lg: 1.4,
                    xl: 1.3,
                },
            },
            spacing: SpacingTheme {
                unit: Length::px(8.0),
                xs: Length::px(4.0),
                sm: Length::px(8.0),
                md: Length::px(12.0),
                lg: Length::px(16.0),
                xl: Length::px(24.0),
            },
            radius: RadiusTheme {
                sm: Length::px(4.0),
                md: Length::px(8.0),
                lg: Length::px(12.0),
            },
            shadow: ShadowTheme {
                level_0: BoxShadow::new().color(Color::transparent()),
                level_1: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 77))
                    .offset_y(1.0)
                    .blur(4.0),
                level_2: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 102))
                    .offset_y(4.0)
                    .blur(12.0),
                level_3: BoxShadow::new()
                    .color(Color::rgba(0, 0, 0, 128))
                    .offset_y(10.0)
                    .blur(24.0),
            },
            motion: MotionTheme {
                duration: DurationScale {
                    fast: 120,
                    normal: 180,
                    slow: 280,
                },
                easing: EasingTheme {
                    standard: TransitionTiming::EaseInOut,
                    enter: TransitionTiming::EaseOut,
                    exit: TransitionTiming::EaseIn,
                },
            },
            component: ComponentTheme {
                button: ButtonTheme {
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    radius: BorderRadius::uniform(Length::px(4.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                input: InputTheme {
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    radius: BorderRadius::uniform(Length::px(4.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                card: CardTheme {
                    padding: Padding::uniform(Length::px(4.0)),
                    radius: BorderRadius::uniform(Length::px(12.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                },
                select: SelectTheme {
                    trigger_hover_background: rgba(171, 178, 191, 26),
                    option_hover_background: rgba(171, 178, 191, 26),
                    option_selected_background: rgba(171, 178, 191, 44),
                    option_disabled_background: rgba(92, 99, 112, 128),
                    option_selected_text: rgb(97, 175, 239),
                    option_disabled_text: rgb(92, 99, 112),
                },
                slider: SliderTheme {
                    width: 240.0,
                    height: 18.0,
                    frame_radius: BorderRadius::uniform(Length::px(4.0)),
                    frame_background: rgb(44, 49, 60),
                    frame_hover_background: rgb(56, 62, 74),
                    frame_active_background: rgb(64, 70, 84),
                    frame_disabled_background: rgba(92, 99, 112, 96),
                    grab_padding: 2.0,
                    grab_width: 14.0,
                    grab_radius: BorderRadius::uniform(Length::px(3.0)),
                    grab_background: rgb(77, 139, 189),
                    grab_hover_background: rgb(78, 131, 174),
                    grab_active_background: rgb(55, 121, 178),
                    grab_disabled_background: rgba(92, 99, 112, 192),
                },
            },
        }
    }
}

pub fn init_theme(theme: Theme) {
    let state = global_state(Theme::light);
    state.set(theme);
}

pub fn use_theme() -> GlobalState<Theme> {
    global_state(Theme::light)
}

pub fn set_theme(theme: Theme) {
    init_theme(theme);
}

fn hex(hex_string: &str) -> Box<dyn ColorLike> {
    let [r, g, b, a] = Color::hex(hex_string).to_rgba_u8();
    Box::new(Color::rgba(r, g, b, a))
}

fn rgb(r: u8, g: u8, b: u8) -> Box<dyn ColorLike> {
    Box::new(Color::rgb(r, g, b))
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Box<dyn ColorLike> {
    Box::new(Color::rgba(r, g, b, a))
}
