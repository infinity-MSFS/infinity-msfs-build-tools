use infinity_rs::prelude::*;

pub struct MyGauge;

impl Gauge for MyGauge {
    fn init(&mut self, _ctx: &Context, _install: &mut GaugeInstall) -> bool {
        true
    }
    fn update(&mut self, _ctx: &Context, _dt: f32) -> bool {
        true
    }
    fn draw(&mut self, _ctx: &Context, _draw: &mut GaugeDraw) -> bool {
        true
    }
    fn kill(&mut self, _ctx: &Context) -> bool {
        true
    }
}

infinity_rs::export_gauge!(name = my_gauge, state = MyGauge, ctor = MyGauge);
