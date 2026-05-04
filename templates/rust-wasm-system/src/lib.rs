use infinity_rs::prelude::*;

pub struct MySystem;

impl System for MySystem {
    fn init(&mut self, ctx: &Context, install: &SystemInstall) -> bool {
        true
    }
    fn update(&mut self, _ctx: &Context, _dt: f32) -> bool {
        true
    }
    fn kill(&mut self, _ctx: &Context) -> bool {
        true
    }
}

infinity_rs::export_system!(name = my_system, state = MySystem, ctor = MySystem);
