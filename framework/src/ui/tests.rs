use crate::platform::MouseButton;

use super::text::*;
use super::widgets::ZStack;
use super::*;

#[derive(Default, Debug, PartialEq, Eq)]
struct Data {
    clicks: u64,
    hot: bool,
}

fn simple_hstack() -> impl Widget<(Data, Data, Data)> + 'static {
    let items = vec![
        Rgba8::RED
            .sized([32., 32.])
            .on_hover(|hot, _, data: &mut (Data, Data, Data)| data.0.hot = hot)
            .boxed(),
        Rgba8::GREEN
            .sized([32., 32.])
            .on_hover(|hot, _, data: &mut (Data, Data, Data)| data.1.hot = hot)
            .boxed(),
        Rgba8::BLUE
            .sized([32., 32.])
            .on_hover(|hot, _, data: &mut (Data, Data, Data)| data.2.hot = hot)
            .boxed(),
    ];
    center(hstack(items).spacing(8.))
}

fn simple_zstack() -> ZStack<(Data, Data)> {
    zstack((
        center(
            Rgba8::BLUE
                .sized([256., 256.])
                .on_click(|_, data: &mut (Data, Data)| {
                    data.1.clicks += 1;
                })
                .on_hover(|hot, _, data| {
                    data.1.hot = hot;
                }),
        ),
        center(
            Rgba8::RED
                .sized([128., 128.])
                .on_click(|_, data: &mut (Data, Data)| {
                    data.0.clicks += 1;
                })
                .on_hover(|hot, _, data| {
                    data.0.hot = hot;
                }),
        ),
    ))
}

fn setup<'a, T, W: Widget<T> + 'static>(
    widget: fn() -> W,
    store: &'a HashMap<TextureId, Image>,
    fonts: &'a HashMap<FontId, Font>,
) -> (W, LayoutCtx<'a>, Context<'a>, Env) {
    let ctx = Context::new(Point::default(), store);
    let env = Env::default();
    let layout_ctx = LayoutCtx::new(fonts);
    let event_ctx = ctx.into();

    (widget(), layout_ctx, event_ctx, env)
}

fn hover<T, W: Widget<T> + 'static, P: Into<Point>>(
    point: P,
    ui: &mut W,
    event_ctx: &mut Context<'_>,
    data: &mut T,
) {
    ui.event(&WidgetEvent::MouseMove(point.into()), event_ctx, data);
}

fn click<T, W: Widget<T> + 'static>(ui: &mut W, event_ctx: &mut Context<'_>, data: &mut T) {
    ui.event(&WidgetEvent::MouseDown(MouseButton::Left), event_ctx, data);
    ui.event(&WidgetEvent::MouseUp(MouseButton::Left), event_ctx, data);
}

#[test]
fn test_simple_zstack_hover() {
    let (store, fonts) = (HashMap::new(), HashMap::new());
    let (mut ui, layout_ctx, mut event_ctx, env) = setup(simple_zstack, &store, &fonts);
    let mut data: (Data, Data) = Default::default();

    ui.layout(Size::new(512., 512.), &layout_ctx, &data, &env);

    hover([64., 64.], &mut ui, &mut event_ctx, &mut data);
    assert!(!data.1.hot);
    assert!(!data.0.hot);

    hover([160., 160.], &mut ui, &mut event_ctx, &mut data);
    assert!(data.1.hot);
    assert!(!data.0.hot);

    hover([256., 256.], &mut ui, &mut event_ctx, &mut data);
    assert!(!data.1.hot);
    assert!(data.0.hot);
}

#[test]
fn test_simple_zstack_click() {
    let (store, fonts) = (HashMap::new(), HashMap::new());
    let (mut ui, layout_ctx, mut event_ctx, env) = setup(simple_zstack, &store, &fonts);
    let mut data = Default::default();

    ui.layout(Size::new(512., 512.), &layout_ctx, &data, &env);

    hover([64., 64.], &mut ui, &mut event_ctx, &mut data);
    click(&mut ui, &mut event_ctx, &mut data);
    assert_eq!(data.1.clicks, 0);
    assert_eq!(data.0.clicks, 0);

    hover([160., 160.], &mut ui, &mut event_ctx, &mut data);
    click(&mut ui, &mut event_ctx, &mut data);
    assert_eq!(data.1.clicks, 1);
    assert_eq!(data.0.clicks, 0);

    hover([256., 256.], &mut ui, &mut event_ctx, &mut data);
    click(&mut ui, &mut event_ctx, &mut data);
    assert_eq!(data.1.clicks, 1);
    assert_eq!(data.0.clicks, 1);
}

#[test]
fn test_simple_hstack_hover() {
    let (store, fonts) = (HashMap::new(), HashMap::new());
    let (mut ui, layout_ctx, mut event_ctx, env) = setup(simple_hstack, &store, &fonts);
    let mut data = Default::default();

    ui.layout(Size::new(512., 512.), &layout_ctx, &data, &env);

    hover([0., 0.], &mut ui, &mut event_ctx, &mut data);
    assert!(!data.0.hot);
    assert!(!data.1.hot);
    assert!(!data.2.hot);

    hover([216., 256.], &mut ui, &mut event_ctx, &mut data);
    assert!(data.0.hot);
    assert!(!data.1.hot);
    assert!(!data.2.hot);

    hover([256., 256.], &mut ui, &mut event_ctx, &mut data);
    assert!(!data.0.hot);
    assert!(data.1.hot);
    assert!(!data.2.hot);

    hover([296., 256.], &mut ui, &mut event_ctx, &mut data);
    assert!(!data.0.hot);
    assert!(!data.1.hot);
    assert!(data.2.hot);
}
