use super::*;
use std::{cell::RefCell, rc::Rc};

struct ObserveMoves(Rc<RefCell<Vec<(f32, f32)>>>);
impl App for ObserveMoves {
    fn build(&mut self, _: &mut rfgui::app::AppContext<'_>) -> rfgui::ui::RsxNode {
        rfgui::ui::RsxNode::fragment(Vec::new())
    }
    fn on_event(&mut self, event: &AppEvent, _: &mut rfgui::app::AppContext<'_>) {
        if let AppEvent::Pointer(PlatformPointerEvent {
            kind: PlatformPointerEventKind::Move { x, y },
            ..
        }) = event
        {
            self.0.borrow_mut().push((*x, *y));
        }
    }
}
fn runner() -> (Runner, Rc<RefCell<Vec<(f32, f32)>>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut runner = Runner::new(Box::new(ObserveMoves(log.clone())), AppConfig::default());
    let mut viewport = Viewport::new();
    viewport.set_app(runner.pending_app.take().unwrap());
    runner.viewport = Some(viewport);
    (runner, log)
}

#[test]
fn a_frame_dispatches_only_the_latest_move_to_app_and_viewport() {
    let (mut runner, log) = runner();
    for x in 0..100 {
        runner.queue_pointer_move(x as f32, 7., true);
    }
    assert!(log.borrow().is_empty());
    assert!(
        runner
            .viewport
            .as_mut()
            .unwrap()
            .drain_platform_requests()
            .request_redraw
    );
    runner.flush_pointer_move();
    runner.flush_pointer_move();
    assert_eq!(&*log.borrow(), &[(99., 7.)]);
    assert_eq!(
        runner
            .viewport
            .as_ref()
            .unwrap()
            .pointer_position_viewport(),
        Some((99., 7.))
    );
    runner.queue_pointer_move(101., 9., false);
    runner.flush_pointer_move();
    assert_eq!(
        log.borrow().len(),
        1,
        "raw device drag keeps its existing engine-only routing"
    );
    assert_eq!(
        runner
            .viewport
            .as_ref()
            .unwrap()
            .pointer_position_viewport(),
        Some((101., 9.))
    );
}

#[test]
fn input_and_redraw_boundaries_flush_the_latest_position_first() {
    let (mut runner, log) = runner();
    let device_id = DeviceId::dummy();
    let boundaries = [
        WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: WinitMouseButton::Left,
        },
        WindowEvent::MouseInput {
            device_id,
            state: ElementState::Released,
            button: WinitMouseButton::Left,
        },
        WindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::LineDelta(0., 1.),
            phase: winit::event::TouchPhase::Moved,
        },
        WindowEvent::ModifiersChanged(Default::default()),
        WindowEvent::Ime(Ime::Commit("a".into())),
        WindowEvent::RedrawRequested,
        WindowEvent::CursorLeft { device_id },
    ];
    for (i, event) in boundaries.iter().enumerate() {
        runner.queue_pointer_move(-1., 0., true);
        runner.queue_pointer_move(i as f32, 3., true);
        runner.flush_pointer_move_before(&WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(0., 0.),
        });
        assert_eq!(log.borrow().len(), i);
        runner.flush_pointer_move_before(event);
        assert_eq!(log.borrow().last().copied(), Some((i as f32, 3.)));
        assert_eq!(
            runner
                .viewport
                .as_ref()
                .unwrap()
                .pointer_position_viewport(),
            Some((i as f32, 3.))
        );
    }
}
