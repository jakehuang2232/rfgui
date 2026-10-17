use crate::rfgui::style::{
    Angle, Length, Perspective, Rotate, Scale, Transform, TransformOrigin, Translate,
};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

#[component]
pub fn TransformBasics() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Transforms"
            description="Translate, scale and rotate in 2D, or add perspective and rotate in 3D. The transform origin sets the point they turn around."
        >
            <SampleRow>
                <Sample label="translate + scale">
                    <Element style={{ width: Length::px(176.0), height: Length::px(120.0) }}>
                        <Element style={{
                            width: Length::px(136.0),
                            height: Length::px(96.0),
                            background: theme.color.primary.base.clone(),
                            color: theme.color.primary.on.clone(),
                            border_radius: theme.radius.md,
                            transform: Transform::new([
                                Translate::x(Length::px(10.0)).with_y(Length::px(8.0)),
                                Scale::xy(1.1, 0.9),
                            ]),
                            transform_origin: TransformOrigin::center(),
                        }}>
                            <Text>translate + scale</Text>
                        </Element>
                    </Element>
                </Sample>
                <Sample label="rotate z">
                    <Element style={{ width: Length::px(176.0), height: Length::px(120.0) }}>
                        <Element style={{
                            width: Length::px(132.0),
                            height: Length::px(92.0),
                            background: "#0f766e",
                            color: theme.color.background.base.clone(),
                            border_radius: theme.radius.md,
                            transform: Transform::new([Rotate::z(Angle::deg(-15.0))]),
                            transform_origin: TransformOrigin::center(),
                        }}>
                            <Text>rotate z</Text>
                        </Element>
                    </Element>
                </Sample>
                <Sample label="perspective + rotate x/y">
                    <Element style={{ width: Length::px(176.0), height: Length::px(120.0) }}>
                        <Element style={{
                            width: Length::px(132.0),
                            height: Length::px(92.0),
                            background: "#7c3aed",
                            color: theme.color.background.base.clone(),
                            border_radius: theme.radius.md,
                            transform: Transform::new([
                                Perspective::px(560.0),
                                Rotate::x(Angle::deg(26.0)).y(Angle::deg(-18.0)).z(Angle::deg(8.0)),
                            ]),
                            transform_origin: TransformOrigin::center().with_z(12.0),
                        }}>
                            <Text>3D card</Text>
                        </Element>
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
